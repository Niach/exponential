// EXP-825: the composer's PENDING attachments — the pure half of what the
// steer session store does for a live run (`steer-session-store.ts`
// `addDraftImages`/`removeDraftImage`), shared with the Agent page's launch
// composer, which has no session store yet. Wave D: any file rides beside
// the images. An image (accepted type, ≤ 10 MB) drops its EXP-698 positional
// `[Image #k]` marker at the caret and is renumbered when it leaves the
// strip; any other file (≤ 50 MB) is a FILE with no marker. Each kind has
// its own cap of four. No React, no DOM beyond the injectable object-URL
// factory, so it is unit-testable and the hook stays thin.
import {
  insertImageMarker,
  MAX_STEER_FILES,
  MAX_STEER_IMAGES,
  renumberImageMarkers,
  type SteerFileRef,
} from "@/lib/steer-image-message"
import {
  canonicalizeContentType,
  isAcceptedImageContentType,
  maxFileUploadBytes,
  maxImageUploadBytes,
} from "@/lib/storage/issue-attachments"

export type PendingAttachmentKind = `image` | `file`

export interface PendingAttachment {
  kind: PendingAttachmentKind
  file: File
  /** The preview object URL — also the strip's stable key. */
  url: string
  /** Set once the attachment landed server-side, so a retried submit only
   *  uploads the rest (the steer composer's contract, EXP-702). */
  uploadedId?: string
  /** The server's sanitized filename — a FILE's link text on the wire. */
  uploadedName?: string
}

/** The rejection toast ×4 (wave D). */
export const ATTACHMENT_REJECTED_TOAST = `Images up to 10 MB and files up to 50 MB can be attached`
export const IMAGE_CAP_TOAST = `Up to ${MAX_STEER_IMAGES} images per message`
export const FILE_CAP_TOAST = `Up to ${MAX_STEER_FILES} files per message`

/** `image` for an accepted image ≤ 10 MB, `file` for anything else ≤ 50 MB,
 *  null for what the upload would refuse (empty, or over its cap). */
export function classifyPendingFile(file: File): PendingAttachmentKind | null {
  if (file.size === 0) return null
  if (isAcceptedImageContentType(canonicalizeContentType(file.type))) {
    return file.size <= maxImageUploadBytes ? `image` : null
  }
  return file.size <= maxFileUploadBytes ? `file` : null
}

export interface StagedAttachments {
  images: PendingAttachment[]
  /** The draft with one marker per ADDED image inserted at the caret. */
  text: string
  /** The caret behind the last inserted marker. */
  caret: number
  /** Files refused for size (or empty). */
  rejected: number
  /** Accepted images that did not fit under the image cap. */
  overflow: number
  /** Accepted files that did not fit under the file cap. */
  fileOverflow: number
  /** Attachments (both kinds) that joined the strip. */
  added: number
}

export function pendingImageCount(images: readonly PendingAttachment[]) {
  return images.filter((entry) => entry.kind === `image`).length
}

/** Stages `files` onto `images`, inserting a positional marker for each
 *  IMAGE that fits into `text` at `caret`. The image count BEFORE the add is
 *  the numbering base (image #1 is the strip's first image). */
export function stagePendingImages(
  images: PendingAttachment[],
  files: File[],
  text: string,
  caret: number,
  createUrl: (file: File) => string = (file) => URL.createObjectURL(file)
): StagedAttachments {
  const imageBase = pendingImageCount(images)
  let imageRoom = Math.max(0, MAX_STEER_IMAGES - imageBase)
  let fileRoom = Math.max(
    0,
    MAX_STEER_FILES - (images.length - imageBase)
  )
  let rejected = 0
  let overflow = 0
  let fileOverflow = 0
  const taking: { kind: PendingAttachmentKind; file: File }[] = []
  for (const file of files) {
    const kind = classifyPendingFile(file)
    if (kind === null) rejected++
    else if (kind === `image`) {
      if (imageRoom > 0) {
        imageRoom--
        taking.push({ kind, file })
      } else overflow++
    } else if (fileRoom > 0) {
      fileRoom--
      taking.push({ kind, file })
    } else fileOverflow++
  }
  let nextText = text
  let nextCaret = Math.max(0, Math.min(caret, text.length))
  let number = imageBase
  for (const entry of taking) {
    if (entry.kind !== `image`) continue
    number++
    const inserted = insertImageMarker(nextText, nextCaret, number)
    nextText = inserted.text
    nextCaret = inserted.caret
  }
  return {
    images:
      taking.length > 0
        ? [
            ...images,
            ...taking.map(({ kind, file }) => ({
              kind,
              file,
              url: createUrl(file),
            })),
          ]
        : images,
    text: nextText,
    caret: nextCaret,
    rejected,
    overflow,
    fileOverflow,
    added: taking.length,
  }
}

/** Drops the attachment at `url`. An image's markers leave the draft and
 *  every higher one slides down, so the numbers keep matching the strip; a
 *  file has no marker. The caller revokes the object URL (the store does; a
 *  test has none to revoke). */
export function dropPendingImage(
  images: PendingAttachment[],
  url: string,
  text: string
): { images: PendingAttachment[]; text: string } {
  const index = images.findIndex((image) => image.url === url)
  if (index < 0) return { images, text }
  const entry = images[index]
  const rest = images.filter((image) => image.url !== url)
  if (entry.kind !== `image`) return { images: rest, text }
  const number = pendingImageCount(images.slice(0, index)) + 1
  return { images: rest, text: renumberImageMarkers(text, number) }
}

/** `images` with `uploadedId` (and the server's filename) stamped on the
 *  entry at `url`. */
export function markPendingImageUploaded(
  images: PendingAttachment[],
  url: string,
  uploadedId: string,
  uploadedName?: string
): PendingAttachment[] {
  return images.map((image) =>
    image.url === url
      ? {
          ...image,
          uploadedId,
          ...(uploadedName !== undefined ? { uploadedName } : {}),
        }
      : image
  )
}

/** The uploaded strip split into the wire's two blocks, in strip order. */
export function uploadedWireParts(images: readonly PendingAttachment[]): {
  imageIds: string[]
  files: SteerFileRef[]
} {
  const imageIds: string[] = []
  const files: SteerFileRef[] = []
  for (const entry of images) {
    if (!entry.uploadedId) continue
    if (entry.kind === `image`) imageIds.push(entry.uploadedId)
    else
      files.push({
        id: entry.uploadedId,
        name: entry.uploadedName ?? entry.file.name,
      })
  }
  return { imageIds, files }
}
