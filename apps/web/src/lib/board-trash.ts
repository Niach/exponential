// Board trash purge sweep. Soft-deleted boards (boards.deleted_at) are
// hard-deleted here once they age past BOARD_TRASH_RETENTION_HOURS, cascading
// to all their issues/comments/attachments rows and reclaiming the attachment
// blobs from S3. Mirrors notification-email-digest.ts's in-process scheduler
// shell; started once from server-bun.ts. Multi-instance safe by construction:
// the row delete is the atomic claim (an already-purged row deletes 0 rows) and
// S3 deletes are idempotent.

import { and, eq, isNotNull, lte } from "drizzle-orm"
import { db } from "@/db/connection"
import { attachments, boards } from "@/db/schema"
import { BOARD_TRASH_RETENTION_MS } from "@exp/db-schema/domain"
import { deleteStorageObjectsViaBun } from "@/lib/storage/bun-s3-cleanup"

type Tx = Parameters<Parameters<(typeof db)[`transaction`]>[0]>[0]

const INITIAL_DELAY_MS = 90 * 1000
const SWEEP_INTERVAL_MS = 30 * 60 * 1000

// Pure purge-due predicate: a trashed board is due once deletedAt + the
// retention window has passed. The sweep query applies the equivalent cutoff
// server-side; this documents (and tests) the rule.
export function isBoardPurgeDue(
  deletedAt: Date | null,
  now: Date = new Date()
): boolean {
  if (!deletedAt) return false
  return deletedAt.getTime() + BOARD_TRASH_RETENTION_MS <= now.getTime()
}

// Hard-delete one trashed board inside a transaction. Collects the attachment
// storage keys BEFORE the row delete (the cascade removes the attachment rows),
// then re-checks the cutoff atomically so a concurrent restore wins the race:
// if the delete touches 0 rows, the caller skips the S3 cleanup.
export async function purgeBoardInTx(
  tx: Tx,
  boardId: string,
  cutoff: Date
): Promise<{ purged: boolean; storageKeys: string[] }> {
  const attachmentRows = await tx
    .select({ storageKey: attachments.storageKey })
    .from(attachments)
    .where(eq(attachments.boardId, boardId))

  const deleted = await tx
    .delete(boards)
    .where(
      and(
        eq(boards.id, boardId),
        isNotNull(boards.deletedAt),
        lte(boards.deletedAt, cutoff)
      )
    )
    .returning({ id: boards.id })

  if (deleted.length === 0) return { purged: false, storageKeys: [] }
  return { purged: true, storageKeys: attachmentRows.map((r) => r.storageKey) }
}

// One sweep pass, injectable clock for tests/manual runs. Returns counts for
// the caller's logging.
export async function runBoardPurgeSweep(
  now: Date = new Date()
): Promise<{ boardsPurged: number; objectsDeleted: number }> {
  const cutoff = new Date(now.getTime() - BOARD_TRASH_RETENTION_MS)

  const due = await db
    .select({ id: boards.id })
    .from(boards)
    .where(
      and(
        isNotNull(boards.deletedAt),
        lte(boards.deletedAt, cutoff),
        // Protected boards (the dogfood board) are never purged, defensively —
        // the delete guard already refuses to trash them.
        eq(boards.isProtected, false)
      )
    )

  let boardsPurged = 0
  let objectsDeleted = 0
  for (const { id } of due) {
    const { purged, storageKeys } = await db.transaction((tx) =>
      purgeBoardInTx(tx, id, cutoff)
    )
    if (!purged) continue
    boardsPurged += 1
    if (storageKeys.length > 0) {
      // Best-effort, per-key error-swallowed (same as the attachment cleanup
      // path). Also closes the pre-existing cascade-orphan gap on board
      // delete — blobs used to be left behind. Uses the Bun-native client:
      // see bun-s3-cleanup.ts for why this module must not reach aws-sdk.
      await deleteStorageObjectsViaBun(storageKeys)
      objectsDeleted += storageKeys.length
    }
  }

  return { boardsPurged, objectsDeleted }
}

let started = false
let running = false

async function sweep(): Promise<void> {
  if (running) return
  running = true
  try {
    const result = await runBoardPurgeSweep()
    if (result.boardsPurged > 0) {
      console.log(
        `[board-trash] purged ${result.boardsPurged} board(s), deleted ${result.objectsDeleted} attachment object(s)`
      )
    }
  } catch (err) {
    console.error(`[board-trash] sweep failed:`, err)
  } finally {
    running = false
  }
}

// Start the in-process purge scheduler — call once at boot (server-bun.ts).
// Double-start-guarded within the process. Worst-case a board is purged
// ~SWEEP_INTERVAL_MS after its 48h window elapses, which is fine.
export function startBoardTrashScheduler(): void {
  if (started) return
  started = true
  setTimeout(() => {
    void sweep()
  }, INITIAL_DELAY_MS)
  setInterval(() => {
    void sweep()
  }, SWEEP_INTERVAL_MS)
}
