// EXP-988 contract for the SIGNED half of `exponential_attachments_upload`
// (owner: EXP-929). The inline base64 path stays in the registry unchanged.
//
// Two calls, one tool (open decision 4: no new tool, the context budget):
//
//  1. `mintSignedAttachmentUpload` — the call WITHOUT `dataBase64`. The
//     registry has resolved the issue and checked grant + membership; this
//     function reserves the attachments row (issue_id, optional comment_id,
//     filename, contentType, `sizeBytes` 0 until finalized) and returns a
//     signed upload URL, its expiry and a ready curl line: exactly the shape
//     `exponential_sessions_results` returns, so an agent that knows one knows
//     both. The bytes never cross MCP.
//  2. `finalizeSignedAttachmentUpload` — a second call with `attachmentId`
//     ALONE. It calls `finalizeAttachmentUpload` (lib/attachments/finalize.ts,
//     owner EXP-955), which HEADs the stored object and writes the real
//     size/type, and returns the finished attachment (id + the markdown the
//     inline path would have returned). `exponential_comments_create` keeps
//     taking `attachmentIds`, so a finalized signed upload covers the comment
//     case with no new parameter there.
import { NotImplementedError } from "./not-implemented"

export interface SignedAttachmentUploadInput {
  /** The resolved issue UUID. */
  issueId: string
  teamId: string
  boardId: string
  userId: string
  filename: string
  contentType: string
  commentId?: string
  /** The public origin the upload URL is built on (`appBaseUrl()` behind a
   * TLS-terminating proxy, else the request origin — same rule as
   * `exponential_attachments_get`'s downloadUrl). */
  origin: string
}

/** Byte-compatible with `exponential_sessions_results`' upload grant. */
export interface SignedAttachmentUploadResult {
  attachmentId: string
  uploadUrl: string
  /** ISO timestamp. */
  expiresAt: string
  curl: string
}

export interface FinalizedAttachmentResult {
  id: string
  url: string
  filename: string
  contentType: string
  sizeBytes: number
  /** Images embed as `![]()`, inline media as a plain link; other files have
   * no markdown (they live in the issue's Files list). */
  markdown?: string
  width: number | null
  height: number | null
  durationMs: number | null
}

export async function mintSignedAttachmentUpload(
  _input: SignedAttachmentUploadInput
): Promise<SignedAttachmentUploadResult> {
  throw new NotImplementedError(
    `exponential_attachments_upload without dataBase64`,
    `EXP-929`
  )
}

export async function finalizeSignedAttachmentUpload(_input: {
  attachmentId: string
  userId: string
}): Promise<FinalizedAttachmentResult> {
  throw new NotImplementedError(
    `exponential_attachments_upload({ attachmentId })`,
    `EXP-929`
  )
}
