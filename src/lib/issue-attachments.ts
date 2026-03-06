const attachmentPathPattern =
  /^\/api\/attachments\/(?<attachmentId>[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$/i

const markdownImagePattern = /!\[[^\]]*]\(([^)\s]+)(?:\s+"[^"]*")?\)/g

export const acceptedImageContentTypes = [
  `image/png`,
  `image/jpeg`,
  `image/webp`,
  `image/gif`,
  `image/avif`,
] as const

export const maxImageUploadBytes = 10 * 1024 * 1024

export function buildAttachmentUrl(attachmentId: string) {
  return `/api/attachments/${attachmentId}`
}

export function sanitizeAttachmentFilename(filename: string) {
  const normalized = filename
    .normalize(`NFKD`)
    .replace(/[^\x20-\x7E]/g, ``)
    .replace(/[^a-zA-Z0-9._-]+/g, `-`)
    .replace(/-+/g, `-`)
    .replace(/^-|-$/g, ``)

  return normalized || `image`
}

export function buildAttachmentStorageKey(
  issueId: string,
  attachmentId: string,
  filename: string
) {
  return `issues/${issueId}/${attachmentId}-${sanitizeAttachmentFilename(filename)}`
}

export function isAcceptedImageContentType(contentType: string) {
  return acceptedImageContentTypes.includes(
    contentType as (typeof acceptedImageContentTypes)[number]
  )
}

export function extractMarkdownImageUrls(text: string) {
  return [...text.matchAll(markdownImagePattern)].map((match) => match[1] ?? ``)
}

function getAttachmentIdFromParsedUrl(url: URL) {
  const match = attachmentPathPattern.exec(url.pathname)
  return match?.groups?.attachmentId ?? null
}

export function getAttachmentIdFromUrl(value: string, origin: string) {
  try {
    const url = new URL(value, origin)

    if (/^[a-z][a-z0-9+.-]*:\/\//i.test(value) && url.origin !== new URL(origin).origin) {
      return null
    }

    return getAttachmentIdFromParsedUrl(url)
  } catch {
    return null
  }
}

export function extractAttachmentIdsFromDescription(text: string, origin: string) {
  const attachmentIds = new Set<string>()
  const invalidUrls = new Set<string>()

  for (const imageUrl of extractMarkdownImageUrls(text)) {
    const attachmentId = getAttachmentIdFromUrl(imageUrl, origin)

    if (!attachmentId) {
      invalidUrls.add(imageUrl)
      continue
    }

    attachmentIds.add(attachmentId)
  }

  return {
    attachmentIds: [...attachmentIds],
    invalidUrls: [...invalidUrls],
  }
}

export function hasMarkdownImages(text: string) {
  return extractMarkdownImageUrls(text).length > 0
}

export function getRemovedAttachmentIds(
  previousText: string,
  nextText: string,
  origin: string
) {
  const previousIds = new Set(
    extractAttachmentIdsFromDescription(previousText, origin).attachmentIds
  )
  const nextIds = new Set(
    extractAttachmentIdsFromDescription(nextText, origin).attachmentIds
  )

  return [...previousIds].filter((attachmentId) => !nextIds.has(attachmentId))
}
