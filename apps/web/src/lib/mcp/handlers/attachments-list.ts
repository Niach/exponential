// `exponential_attachments_list` (EXP-979, contract EXP-988).
//
// The registry resolves the issue, checks the OAuth grant and the caller's
// membership (the SAME rule `exponential_attachments_get` applies), then calls
// `listIssueAttachments` with the resolved issue UUID. This file owns the
// query: the issue's `attachments` rows (draft rows never carry an issue_id,
// so they never appear), newest first, paged by `limit`/`offset`, plus the
// unpaged `total`. No signed URLs here: a row's bytes are one
// `exponential_attachments_get` call away.
import { count, desc, eq } from "drizzle-orm"
import { attachments } from "@exp/db-schema"
import { db } from "@/db/connection"

export interface AttachmentListEntry {
  id: string
  filename: string
  contentType: string
  /** Real stored size. `finalizeAttachmentUpload` (EXP-955) is what makes a
   * signed-URL upload report its true size; until it ran the row reads 0. */
  sizeBytes: number
  /** ISO timestamp. */
  createdAt: string
  /** Set when the file was attached to a comment rather than the issue body. */
  commentId?: string
}

export interface AttachmentListResult {
  attachments: AttachmentListEntry[]
  total: number
}

export interface AttachmentListInput {
  /** The resolved issue UUID (never an identifier: the registry resolved it). */
  issueId: string
  limit: number
  offset: number
}

export async function listIssueAttachments(
  input: AttachmentListInput
): Promise<AttachmentListResult> {
  const { issueId, limit, offset } = input
  const [rows, total] = await Promise.all([
    db
      .select({
        id: attachments.id,
        filename: attachments.filename,
        contentType: attachments.contentType,
        sizeBytes: attachments.sizeBytes,
        createdAt: attachments.createdAt,
        commentId: attachments.commentId,
      })
      .from(attachments)
      .where(eq(attachments.issueId, issueId))
      // Newest first; the id breaks ties so paging never repeats or skips a
      // row when two uploads share a timestamp.
      .orderBy(desc(attachments.createdAt), desc(attachments.id))
      .limit(limit)
      .offset(offset),
    countIssueAttachments(issueId),
  ])
  return {
    attachments: rows.map((row) => {
      const entry: AttachmentListEntry = {
        id: row.id,
        filename: row.filename,
        contentType: row.contentType,
        sizeBytes: row.sizeBytes,
        createdAt: new Date(row.createdAt).toISOString(),
      }
      if (row.commentId) entry.commentId = row.commentId
      return entry
    }),
    total,
  }
}

/** `exponential_issues_get`'s `attachmentCount`: how many attachments rows the
 * issue carries (embedded or not, issue body or comment). */
export async function countIssueAttachments(issueId: string): Promise<number> {
  const [row] = await db
    .select({ total: count() })
    .from(attachments)
    .where(eq(attachments.issueId, issueId))
  return row?.total ?? 0
}
