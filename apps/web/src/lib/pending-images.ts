// EXP-825: the composer's PENDING attachments — the pure half of what the
// steer session store does for a live run (`steer-session-store.ts`
// `addDraftImages`/`removeDraftImage`), shared with the Agent page's launch
// composer, which has no session store yet. Wave D: any file rides beside
// the images. An image (accepted type, ≤ 10 MB) drops its EXP-698 positional
// `[Image #k]` marker at the caret and is renumbered when it leaves the
// strip; any other file (≤ 50 MB) is a FILE with no marker. Each kind has
// its own cap of four. No React, no DOM beyond the injectable object-URL
// factory, so it is unit-testable and the hook stays thin.
import { type SteerFileRef } from "@/lib/steer-image-message"
import {
  ATTACHMENT_REJECTED_TOAST,
  dropAttachment,
  FILE_CAP_TOAST,
  IMAGE_CAP_TOAST,
  stageAttachments,
  stageToasts,
} from "@exp/domain-contract/composer-attachments"
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

/** The rejection toasts ×4 (composer-attachments.json). */
export { ATTACHMENT_REJECTED_TOAST, FILE_CAP_TOAST, IMAGE_CAP_TOAST, stageToasts }

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
 *  IMAGE that fits into `text` at `caret` (composer-attachments.json
 *  `stage`: image #1 is the strip's first image). */
export function stagePendingImages(
  images: PendingAttachment[],
  files: File[],
  text: string,
  caret: number,
  createUrl: (file: File) => string = (file) => URL.createObjectURL(file)
): StagedAttachments {
  const staged = stageAttachments({
    strip: images.map((entry) => entry.kind),
    add: files.map((file) => classifyPendingFile(file) ?? `rejected`),
    text,
    caret,
  })
  const taking = files.flatMap((file, index) => {
    if (!staged.taken[index]) return []
    const kind = classifyPendingFile(file) as PendingAttachmentKind
    return [{ kind, file, url: createUrl(file) }]
  })
  return {
    images: taking.length > 0 ? [...images, ...taking] : images,
    text: staged.text,
    caret: staged.caret,
    rejected: staged.rejected,
    overflow: staged.overflow,
    fileOverflow: staged.fileOverflow,
    added: taking.length,
  }
}

/** Drops the attachment at `url`. An image's markers leave the draft and
 *  every higher one slides down, so the numbers keep matching the strip; a
 *  file has no marker (composer-attachments.json `drop`). The caller revokes
 *  the object URL. */
export function dropPendingImage(
  images: PendingAttachment[],
  url: string,
  text: string,
  caret: number = text.length
): { images: PendingAttachment[]; text: string; caret: number } {
  const at = images.findIndex((image) => image.url === url)
  if (at < 0) return { images, text, caret: Math.max(0, Math.min(caret, text.length)) }
  const dropped = dropAttachment({ strip: images.map((entry) => entry.kind), at, text, caret })
  return {
    images: images.filter((image) => image.url !== url),
    text: dropped.text,
    caret: dropped.caret,
  }
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
