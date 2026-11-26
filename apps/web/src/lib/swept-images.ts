// #EXP-2115: the Storage sweep (attachments.sweepUnreferencedImages) only
// MARKS an unreferenced issue image (`attachments.swept_at`). It stays
// readable; this purge deletes it after the retention window, and only when
// it is STILL unreferenced. A read of the bytes clears the mark (a sealed
// chat the server cannot read, a page open somewhere), and so does a
// reference found here.

import { and, eq, inArray, isNotNull, lte } from "drizzle-orm"
import { db } from "@/db/connection"
import { attachments } from "@/db/schema"
import { appBaseUrl } from "@/lib/notification-email-policy"
import { collectSweepableIdsInTx, SWEEP_COLUMNS } from "@/lib/storage/image-references"
import { collectAttachmentStorageKeys } from "@/lib/storage/issue-attachments"

type Tx = Parameters<Parameters<(typeof db)[`transaction`]>[0]>[0]

export const SWEPT_IMAGE_RETENTION_MS = 30 * 24 * 60 * 60 * 1000

// Bounds one transaction; a backlog drains over passes.
const MAX_BATCH = 500

export async function purgeSweptImagesInTx(tx: Tx, cutoff: Date): Promise<{ storageKeys: string[] }> {
  const due = await tx
    .select({ ...SWEEP_COLUMNS, teamId: attachments.teamId })
    .from(attachments)
    .where(and(isNotNull(attachments.sweptAt), lte(attachments.sweptAt, cutoff)))
    .limit(MAX_BATCH)
    .for(`update`, { skipLocked: true })

  const gone: string[] = []
  for (const teamId of new Set(due.map((row) => row.teamId))) {
    const rows = due.filter((row) => row.teamId === teamId)
    const { sweepableIds } = await collectSweepableIdsInTx(tx, teamId, appBaseUrl(), rows)
    gone.push(...sweepableIds)
    const kept = rows.map((row) => row.id).filter((id) => !sweepableIds.has(id))
    if (kept.length > 0) {
      await tx.update(attachments).set({ sweptAt: null }).where(inArray(attachments.id, kept))
    }
  }
  if (gone.length === 0) return { storageKeys: [] }
  const deleted = await tx
    .delete(attachments)
    .where(inArray(attachments.id, gone))
    .returning({ storageKey: attachments.storageKey, posterStorageKey: attachments.posterStorageKey })
  return { storageKeys: collectAttachmentStorageKeys(deleted) }
}

/** The byte route: somebody still shows it, so it is not a leftover. */
export async function keepReadImage(attachmentId: string): Promise<void> {
  await db
    .update(attachments)
    .set({ sweptAt: null })
    .where(and(eq(attachments.id, attachmentId), isNotNull(attachments.sweptAt)))
}
