// EXP-1291: orphan upload reclaim. Conversation files and Guide pictures are
// team-scoped `attachments` rows with no owner column set at upload (issue,
// draft and message all NULL): `sessions.send({attachmentIds})` claims a
// message's (`message_id`), and a Guide entry references a picture by id in
// its root's `sessions.guide`. An upload never sent, a revoked message's or
// a deleted agent's files (both hand them back owner-less first) and a
// picture whose session tree is gone are unreachable, yet their bytes count
// against the team's storage budget with no delete UI.
//
// This sweep deletes owner-less rows once they age past the grace window
// AND no `sessions.guide` names them, then reclaims their blobs. Mirrors
// board-trash.ts: storage keys come from DELETE ... RETURNING in the
// deleting transaction (the atomic multi-instance claim), the S3 objects go
// after the commit (idempotent).

import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { deleteStorageObjectsViaBun } from "@/lib/storage/bun-s3-cleanup"
import { collectAttachmentStorageKeys } from "@/lib/storage/issue-attachments"
import { reportSchedulerRun } from "@/lib/metrics/registry"
import { postMovedMessage } from "@/lib/sessions/messages"

type Tx = Parameters<Parameters<(typeof db)[`transaction`]>[0]>[0]

// A week of grace: an upload the composer has not sent yet, or a picture a
// Guide is about to reference, is never reclaimed under its writer.
export const ORPHAN_SESSION_ATTACHMENT_RETENTION_MS = 7 * 24 * 60 * 60 * 1000

const INITIAL_DELAY_MS = 4 * 60 * 1000
const SWEEP_INTERVAL_MS = 30 * 60 * 1000

// Bounds one transaction (and one S3 fan-out); a backlog drains over passes.
const MAX_BATCH = 500

/** The pure reclaim-due rule: owner-less, unreferenced by any Guide, aged
 *  past the window. The sweep's SQL applies the same predicate. */
export function isOrphanSessionAttachmentPurgeDue(
  row: {
    issueId: string | null
    draftId: string | null
    messageId: string | null
    createdAt: Date
  },
  referencedByGuide: boolean,
  now: Date = new Date()
): boolean {
  if (row.issueId || row.draftId || row.messageId || referencedByGuide) return false
  return row.createdAt.getTime() + ORPHAN_SESSION_ATTACHMENT_RETENTION_MS <= now.getTime()
}

/** The server-side predicate over `attachments a`, shared by the select and
 *  the re-checking delete. The Guide probe is team-scoped (an attachment
 *  and the Guide naming it share the team). */
function orphanCondition(cutoff: Date) {
  return sql`a.issue_id IS NULL AND a.draft_id IS NULL AND a.message_id IS NULL
    AND a.created_at <= ${cutoff}
    AND NOT EXISTS (
      SELECT 1 FROM sessions s
      WHERE s.team_id = a.team_id
        AND s.guide IS NOT NULL
        AND s.guide @> jsonb_build_array(jsonb_build_object('attachmentId', a.id::text))
    )`
}

export async function reclaimOrphanSessionAttachmentsInTx(
  tx: Tx,
  cutoff: Date
): Promise<{ storageKeys: string[] }> {
  const deleted = await tx.execute(sql`
    DELETE FROM attachments
    WHERE id IN (
      SELECT a.id FROM attachments a WHERE ${orphanCondition(cutoff)} LIMIT ${MAX_BATCH}
    )
    RETURNING storage_key, poster_storage_key
  `)
  const rows = (deleted.rows as Array<{ storage_key: string; poster_storage_key: string | null }>).map(
    (row) => ({ storageKey: row.storage_key, posterStorageKey: row.poster_storage_key })
  )
  return { storageKeys: collectAttachmentStorageKeys(rows) }
}

/** EXP-1252: a week-old handoff package (`sessions.handoff.blob`) nobody
 *  restored (a target that never came back, a cancelled move's upload) is
 *  dropped from its row; the keys come back for the S3 delete. EXP-1349: a
 *  move still PENDING (`move_to_device_id` set: the source never reported
 *  it) is never just stripped of its package: the move expires instead
 *  (cleared, the failure line posted) and the session stays on its source,
 *  whose worktree never left. A pending move's age is its REQUEST's
 *  (`handoff.request.requestedAt`, send.ts), never its package's: a new
 *  request carries an older, unrestored blob forward (a cancelled move's
 *  upload, a claim whose restore failed), and that blob's week must not
 *  expire the move just asked for. */
export const HANDOFF_BLOB_RETENTION_MS = 7 * 24 * 60 * 60 * 1000

/** The Conversation line of a move the sweep expired. */
export const EXPIRED_MOVE_TEXT = `Couldn't move the session: the move did not finish within a week`

export async function reclaimStaleHandoffBlobsInTx(
  tx: Tx,
  cutoff: Date
): Promise<{ storageKeys: string[]; expiredMoves: number }> {
  // A request-less pending row (none since send.ts writes the request with
  // the move) falls back to its package's age, as before.
  const expired = await tx.execute(sql`
    WITH stale AS (
      SELECT id, handoff->'blob'->>'storageKey' AS storage_key
      FROM sessions
      WHERE move_to_device_id IS NOT NULL
        AND COALESCE(
          (handoff->'request'->>'requestedAt')::timestamptz,
          (handoff->'blob'->>'uploadedAt')::timestamptz
        ) <= ${cutoff}
      LIMIT ${MAX_BATCH}
      FOR UPDATE
    )
    UPDATE sessions s
    SET move_to_device_id = NULL,
      handoff = NULL,
      handoff_restored_at = NULL,
      caption = NULL,
      updated_at = now()
    FROM stale
    WHERE s.id = stale.id
    RETURNING s.id, stale.storage_key
  `)
  const expiredRows = expired.rows as Array<{ id: string; storage_key: string | null }>
  for (const row of expiredRows) await postMovedMessage(tx, row.id, EXPIRED_MOVE_TEXT)
  const updated = await tx.execute(sql`
    WITH stale AS (
      SELECT id, handoff->'blob'->>'storageKey' AS storage_key
      FROM sessions
      WHERE jsonb_typeof(handoff->'blob') = 'object'
        AND (handoff->'blob'->>'uploadedAt')::timestamptz <= ${cutoff}
        AND move_to_device_id IS NULL
      LIMIT ${MAX_BATCH}
      FOR UPDATE
    )
    UPDATE sessions s
    SET handoff = CASE
        WHEN jsonb_typeof(s.handoff->'request') = 'object'
          THEN jsonb_build_object('request', s.handoff->'request', 'blob', NULL)
        ELSE NULL
      END,
      updated_at = now()
    FROM stale
    WHERE s.id = stale.id
    RETURNING stale.storage_key
  `)
  const storageKeys = [
    ...expiredRows,
    ...(updated.rows as Array<{ storage_key: string | null }>),
  ]
    .map((row) => row.storage_key)
    .filter((key): key is string => !!key)
  return { storageKeys, expiredMoves: expiredRows.length }
}

export async function runSessionAttachmentSweep(
  now: Date = new Date()
): Promise<{ attachmentsDeleted: number; objectsDeleted: number; handoffsDeleted: number }> {
  const cutoff = new Date(now.getTime() - ORPHAN_SESSION_ATTACHMENT_RETENTION_MS)
  const { storageKeys } = await db.transaction((tx) =>
    reclaimOrphanSessionAttachmentsInTx(tx, cutoff)
  )
  const handoffCutoff = new Date(now.getTime() - HANDOFF_BLOB_RETENTION_MS)
  const handoffs = await db.transaction((tx) => reclaimStaleHandoffBlobsInTx(tx, handoffCutoff))
  const keys = [...storageKeys, ...handoffs.storageKeys]
  if (keys.length === 0) return { attachmentsDeleted: 0, objectsDeleted: 0, handoffsDeleted: 0 }
  // Best-effort, per-key error-swallowed (bun-s3-cleanup.ts: this module
  // must not reach aws-sdk).
  await deleteStorageObjectsViaBun(keys)
  return {
    attachmentsDeleted: storageKeys.length,
    objectsDeleted: keys.length,
    handoffsDeleted: handoffs.storageKeys.length,
  }
}

let started = false
let running = false

async function sweep(): Promise<void> {
  if (running) return
  running = true
  const startMs = performance.now()
  try {
    const result = await runSessionAttachmentSweep()
    reportSchedulerRun(`session-attachment-sweep`, {
      ok: true,
      durationMs: performance.now() - startMs,
      detail: `${result.attachmentsDeleted} deleted, ${result.handoffsDeleted} handoffs, ${result.objectsDeleted} objects deleted`,
    })
    if (result.attachmentsDeleted > 0) {
      console.log(
        `[session-attachment-sweep] deleted ${result.attachmentsDeleted} orphan upload(s)`
      )
    }
  } catch (err) {
    reportSchedulerRun(`session-attachment-sweep`, {
      ok: false,
      durationMs: performance.now() - startMs,
      error: String(err),
    })
    console.error(`[session-attachment-sweep] sweep failed:`, err)
  } finally {
    running = false
  }
}

/** Start the in-process reclaim scheduler — once at boot (server-bun.ts). */
export function startSessionAttachmentSweepScheduler(): void {
  if (started) return
  started = true
  setTimeout(() => {
    void sweep()
  }, INITIAL_DELAY_MS)
  setInterval(() => {
    void sweep()
  }, SWEEP_INTERVAL_MS)
}
