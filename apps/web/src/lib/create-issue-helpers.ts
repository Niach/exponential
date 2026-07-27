export interface DraftImage {
  alt: string
  file: File
  id: string
  objectUrl: string
}

// EXP-297: a non-inline-image attachment queued in the create dialog. Unlike
// draft images these never enter the markdown — they are uploaded after the
// issue exists and then live purely as `attachments` rows.
export interface DraftFile {
  file: File
  id: string
}

export function revokeDraftImages(images: DraftImage[]) {
  for (const image of images) {
    URL.revokeObjectURL(image.objectUrl)
  }
}

export function buildPostCreateImageErrorMessage(
  issueIdentifier: string,
  failedImageCount?: number
) {
  if (typeof failedImageCount === `number` && failedImageCount > 0) {
    return `Created ${issueIdentifier}, but ${failedImageCount} ${
      failedImageCount === 1 ? `image` : `images`
    } failed to upload. Reopen the issue later to retry failed images.`
  }

  return `Created ${issueIdentifier}, but the image uploads could not be finalized cleanly. Reopen the issue later to retry them.`
}

// Non-fatal counterpart for the draft FILE uploads (EXP-297): the issue exists,
// only some attachments are missing.
export function buildPostCreateFileErrorMessage(
  issueIdentifier: string,
  failedFileCount: number
) {
  return `Created ${issueIdentifier}, but ${failedFileCount} ${
    failedFileCount === 1 ? `file` : `files`
  } failed to upload. Open the issue to attach them again.`
}
