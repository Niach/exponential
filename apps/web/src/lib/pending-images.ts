// EXP-825: the composer's PENDING images — the pure half of what the steer
// session store does for a live run (`steer-session-store.ts`
// `addDraftImages`/`removeDraftImage`), lifted out so the Agent page's launch
// composer, which has no session store yet, stages images the same way: the
// accepted-type + 10 MB filter, the `MAX_STEER_IMAGES` cap, and the EXP-698
// positional `[Image #k]` markers dropped at the caret and renumbered when an
// image leaves the strip. No React, no DOM beyond the injectable object-URL
// factory, so it is unit-testable and the hook stays thin.
import {
  insertImageMarker,
  MAX_STEER_IMAGES,
  renumberImageMarkers,
} from "@/lib/steer-image-message"
import {
  isAcceptedImageContentType,
  maxImageUploadBytes,
} from "@/lib/storage/issue-attachments"

export interface PendingImage {
  file: File
  /** The preview object URL — also the strip's stable key. */
  url: string
  /** Set once the image landed server-side, so a retried submit only
   *  uploads the rest (the steer composer's contract, EXP-702). */
  uploadedId?: string
}

export interface StagedImages {
  images: PendingImage[]
  /** The draft with one marker per ADDED image inserted at the caret. */
  text: string
  /** The caret behind the last inserted marker. */
  caret: number
  /** Files refused for type or size. */
  rejected: number
  /** Accepted files that did not fit under the cap. */
  overflow: number
  added: number
}

/** Stages `files` onto `images`, inserting a positional marker for each one
 *  that fits into `text` at `caret`. The strip length BEFORE the add is the
 *  numbering base (image #1 is `images[0]`). */
export function stagePendingImages(
  images: PendingImage[],
  files: File[],
  text: string,
  caret: number,
  createUrl: (file: File) => string = (file) => URL.createObjectURL(file)
): StagedImages {
  const accepted = files.filter(
    (file) =>
      isAcceptedImageContentType(file.type) && file.size <= maxImageUploadBytes
  )
  const room = Math.max(0, MAX_STEER_IMAGES - images.length)
  const taking = accepted.slice(0, room)
  let nextText = text
  let nextCaret = Math.max(0, Math.min(caret, text.length))
  for (let i = 0; i < taking.length; i++) {
    const inserted = insertImageMarker(nextText, nextCaret, images.length + i + 1)
    nextText = inserted.text
    nextCaret = inserted.caret
  }
  return {
    images:
      taking.length > 0
        ? [...images, ...taking.map((file) => ({ file, url: createUrl(file) }))]
        : images,
    text: nextText,
    caret: nextCaret,
    rejected: files.length - accepted.length,
    overflow: accepted.length - taking.length,
    added: taking.length,
  }
}

/** Drops the image at `url`: its markers leave the draft and every higher
 *  one slides down, so the numbers keep matching the strip. The caller
 *  revokes the object URL (the store does; a test has none to revoke). */
export function dropPendingImage(
  images: PendingImage[],
  url: string,
  text: string
): { images: PendingImage[]; text: string } {
  const index = images.findIndex((image) => image.url === url)
  if (index < 0) return { images, text }
  return {
    images: images.filter((image) => image.url !== url),
    text: renumberImageMarkers(text, index + 1),
  }
}

/** `images` with `uploadedId` stamped on the entry at `url`. */
export function markPendingImageUploaded(
  images: PendingImage[],
  url: string,
  uploadedId: string
): PendingImage[] {
  return images.map((image) =>
    image.url === url ? { ...image, uploadedId } : image
  )
}
