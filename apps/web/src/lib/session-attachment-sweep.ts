// Orphan steer-image reclaim (EXP-702). session_attachments rows point at
// their coding_sessions row with ON DELETE SET NULL, and coding_sessions rows
// DO get deleted — the staleness sweep (coding-session-sweep.ts) is the
// routine path, plus any other row delete. The image row survives that with a
// NULL session_id: nothing can ever reach it again (the read route serves it
// only through the run's transcript embed), yet its bytes keep counting
// against the team's storage budget in getTeamUsage forever, and there is no
// delete UI. On the cloud free tier (250MB) a handful of 10MB steering
// screenshots silently eats the whole allowance.
//
// This sweep deletes orphan rows once they age past the retention window and
// reclaims their S3 blobs. It is deliberately a SEPARATE sweep rather than a
// cascade folded into coding-session-sweep.ts: orphans are produced by every
// coding_sessions delete path (and by rows already orphaned before this
// shipped), so keying on the orphan state itself is the only rule that
// catches all of them — and the grace window keeps a just-swept session's
// images readable for a week rather than vanishing the instant its row goes.
//
// Mirrors board-trash.ts: storage keys are collected IN the deleting
// transaction (via DELETE ... RETURNING, so only rows this pass actually
// removed are reclaimed) and the S3 objects go after the commit — the row
// delete is the atomic multi-instance claim and S3 deletes are idempotent.

import { and, isNull, lte, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { sessionAttachments } from "@/db/schema"
import { deleteStorageObjectsViaBun } from "@/lib/storage/bun-s3-cleanup"
import { reportSchedulerRun } from "@/lib/metrics/registry"

type Tx = Parameters<Parameters<(typeof db)[`transaction`]>[0]>[0]

// A week of grace after the session row goes: long enough that a sweep of a
// crashed-but-interesting run does not evaporate its screenshots the same
// day, short enough that the budget is not held hostage.
export const ORPHAN_SESSION_ATTACHMENT_RETENTION_MS = 7 * 24 * 60 * 60 * 1000

const INITIAL_DELAY_MS = 4 * 60 * 1000
const SWEEP_INTERVAL_MS = 30 * 60 * 1000

// Bounds one transaction (and one S3 fan-out) — a backlog just drains over
// the following passes.
const MAX_BATCH = 500

// Pure reclaim-due predicate: only a row that lost its session (session_id
// NULL) and has aged past the retention window is due. A row still attached
// to a session is NEVER due, however old — the run's transcript still embeds
// it. The sweep query applies the equivalent cutoff server-side; this
// documents (and tests) the rule.
export function isOrphanSessionAttachmentPurgeDue(
  sessionId: string | null,
  createdAt: Date,
  now: Date = new Date()
): boolean {
  if (sessionId !== null) return false
  return (
    createdAt.getTime() + ORPHAN_SESSION_ATTACHMENT_RETENTION_MS <=
    now.getTime()
  )
}

// The server-side form of the predicate above, shared by the select and the
// re-checking delete so the two can never drift apart.
export function orphanSessionAttachmentCondition(cutoff: Date) {
  return and(
    isNull(sessionAttachments.sessionId),
    lte(sessionAttachments.createdAt, cutoff)
  )
}

// Delete up to MAX_BATCH due orphans inside a transaction and hand back the
// storage keys of the rows this transaction actually removed, for the
// caller's post-commit S3 cleanup. Re-applies the cutoff on the delete so a
// concurrent writer can never lose a row that stopped being due.
export async function reclaimOrphanSessionAttachmentsInTx(
  tx: Tx,
  cutoff: Date
): Promise<{ storageKeys: string[] }> {
  const due = await tx
    .select({ id: sessionAttachments.id })
    .from(sessionAttachments)
    .where(orphanSessionAttachmentCondition(cutoff))
    .limit(MAX_BATCH)

  if (due.length === 0) return { storageKeys: [] }

  const deleted = await tx
    .delete(sessionAttachments)
    .where(
      and(
        inArray(
          sessionAttachments.id,
          due.map((row) => row.id)
        ),
        orphanSessionAttachmentCondition(cutoff)
      )
    )
    .returning({ storageKey: sessionAttachments.storageKey })

  return { storageKeys: deleted.map((row) => row.storageKey) }
}

// One sweep pass, injectable clock for tests/manual runs. Returns counts for
// the caller's logging.
export async function runSessionAttachmentSweep(
  now: Date = new Date()
): Promise<{ attachmentsDeleted: number; objectsDeleted: number }> {
  const cutoff = new Date(
    now.getTime() - ORPHAN_SESSION_ATTACHMENT_RETENTION_MS
  )

  const { storageKeys } = await db.transaction((tx) =>
    reclaimOrphanSessionAttachmentsInTx(tx, cutoff)
  )
  if (storageKeys.length === 0) {
    return { attachmentsDeleted: 0, objectsDeleted: 0 }
  }

  // Best-effort, per-key error-swallowed (same as the board-trash purge).
  // Uses the Bun-native client: see bun-s3-cleanup.ts for why this module
  // must not reach aws-sdk.
  await deleteStorageObjectsViaBun(storageKeys)
  return {
    attachmentsDeleted: storageKeys.length,
    objectsDeleted: storageKeys.length,
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
      detail: `${result.attachmentsDeleted} deleted, ${result.objectsDeleted} objects deleted`,
    })
    if (result.attachmentsDeleted > 0) {
      console.log(
        `[session-attachment-sweep] deleted ${result.attachmentsDeleted} orphan steer image(s), deleted ${result.objectsDeleted} object(s)`
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

// Start the in-process reclaim scheduler — call once at boot (server-bun.ts).
// Double-start-guarded within the process. Worst-case an orphan lives
// ~SWEEP_INTERVAL_MS past its window, which is fine for a storage budget.
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
