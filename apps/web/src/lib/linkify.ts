/**
 * URL linkification for agent-feed narration text (EXP-430).
 *
 * Narration used to render as plain text, which made the claude sign-in URL
 * the emitter publishes during a remote `/login` uncopyable-by-tap. This is
 * a deliberately tiny tokenizer, not a markdown pass: split the text into
 * plain and `https?://` segments so the view can render anchors in place.
 * Hand-mirrored on iOS (`Linkify.swift`) and Android (`Linkify.kt`).
 */

export interface LinkSegment {
  text: string
  /** Present iff this segment renders as a link. */
  href?: string
}

const URL_PATTERN = /https?:\/\/[^\s]+/g

const count = (haystack: string, needle: string) =>
  haystack.split(needle).length - 1

/** Trailing punctuation is prose, not URL — `(see https://x.dev).` must not
 *  link the `).`. A BALANCED closing paren/bracket is kept, so
 *  `https://x.dev/a(b)` stays whole. */
function trimTrailingPunctuation(url: string): string {
  let out = url
  for (;;) {
    const last = out.at(-1)
    if (last === undefined) return out
    if (`.,;:!?`.includes(last)) {
      out = out.slice(0, -1)
    } else if (last === `)` && count(out, `(`) < count(out, `)`)) {
      out = out.slice(0, -1)
    } else if (last === `]` && count(out, `[`) < count(out, `]`)) {
      out = out.slice(0, -1)
    } else {
      return out
    }
  }
}

/** Split `text` into plain/link segments; concatenating the segments' `text`
 *  always reproduces the input byte-for-byte. */
export function linkSegments(text: string): LinkSegment[] {
  const segments: LinkSegment[] = []
  let cursor = 0
  for (const match of text.matchAll(URL_PATTERN)) {
    const url = trimTrailingPunctuation(match[0])
    if (url.length === 0) continue
    if (match.index > cursor) {
      segments.push({ text: text.slice(cursor, match.index) })
    }
    segments.push({ text: url, href: url })
    cursor = match.index + url.length
  }
  if (cursor < text.length || segments.length === 0) {
    segments.push({ text: text.slice(cursor) })
  }
  return segments
}
