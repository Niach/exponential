// Project trash purge sweep. Soft-deleted projects (projects.deleted_at) are
// hard-deleted here once they age past PROJECT_TRASH_RETENTION_HOURS, cascading
// to all their issues/comments/attachments rows and reclaiming the attachment
// blobs from S3. Mirrors notification-email-digest.ts's in-process scheduler
// shell; started once from server-bun.ts. Multi-instance safe by construction:
// the row delete is the atomic claim (an already-purged row deletes 0 rows) and
// S3 deletes are idempotent.

import { and, eq, isNotNull, lte } from "drizzle-orm"
import { db } from "@/db/connection"
import { attachments, projects } from "@/db/schema"
import { PROJECT_TRASH_RETENTION_MS } from "@exp/db-schema/domain"
import { deleteStorageObjects } from "@/lib/storage/issue-attachment-cleanup"
import { invalidatePublicProjectCache } from "@/lib/workspace-membership"

type Tx = Parameters<Parameters<(typeof db)[`transaction`]>[0]>[0]

const INITIAL_DELAY_MS = 90 * 1000
const SWEEP_INTERVAL_MS = 30 * 60 * 1000

// Pure purge-due predicate: a trashed project is due once deletedAt + the
// retention window has passed. The sweep query applies the equivalent cutoff
// server-side; this documents (and tests) the rule.
export function isProjectPurgeDue(
  deletedAt: Date | null,
  now: Date = new Date()
): boolean {
  if (!deletedAt) return false
  return deletedAt.getTime() + PROJECT_TRASH_RETENTION_MS <= now.getTime()
}

// Hard-delete one trashed project inside a transaction. Collects the attachment
// storage keys BEFORE the row delete (the cascade removes the attachment rows),
// then re-checks the cutoff atomically so a concurrent restore wins the race:
// if the delete touches 0 rows, the caller skips the S3 cleanup.
export async function purgeProjectInTx(
  tx: Tx,
  projectId: string,
  cutoff: Date
): Promise<{ purged: boolean; storageKeys: string[] }> {
  const attachmentRows = await tx
    .select({ storageKey: attachments.storageKey })
    .from(attachments)
    .where(eq(attachments.projectId, projectId))

  const deleted = await tx
    .delete(projects)
    .where(
      and(
        eq(projects.id, projectId),
        isNotNull(projects.deletedAt),
        lte(projects.deletedAt, cutoff)
      )
    )
    .returning({ id: projects.id })

  if (deleted.length === 0) return { purged: false, storageKeys: [] }
  return { purged: true, storageKeys: attachmentRows.map((r) => r.storageKey) }
}

// One sweep pass, injectable clock for tests/manual runs. Returns counts for
// the caller's logging.
export async function runProjectPurgeSweep(
  now: Date = new Date()
): Promise<{ projectsPurged: number; objectsDeleted: number }> {
  const cutoff = new Date(now.getTime() - PROJECT_TRASH_RETENTION_MS)

  const due = await db
    .select({ id: projects.id })
    .from(projects)
    .where(
      and(
        isNotNull(projects.deletedAt),
        lte(projects.deletedAt, cutoff),
        // Protected projects (the dogfood board) are never purged, defensively —
        // the delete guard already refuses to trash them.
        eq(projects.isProtected, false)
      )
    )

  let projectsPurged = 0
  let objectsDeleted = 0
  for (const { id } of due) {
    const { purged, storageKeys } = await db.transaction((tx) =>
      purgeProjectInTx(tx, id, cutoff)
    )
    if (!purged) continue
    projectsPurged += 1
    if (storageKeys.length > 0) {
      // Best-effort, per-key error-swallowed (same as the attachment cleanup
      // path). Also closes the pre-existing cascade-orphan gap on project
      // delete — blobs used to be left behind.
      await deleteStorageObjects(storageKeys)
      objectsDeleted += storageKeys.length
    }
  }

  if (projectsPurged > 0) invalidatePublicProjectCache()
  return { projectsPurged, objectsDeleted }
}

let started = false
let running = false

async function sweep(): Promise<void> {
  if (running) return
  running = true
  try {
    const result = await runProjectPurgeSweep()
    if (result.projectsPurged > 0) {
      console.log(
        `[project-trash] purged ${result.projectsPurged} project(s), deleted ${result.objectsDeleted} attachment object(s)`
      )
    }
  } catch (err) {
    console.error(`[project-trash] sweep failed:`, err)
  } finally {
    running = false
  }
}

// Start the in-process purge scheduler — call once at boot (server-bun.ts).
// Double-start-guarded within the process. Worst-case a project is purged
// ~SWEEP_INTERVAL_MS after its 48h window elapses, which is fine.
export function startProjectTrashScheduler(): void {
  if (started) return
  started = true
  setTimeout(() => {
    void sweep()
  }, INITIAL_DELAY_MS)
  setInterval(() => {
    void sweep()
  }, SWEEP_INTERVAL_MS)
}
