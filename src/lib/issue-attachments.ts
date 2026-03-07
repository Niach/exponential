const attachmentPathPattern =
  /^\/api\/attachments\/(?<attachmentId>[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$/i

const markdownImagePattern = /!\[([^\]]*)]\(([^)\s]+)(?:\s+"[^"]*")?\)/g

export const acceptedImageContentTypes = [
  `image/png`,
  `image/jpeg`,
  `image/webp`,
  `image/gif`,
  `image/avif`,
] as const

export const maxImageUploadBytes = 10 * 1024 * 1024

export interface MarkdownImageOccurrence {
  alt: string
  end: number
  occurrenceIndex: number
  start: number
  markdown: string
  url: string
}

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

export function extractMarkdownImageOccurrences(
  text: string
): MarkdownImageOccurrence[] {
  return [...text.matchAll(markdownImagePattern)].map((match, occurrenceIndex) => {
    const start = match.index ?? 0

    return {
      alt: match[1] ?? ``,
      end: start + match[0].length,
      occurrenceIndex,
      start,
      markdown: match[0],
      url: match[2] ?? ``,
    }
  })
}

export function extractMarkdownImageUrls(text: string) {
  return extractMarkdownImageOccurrences(text).map((match) => match.url)
}

export function collectMarkdownImageUrls(text: string) {
  const seenUrls = new Set<string>()
  const orderedUrls: string[] = []

  for (const url of extractMarkdownImageUrls(text)) {
    if (seenUrls.has(url)) {
      continue
    }

    seenUrls.add(url)
    orderedUrls.push(url)
  }

  return orderedUrls
}

function updateMarkdownImages(
  text: string,
  transform: (match: MarkdownImageOccurrence) => string | undefined
) {
  let result = ``
  let lastIndex = 0

  for (const match of extractMarkdownImageOccurrences(text)) {
    result += text.slice(lastIndex, match.start)
    result += transform(match) ?? match.markdown
    lastIndex = match.end
  }

  result += text.slice(lastIndex)
  return result
}

export function removeMarkdownImagesByUrl(
  text: string,
  urls: Iterable<string>
) {
  const urlSet = new Set(urls)

  if (urlSet.size === 0) {
    return text
  }

  return updateMarkdownImages(text, (match) =>
    urlSet.has(match.url) ? `` : undefined
  )
}

export function removeMarkdownImageByOccurrence(
  text: string,
  occurrenceIndex: number
) {
  return updateMarkdownImages(text, (match) =>
    match.occurrenceIndex === occurrenceIndex ? `` : undefined
  )
}

export function replaceMarkdownImageUrls(
  text: string,
  replacements: Map<string, string> | Record<string, string>
) {
  const replacementMap =
    replacements instanceof Map
      ? replacements
      : new Map(Object.entries(replacements))

  if (replacementMap.size === 0) {
    return text
  }

  return updateMarkdownImages(text, (match) => {
    const nextUrl = replacementMap.get(match.url)

    if (!nextUrl) {
      return undefined
    }

    return match.markdown.replace(match.url, nextUrl)
  })
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
