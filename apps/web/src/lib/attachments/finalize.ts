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
// function (attachment-size-backfill.ts), so there is exactly one place that
// decides what a stored size is. Idempotent: finalizing a finalized row
// re-reads the object and rewrites the same numbers.
//
// The logic lives in finalize-core.ts with the object store injected; this
// module binds it to the aws-sdk client for the request graph. Default mode
// is `upload` (caps + team budget enforced, a refusal deletes object and
// row); the backfill passes `{ mode: "backfill" }`.
import type { Attachment } from "@/db/schema"
import { deleteObject, getObject, headObject } from "@/lib/storage"
import {
  finalizeAttachmentUploadWith,
  type AttachmentObjectProbe,
  type FinalizeAttachmentOptions,
} from "@/lib/attachments/finalize-core"

export type {
  AttachmentObjectProbe,
  FinalizeAttachmentMode,
  FinalizeAttachmentOptions,
} from "@/lib/attachments/finalize-core"

const storageProbe: AttachmentObjectProbe = {
  // Resolved at call time, not module load: suites that mock `@/lib/storage`
  // for the inline upload path (EXP-929 imports this module through the MCP
  // handler) need not know about `headObject`.
  head: (key) => headObject(key),
  read: async (key) => {
    const object = await getObject(key)
    const body = object?.Body
    if (!body) return null
    return new Uint8Array(await body.transformToByteArray())
  },
  remove: deleteObject,
}

export async function finalizeAttachmentUpload(
  attachmentId: string,
  options?: FinalizeAttachmentOptions
): Promise<Attachment> {
  return finalizeAttachmentUploadWith(attachmentId, storageProbe, options)
}
