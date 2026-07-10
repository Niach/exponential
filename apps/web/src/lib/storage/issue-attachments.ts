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

function sanitizeAttachmentFilename(filename: string) {
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

/**
 * Write-path sanitizer for the stored `attachments.filename` display value.
 * Preserves Unicode (display names stay human-readable — header safety is the
 * read path's job via buildContentDispositionHeader) but strips C0/C1 control
 * characters and DEL, and clamps to 255 characters so any browser-supplied
 * name fits the varchar column.
 */
export function sanitizeUploadFilename(filename: string, fallback = `file`) {
  const sanitized = filename
    .replace(/[\x00-\x1F\x7F-\x9F]/g, ``)
    .trim()
    .slice(0, 255)
    .trim()

  return sanitized || fallback
}

function encodeRfc5987ValueChars(value: string) {
  // encodeURIComponent leaves `'()*` raw, but RFC 5987 attr-char excludes
  // them; `!-._~` are attr-chars and correctly stay raw.
  return encodeURIComponent(value).replace(
    /['()*]/g,
    (char) => `%${char.charCodeAt(0).toString(16).toUpperCase()}`
  )
}

/**
 * Builds an RFC 6266/5987 Content-Disposition value that is safe for ANY
 * stored filename (legacy unsanitized rows included): the output never
 * contains control characters or non-Latin-1 code points, so `new Headers()`
 * can never throw on it. Plain-ASCII names get the simple quoted form;
 * anything else gets a `?`-mangled ASCII fallback plus the UTF-8 `filename*`
 * form that modern clients prefer.
 */
export function buildContentDispositionHeader(
  disposition: `inline` | `attachment`,
  filename: string
) {
  const stripped = filename.replace(/[\x00-\x1F\x7F-\x9F]/g, ``).trim()
  const asciiFallback =
    stripped
      .replace(/[^\x20-\x7E]/g, `?`)
      .replace(/["\\]/g, `'`)
      .trim() || `file`

  if (!stripped || asciiFallback === stripped) {
    return `${disposition}; filename="${asciiFallback}"`
  }

  try {
    return `${disposition}; filename="${asciiFallback}"; filename*=UTF-8''${encodeRfc5987ValueChars(stripped)}`
  } catch {
    // Lone surrogates make encodeURIComponent throw a URIError — serve the
    // ASCII fallback rather than failing the whole response.
    return `${disposition}; filename="${asciiFallback}"`
  }
}

export function isAcceptedImageContentType(contentType: string) {
  return acceptedImageContentTypes.includes(
    contentType as (typeof acceptedImageContentTypes)[number]
  )
}

export function extractMarkdownImageOccurrences(
  text: string
): MarkdownImageOccurrence[] {
  return [...text.matchAll(markdownImagePattern)].map(
    (match, occurrenceIndex) => {
      const start = match.index ?? 0

      return {
        alt: match[1] ?? ``,
        end: start + match[0].length,
        occurrenceIndex,
        start,
        markdown: match[0],
        url: match[2] ?? ``,
      }
    }
  )
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

    if (
      /^[a-z][a-z0-9+.-]*:\/\//i.test(value) &&
      url.origin !== new URL(origin).origin
    ) {
      return null
    }

    return getAttachmentIdFromParsedUrl(url)
  } catch {
    return null
  }
}

export function extractAttachmentIdsFromDescription(
  text: string,
  origin: string
) {
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

/**
 * Unions the attachment ids referenced across multiple markdown texts (e.g.
 * all of an issue's comment bodies, optionally plus its description). URLs
 * that don't resolve to a same-origin attachment are ignored — this is a
 * liveness scan, not validation.
 */
export function collectReferencedAttachmentIds(
  texts: Iterable<string>,
  origin: string
): Set<string> {
  const attachmentIds = new Set<string>()

  for (const text of texts) {
    for (const attachmentId of extractAttachmentIdsFromDescription(text, origin)
      .attachmentIds) {
      attachmentIds.add(attachmentId)
    }
  }

  return attachmentIds
}

export function hasMarkdownImages(text: string) {
  return extractMarkdownImageUrls(text).length > 0
}

/**
 * Rewrites every image whose URL resolves to one of our attachments into the
 * canonical relative `/api/attachments/{id}` form. This makes stored markdown
 * client-agnostic (a client that submitted an absolute/proxied URL still ends
 * up relative) and removes the "resolved twice" class of bugs at the source.
 */
export function canonicalizeMarkdownImageUrls(text: string, origin: string) {
  return updateMarkdownImages(text, (match) => {
    const attachmentId = getAttachmentIdFromUrl(match.url, origin)
    if (!attachmentId) return undefined

    const canonical = buildAttachmentUrl(attachmentId)
    if (match.url === canonical) return undefined

    return match.markdown.replace(match.url, canonical)
  })
}

export function stripMarkdownImages(text: string) {
  return updateMarkdownImages(text, () => ``)
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
