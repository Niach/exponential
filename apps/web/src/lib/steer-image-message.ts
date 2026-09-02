// EXP-511: steer messages carry attached images as markdown embeds. The host
// device localizes each embed to a file path before the agent sees it, and the
// activity echo restores the embed, so this exact shape is load-bearing across
// web, iOS (SteerImageMessage.swift) and Android (SteerImageMessage.kt) — keep
// the three builders byte-identical.
export const MAX_STEER_IMAGES = 4

// EXP-698: a POSITIONAL reference to one of the message's images. The composer
// drops `[Image #k]` at the caret when the k-th image is attached, so the
// agent reads "crop [Image #2]" instead of guessing which embed a sentence
// means. The marker is plain text on the wire — the embeds below the text stay
// the only image payload — and the viewer renders it as a chip.
export const IMAGE_MARKER_PATTERN = /\[Image #(\d+)\]/g

export function imageMarker(index: number): string {
  return `[Image #${index}]`
}

// One embed line, exactly as `buildSteerImageMessage` writes it.
const EMBED_LINE = /^!\[image\]\(\/api\/attachments\/([^)\s]+)\)$/

export function buildSteerImageMessage(
  text: string,
  attachmentIds: string[]
): string {
  const trimmed = text.trim()
  if (attachmentIds.length === 0) return trimmed
  const embeds = attachmentIds
    .map((id) => `![image](/api/attachments/${id})`)
    .join(`\n`)
  if (!trimmed) return embeds
  return `${trimmed}\n\n${embeds}`
}

export interface ParsedSteerMessage {
  /** The message without its trailing embed block. */
  text: string
  /** Attachment ids, in embed order — image #1 is `attachmentIds[0]`. */
  attachmentIds: string[]
  /** The `[Image #N]` numbers the text carries, 1-based, in text order,
   *  deduped. A number with no matching embed is still reported — the viewer
   *  decides what to do with a dangling reference. */
  markers: number[]
}

/** The inverse of `buildSteerImageMessage`: splits a composed steer message
 *  back into its prose and its embeds, and reports the positional markers the
 *  prose carries. */
export function parseSteerMessage(message: string): ParsedSteerMessage {
  const lines = message.split(`\n`)
  let end = lines.length
  while (end > 0 && lines[end - 1].trim() === ``) end--
  const attachmentIds: string[] = []
  while (end > 0) {
    const match = EMBED_LINE.exec(lines[end - 1].trim())
    if (!match) break
    attachmentIds.unshift(match[1])
    end--
  }
  const text = lines.slice(0, end).join(`\n`).trimEnd()
  const markers: number[] = []
  for (const match of text.matchAll(IMAGE_MARKER_PATTERN)) {
    const index = Number(match[1])
    if (!markers.includes(index)) markers.push(index)
  }
  return { text, attachmentIds, markers }
}

/** Drops `[Image #index]` at `caret`, space-separated from whatever it lands
 *  against. Returns the new draft and the caret behind the insertion. */
export function insertImageMarker(
  text: string,
  caret: number,
  index: number
): { text: string; caret: number } {
  const at = Math.max(0, Math.min(caret, text.length))
  const before = text.slice(0, at)
  const after = text.slice(at)
  const marker = imageMarker(index)
  const lead = before.length > 0 && !/\s$/.test(before) ? ` ` : ``
  const trail = after.length > 0 && !/^\s/.test(after) ? ` ` : ``
  return {
    text: `${before}${lead}${marker}${trail}${after}`,
    caret: at + lead.length + marker.length + trail.length,
  }
}

/** Removing the `removedIndex`-th pending image renumbers the draft: its own
 *  markers go, and every higher one slides down one. Only a line that LOST a
 *  marker gets the gap it left tidied — untouched lines keep their spacing. */
export function renumberImageMarkers(
  text: string,
  removedIndex: number
): string {
  return text
    .split(`\n`)
    .map((line) => {
      let dropped = false
      const next = line.replace(IMAGE_MARKER_PATTERN, (match, raw: string) => {
        const index = Number(raw)
        if (index === removedIndex) {
          dropped = true
          return ``
        }
        return index > removedIndex ? imageMarker(index - 1) : match
      })
      if (!dropped) return next
      const tidied = next.replace(/[ \t]{2,}/g, ` `).replace(/[ \t]+$/, ``)
      return line.startsWith(imageMarker(removedIndex))
        ? tidied.replace(/^[ \t]+/, ``)
        : tidied
    })
    .join(`\n`)
}
