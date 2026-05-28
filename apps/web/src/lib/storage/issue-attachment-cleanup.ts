import { and, eq, inArray } from "drizzle-orm"
import { attachments } from "@/db/schema"
import { deleteObject } from "@/lib/storage"
import {
  extractAttachmentIdsFromDescription,
  getRemovedAttachmentIds,
} from "@/lib/storage/issue-attachments"

// Grace window before a never-referenced attachment is reclaimed. Must comfortably
// exceed the gap between an upload and the description save that references it —
// mobile clients can upload an image and only persist the description on dismiss.
const ORPHAN_GRACE_MS = 24 * 60 * 60 * 1000

type Tx = Parameters<
  // eslint-disable-next-line quotes
  Parameters<typeof import("@/db/connection").db.transaction>[0]
>[0]

export async function deleteStorageObjects(keys: string[]): Promise<void> {
  if (keys.length === 0) return
  await Promise.allSettled(
    keys.map(async (storageKey) => {
      try {
        await deleteObject(storageKey)
      } catch (error) {
        console.error(`Failed to delete attachment object`, error)
      }
    })
  )
}

export async function collectAndDeleteRemovedAttachmentsInTx(
  tx: Tx,
  issueId: string,
  previousText: string,
  nextText: string,
  requestUrl: string
): Promise<string[]> {
  const removedAttachmentIds = getRemovedAttachmentIds(
    previousText,
    nextText,
    requestUrl
  )

  if (removedAttachmentIds.length === 0) return []

  const removedAttachments = await tx
    .select({
      id: attachments.id,
      storageKey: attachments.storageKey,
    })
    .from(attachments)
    .where(
      and(
        eq(attachments.issueId, issueId),
        inArray(attachments.id, removedAttachmentIds)
      )
    )

  if (removedAttachments.length === 0) return []

  await tx.delete(attachments).where(
    and(
      eq(attachments.issueId, issueId),
      inArray(
        attachments.id,
        removedAttachments.map((attachment) => attachment.id)
      )
    )
  )

  return removedAttachments.map((attachment) => attachment.storageKey)
}

/**
 * Reclaims attachments that belong to an issue but are no longer (or were never)
 * referenced by its current description, once they are older than the grace
 * window. This complements {@link collectAndDeleteRemovedAttachmentsInTx} (which
 * only handles images removed from a *saved* description) by also catching
 * never-referenced uploads left behind by an abandoned or interrupted edit.
 * Piggy-backs on issues.update so no separate cron is required.
 */
export async function collectAndDeleteUnreferencedAttachmentsInTx(
  tx: Tx,
  issueId: string,
  currentText: string,
  requestUrl: string,
  now: number = Date.now()
): Promise<string[]> {
  const referencedIds = new Set(
    extractAttachmentIdsFromDescription(currentText, requestUrl).attachmentIds
  )
  const cutoff = new Date(now - ORPHAN_GRACE_MS)

  const rows = await tx
    .select({
      id: attachments.id,
      storageKey: attachments.storageKey,
      createdAt: attachments.createdAt,
    })
    .from(attachments)
    .where(eq(attachments.issueId, issueId))

  const orphans = rows.filter(
    (row) => !referencedIds.has(row.id) && row.createdAt < cutoff
  )
  if (orphans.length === 0) return []

  await tx.delete(attachments).where(
    and(
      eq(attachments.issueId, issueId),
      inArray(
        attachments.id,
        orphans.map((orphan) => orphan.id)
      )
    )
  )

  return orphans.map((orphan) => orphan.storageKey)
}

export async function collectIssueAttachmentStorageKeysInTx(
  tx: Tx,
  issueId: string
): Promise<string[]> {
  const attachmentRows = await tx
    .select({ storageKey: attachments.storageKey })
    .from(attachments)
    .where(eq(attachments.issueId, issueId))
  return attachmentRows.map((row) => row.storageKey)
}
