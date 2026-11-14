// EXP-511: steer messages carry attached images as markdown embeds. The host
// device localizes each embed to a file path before the agent sees it, and the
// activity echo restores the embed, so this exact shape is load-bearing across
// web, iOS (SteerImageMessage.swift) and Android (SteerImageMessage.kt) — keep
// the three builders byte-identical. EXP-825: the same shape carries a START's
// free text (`steer.startSession` `prompt`), so the cap is the contract's.
import { MAX_START_PROMPT_IMAGES } from "@exp/db-schema/domain"

export const MAX_STEER_IMAGES: number = MAX_START_PROMPT_IMAGES

// Wave D: any file rides beside the images, up to four per message (50 MB
// each, the issue Files cap). Files carry NO positional marker.
export const MAX_STEER_FILES = 4

/** A non-image attachment on the wire: the server's sanitized filename is
 *  the link text. */
export interface SteerFileRef {
  id: string
  name: string
}

// EXP-698: a POSITIONAL reference to one of the message's images. The composer
// drops `[Image #k]` at the caret when the k-th image is attached, so the
// agent reads "crop [Image #2]" instead of guessing which embed a sentence
// means. The marker is plain text on the wire — the embeds below the text stay
// the only image payload — and the viewer renders it as a chip. #EXP-1841:
// the rules are composer-attachments.json's, ONE implementation ×4.
import {
  IMAGE_MARKER_PATTERN,
  imageMarker,
  insertImageMarker,
  renumberImageMarkers,
} from "@exp/domain-contract/composer-attachments"

export { IMAGE_MARKER_PATTERN, imageMarker, insertImageMarker, renumberImageMarkers }

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

// One file line, exactly as `buildSteerMessage` writes it: a plain link
// (never `![`), its label consuming backslash-escape pairs.
const FILE_LINE = /^\[((?:\\.|[^\\\]])*)\]\(\/api\/attachments\/([^)\s]+)\)$/

/** `]` and `\` in a filename are backslash-escaped in the link text. */
export function escapeSteerFileName(name: string): string {
  return name.replace(/[\\\]]/g, (char) => `\\${char}`)
}

function unescapeSteerFileName(label: string): string {
  // Any `\x` reads as `x`, as on desktop, iOS, Android and the host.
  return label.replace(/\\(.)/g, `$1`)
}

/** The wire message with files: prose, a blank line, the image embed block
 *  (unchanged from `buildSteerImageMessage`), then one
 *  `[<filename>](/api/attachments/<id>)` line per file. */
export function buildSteerMessage(
  text: string,
  imageIds: string[],
  files: SteerFileRef[]
): string {
  const head = buildSteerImageMessage(text, imageIds)
  if (files.length === 0) return head
  const links = files
    .map((file) => `[${escapeSteerFileName(file.name)}](/api/attachments/${file.id})`)
    .join(`\n`)
  if (!head) return links
  return imageIds.length > 0 ? `${head}\n${links}` : `${head}\n\n${links}`
}

export interface ParsedSteerMessage {
  /** The message without its trailing embed block. */
  text: string
  /** IMAGE attachment ids, in embed order — image #1 is `attachmentIds[0]`. */
  attachmentIds: string[]
  /** The trailing file links, in order (peeled before the image embeds). */
  files: SteerFileRef[]
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
  const files: SteerFileRef[] = []
  while (end > 0) {
    const match = FILE_LINE.exec(lines[end - 1].trim())
    if (!match) break
    files.unshift({ id: match[2], name: unescapeSteerFileName(match[1]) })
    end--
  }
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
  return { text, attachmentIds, files, markers }
}
