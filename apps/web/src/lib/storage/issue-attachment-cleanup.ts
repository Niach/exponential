import { eq } from "drizzle-orm"
import { attachments } from "@/db/schema"
import { deleteObject } from "@/lib/storage"

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

// EXP-297 removed the automatic attachment GC that used to piggy-back on
// issues.update (removed-from-description + unreferenced-orphan sweeps).
// Attachments now live until someone deletes them explicitly
// (attachments.delete / the owner-only team storage manager) or until the
// owning issue/board/team is destroyed — which is what the collector below
// serves.

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
