export interface DraftImage {
  alt: string
  file: File
  id: string
  objectUrl: string
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
