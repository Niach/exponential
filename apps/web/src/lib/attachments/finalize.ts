// EXP-988 contract: ONE finalize function for every signed-URL attachment
// upload (owner: EXP-955; called by EXP-929's MCP handler; EXP-979 only reads
// the `sizeBytes` it writes).
//
// A signed upload puts the bytes into storage WITHOUT the server seeing them,
// so the row reserved at mint time cannot know its size — that is the
// zero-bytes bug EXP-955 chases: rows whose `size_bytes` stayed 0 because the
// signed path never recorded one. `finalizeAttachmentUpload` closes it:
//
//  1. HEAD the stored object (`storageKey`) — a missing object is an error
//     (the upload never happened or is still in flight), never a 0 write;
//  2. write the real `sizeBytes` and the stored `contentType` back onto the
//     row (and, for images/media, probe width/height/duration like the inline
//     path does);
//  3. return the finished row.
//
// EXP-955 also BACKFILLS existing rows with `sizeBytes = 0` through the same
// function, so there is exactly one place that decides what a stored size is.
// Idempotent: finalizing a finalized row re-reads the object and rewrites the
// same numbers.
import type { Attachment } from "@/db/schema"

export async function finalizeAttachmentUpload(
  _attachmentId: string
): Promise<Attachment> {
  throw new Error(
    `finalizeAttachmentUpload is not implemented yet (EXP-955 owns it)`
  )
}
