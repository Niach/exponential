// EXP-955: the ONE decision of what a stored attachment's size is.
//
// `finalizeAttachmentUploadWith` is `finalizeAttachmentUpload` (finalize.ts,
// the EXP-988 contract) with the object store injected: the request graph
// hands it the aws-sdk client from `@/lib/storage`, the boot-time backfill
// sweep (attachment-size-backfill.ts, in the server ENTRY graph) hands it
// Bun's built-in S3 client instead — the entry chunk group must never reach
// @aws-sdk/client-s3 (see storage/bun-s3-cleanup.ts). Both paths run this
// exact function, so a signed upload, a legacy row and a re-finalize all
// agree on the number.
//
// Two modes (`options.mode`):
//  - `upload` (default) = the signed-URL flow. The row was reserved at mint
//    with `sizeBytes` 0 and the bytes went straight to storage; this is the
//    first time the server can enforce the per-type upload cap and the team's
//    storage budget. A refusal DELETES the stored object and the reserved
//    row: nothing over the cap ever becomes servable or counted, and the
//    agent gets a clean error to retry with a smaller file.
//  - `backfill` = healing rows that already exist with `sizeBytes` 0 (rows a
//    signed upload never finalized, or pre-EXP-955 legacy rows). Nothing is
//    refused or deleted: the bytes are already there and the point is honest
//    accounting.
import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { attachments, type Attachment } from "@/db/schema"
import { assertWithinStorageLimit } from "@/lib/billing"
import {
  canonicalizeContentType,
  getMaxUploadBytesForContentType,
  isAcceptedImageContentType,
  isInlineMediaContentType,
  maxFileUploadBytes,
  maxImageUploadBytes,
} from "@/lib/storage/issue-attachments"
import { getImageDimensions } from "@/lib/storage/image-dimensions"
import {
  getVideoMetadata,
  isProbeableVideoContentType,
} from "@/lib/storage/video-metadata"

export interface StoredObjectInfo {
  sizeBytes: number
  /** The stored Content-Type, or null when the store recorded none. */
  contentType: string | null
}

/** The object-store surface finalize needs; see the module comment. */
export interface AttachmentObjectProbe {
  /** HEAD: size + type, or null when the key does not exist. */
  head(key: string): Promise<StoredObjectInfo | null>
  /** Full read for the image/media header probe; null when missing. */
  read(key: string): Promise<Uint8Array | null>
  /** Best-effort delete (only the `upload` mode's refusal path uses it). */
  remove(key: string): Promise<void>
}

export type FinalizeAttachmentMode = `upload` | `backfill`

export interface FinalizeAttachmentOptions {
  mode?: FinalizeAttachmentMode
}

/** A finalize refused because the stored bytes exceed the per-type cap. */
export function uploadCapMessage(contentType: string) {
  return isAcceptedImageContentType(contentType)
    ? `Images must be ${maxImageUploadBytes / (1024 * 1024)} MB or smaller`
    : `Files must be ${maxFileUploadBytes / (1024 * 1024)} MB or smaller`
}

async function loadRow(attachmentId: string): Promise<Attachment | null> {
  const [row] = await db
    .select()
    .from(attachments)
    .where(eq(attachments.id, attachmentId))
    .limit(1)
  return row ?? null
}

/**
 * The upload-mode refusal: the bytes go, the reserved row goes, and the
 * caller gets the error. Storage deletes are best-effort (the row delete is
 * what makes the object unreachable; an orphaned blob is a sweep's problem,
 * not a correctness one).
 */
async function refuse(
  row: Attachment,
  probe: AttachmentObjectProbe,
  error: TRPCError
): Promise<never> {
  try {
    await probe.remove(row.storageKey)
  } catch (removeError) {
    console.error(
      `[attachments] failed to delete refused upload object ${row.storageKey}`,
      removeError
    )
  }
  await db.delete(attachments).where(eq(attachments.id, row.id))
  throw error
}

/**
 * Finalize one attachment row against its stored object. HEAD → real size +
 * stored type → (images/media with no dimensions yet) header probe → row
 * update → the finished row. Idempotent: a finalized row re-reads the object
 * and rewrites the same numbers. A HEAD miss throws and leaves the row
 * untouched so a later call can finalize it once the bytes land.
 */
export async function finalizeAttachmentUploadWith(
  attachmentId: string,
  probe: AttachmentObjectProbe,
  options: FinalizeAttachmentOptions = {}
): Promise<Attachment> {
  const mode = options.mode ?? `upload`
  const row = await loadRow(attachmentId)
  if (!row) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Attachment not found` })
  }

  const head = await probe.head(row.storageKey)
  if (!head) {
    // Not an error state for the ROW — the upload has not landed (yet).
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `Attachment ${attachmentId} has no uploaded bytes yet`,
    })
  }

  // The stored type wins when the store has one (the uploader's PUT set it);
  // a store that recorded nothing keeps the type the row was reserved with.
  const contentType = canonicalizeContentType(
    head.contentType?.trim() ? head.contentType : row.contentType
  )
  const sizeBytes = Math.max(0, Math.floor(head.sizeBytes))

  if (mode === `upload`) {
    if (sizeBytes === 0) {
      await refuse(
        row,
        probe,
        new TRPCError({ code: `BAD_REQUEST`, message: `File is empty` })
      )
    }
    if (sizeBytes > getMaxUploadBytesForContentType(contentType)) {
      await refuse(
        row,
        probe,
        new TRPCError({
          code: `BAD_REQUEST`,
          message: uploadCapMessage(contentType),
        })
      )
    }
    // The team budget counts the row's CURRENT size already; only the delta
    // is new bytes, so a re-finalize of a finished row never double-charges.
    if (sizeBytes > row.sizeBytes) {
      try {
        await assertWithinStorageLimit(row.teamId, sizeBytes - row.sizeBytes)
      } catch (error) {
        if (error instanceof TRPCError) await refuse(row, probe, error)
        throw error
      }
    }
  }

  // Dimensions/duration like the inline path: probed from the bytes for the
  // accepted raster types and the MP4/MOV family. Only when the row has none
  // yet — a client-supplied set (EXP-824 webm) is never overwritten with
  // nulls, and a finished row is not re-read for nothing.
  const isImage = isAcceptedImageContentType(contentType)
  const isMedia = isInlineMediaContentType(contentType)
  const needsProbe =
    (isImage || (isMedia && isProbeableVideoContentType(contentType))) &&
    row.width === null &&
    row.height === null &&
    (!isMedia || row.durationMs === null)
  let width = row.width
  let height = row.height
  let durationMs = row.durationMs
  if (needsProbe) {
    const bytes = await probe.read(row.storageKey)
    if (bytes) {
      if (isImage) {
        const dimensions = getImageDimensions(bytes)
        width = dimensions?.width ?? null
        height = dimensions?.height ?? null
      } else {
        const media = getVideoMetadata(bytes)
        width = media?.width ?? null
        height = media?.height ?? null
        durationMs = media?.durationMs ?? null
      }
    }
  }

  const [updated] = await db
    .update(attachments)
    .set({
      sizeBytes,
      contentType,
      width,
      height,
      durationMs,
      updatedAt: new Date(),
    })
    .where(eq(attachments.id, row.id))
    .returning()

  if (!updated) {
    // Deleted between the read and the write (a concurrent member delete).
    throw new TRPCError({ code: `NOT_FOUND`, message: `Attachment not found` })
  }
  return updated
}
