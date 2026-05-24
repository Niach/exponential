import { and, eq, inArray } from "drizzle-orm"
import { attachments } from "@/db/schema"
import { deleteObject } from "@/lib/storage"
import { getRemovedAttachmentIds } from "@/lib/storage/issue-attachments"

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
