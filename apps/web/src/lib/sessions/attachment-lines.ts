import { buildAttachmentUrl, isAcceptedImageContentType } from "@/lib/storage/issue-attachments"

/** The line a message's attachment reads as in its body: an image embed for
 *  an accepted image type, else a plain link (fixtures/session-brief.json
 *  FEED-180: a sealed first message's `sealedAttachments`). */
export function attachmentLine(row: { id: string; filename: string; contentType: string }): string {
  const url = buildAttachmentUrl(row.id)
  const name = row.filename.replace(/[[\]]/g, ``)
  return isAcceptedImageContentType(row.contentType) ? `![${name}](${url})` : `[${name}](${url})`
}
