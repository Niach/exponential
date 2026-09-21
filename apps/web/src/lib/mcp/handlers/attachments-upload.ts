// EXP-929: the SIGNED half of `exponential_attachments_upload` (contract:
// EXP-988). The inline base64 path stays in the registry unchanged.
//
// Two calls, one tool (no new tool, the context budget):
//
//  1. `mintSignedAttachmentUpload` — the call WITHOUT `dataBase64`. The
//     registry has resolved the issue and checked grant + membership; this
//     function pre-allocates the attachment id and returns a signed upload
//     URL, its expiry and a ready curl line: the shape
//     `exponential_sessions_results` returns, so an agent that knows one knows
//     both. The bytes never cross MCP. Nothing is written here: the row is
//     inserted by `/api/attachment-uploads/$token` when the PUT lands, with
//     the real size and the inline path's probes, so an abandoned mint leaves
//     no 0-byte phantom in the issue's Files list (the token simply expires).
//  2. `finalizeSignedAttachmentUpload` — a second call with `attachmentId`
//     ALONE. It checks the row is the caller's, runs `finalizeAttachmentUpload`
//     (lib/attachments/finalize.ts, owner EXP-955: HEADs the stored object and
//     confirms size/type on the row, idempotent) and returns the finished
//     attachment (id + the markdown the inline path would have returned).
//     `exponential_comments_create` keeps taking `attachmentIds`, so a
//     finalized signed upload covers the comment case with no new parameter
//     there; a `commentId` at mint time attaches straight to an existing
//     comment of the caller's instead.
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { attachments, comments, type Attachment } from "@/db/schema"
import { finalizeAttachmentUpload } from "@/lib/attachments/finalize"
import { mintAttachmentUploadToken } from "@/lib/storage/attachment-upload-token"
import {
  isAcceptedImageContentType,
  isInlineMediaContentType,
} from "@/lib/storage/issue-attachments"

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

/** POSIX single-quoting: safe for any filename `sanitizeUploadFilename` lets
 *  through (Unicode, spaces, quotes, `$`). */
function shellQuote(value: string) {
  return `'${value.replace(/'/g, `'\\''`)}'`
}

export async function mintSignedAttachmentUpload(
  input: SignedAttachmentUploadInput
): Promise<SignedAttachmentUploadResult> {
  if (input.commentId !== undefined) {
    // The same gate `comments.create({ attachmentIds })` applies when linking
    // after the fact: the comment is on this issue and the caller wrote it.
    const [comment] = await db
      .select({ issueId: comments.issueId, authorId: comments.authorId })
      .from(comments)
      .where(eq(comments.id, input.commentId))
      .limit(1)
    if (!comment || comment.issueId !== input.issueId) {
      throw new Error(`commentId must name a comment on this issue.`)
    }
    if (comment.authorId !== input.userId) {
      throw new Error(
        `Only the comment's author can attach files to it; leave commentId off and pass the finalized attachmentId to exponential_comments_create instead.`
      )
    }
  }

  const attachmentId = crypto.randomUUID()
  const { token, expiresAt } = mintAttachmentUploadToken({
    attachmentId,
    issueId: input.issueId,
    boardId: input.boardId,
    teamId: input.teamId,
    userId: input.userId,
    filename: input.filename,
    contentType: input.contentType,
    commentId: input.commentId,
  })
  const uploadUrl = `${input.origin}/api/attachment-uploads/${token}`
  return {
    attachmentId,
    uploadUrl,
    expiresAt: expiresAt.toISOString(),
    // `-T` PUTs the file as the raw body with its Content-Length; the route
    // takes the type from the token, so no header is needed.
    curl: `curl -sS -T ${shellQuote(input.filename)} "${uploadUrl}"`,
  }
}

export function finalizedAttachmentResult(
  row: Pick<
    Attachment,
    | `id`
    | `url`
    | `filename`
    | `contentType`
    | `sizeBytes`
    | `width`
    | `height`
    | `durationMs`
  >
): FinalizedAttachmentResult {
  const isImage = isAcceptedImageContentType(row.contentType)
  const isMedia = isInlineMediaContentType(row.contentType)
  return {
    id: row.id,
    url: row.url,
    filename: row.filename,
    contentType: row.contentType,
    sizeBytes: row.sizeBytes,
    // Images embed as `![]()`; video/audio (EXP-824) as a plain link on its
    // own paragraph. Other files are NOT markdown-embeddable — they live in
    // the issue's Files list.
    ...(isImage
      ? { markdown: `![${row.filename}](${row.url})` }
      : isMedia
        ? { markdown: `[${row.filename}](${row.url})` }
        : {}),
    width: row.width,
    height: row.height,
    durationMs: row.durationMs,
  }
}

export async function finalizeSignedAttachmentUpload(input: {
  attachmentId: string
  userId: string
}): Promise<FinalizedAttachmentResult> {
  const [row] = await db
    .select({ id: attachments.id, uploaderId: attachments.uploaderId })
    .from(attachments)
    .where(eq(attachments.id, input.attachmentId))
    .limit(1)
  if (!row) {
    throw new Error(
      `No upload has landed for this attachmentId yet. Run the curl line from the first call (the link is good for 10 minutes), then finalize.`
    )
  }
  // Only the uploader finalizes: the row is what the caller's own signed URL
  // wrote, and finalizing is a write (size/type re-confirmed on the row).
  if (row.uploaderId !== input.userId) {
    throw new Error(`This attachment was not uploaded by you.`)
  }
  const finalized = await finalizeAttachmentUpload(input.attachmentId)
  return finalizedAttachmentResult(finalized)
}
