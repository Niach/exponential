// EXP-988 contract for `exponential_attachments_list` (owner: EXP-979).
//
// The registry resolves the issue, checks the OAuth grant and the caller's
// membership (the SAME rule `exponential_attachments_get` applies), then calls
// `listIssueAttachments` with the resolved issue UUID. This file owns the
// query: the issue's `attachments` rows (draft rows never carry an issue_id,
// so they never appear), newest first, paged by `limit`/`offset`, plus the
// unpaged `total`. No signed URLs here: a row's bytes are one
// `exponential_attachments_get` call away.
import { NotImplementedError } from "./not-implemented"

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
  _input: AttachmentListInput
): Promise<AttachmentListResult> {
  throw new NotImplementedError(`exponential_attachments_list`, `EXP-979`)
}

/** `exponential_issues_get`'s `attachmentCount`: how many attachments rows the
 * issue carries. The contract returns 0; EXP-979 fills it with the count. */
export async function countIssueAttachments(_issueId: string): Promise<number> {
  return 0
}
