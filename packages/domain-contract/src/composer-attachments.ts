// #EXP-1841 (fixtures/composer-attachments.json): THE composer's attachment
// strip and its `[Image #N]` placeholders, the same in EVERY composer ×4 (a
// new chat's, a session's, an agent chat's, the Assistant's, an app editor's).
// Pure: the strip is a list of kinds; classifying a picked file (type, size)
// stays the platform's, so `add` arrives already classified.

export type ComposerAttachmentKind = `image` | `file`
export type ComposerPickKind = ComposerAttachmentKind | `rejected`

/** Images and files each cap at four per message. */
export const COMPOSER_MAX_IMAGES = 4
export const COMPOSER_MAX_FILES = 4

/** The rejection toasts ×4. */
export const ATTACHMENT_REJECTED_TOAST = `Images up to 10 MB and files up to 50 MB can be attached`
export const IMAGE_CAP_TOAST = `Up to ${COMPOSER_MAX_IMAGES} images per message`
export const FILE_CAP_TOAST = `Up to ${COMPOSER_MAX_FILES} files per message`

export const IMAGE_MARKER_PATTERN = /\[Image #(\d+)\]/g

export function imageMarker(index: number): string {
  return `[Image #${index}]`
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

/** Removing the `removedIndex`-th image renumbers the draft: its own markers
 *  go, and every higher one slides down one. Only a line that LOST a marker
 *  gets the gap it left tidied; untouched lines keep their spacing. */
export function renumberImageMarkers(text: string, removedIndex: number): string {
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
      return line.startsWith(imageMarker(removedIndex)) ? tidied.replace(/^[ \t]+/, ``) : tidied
    })
    .join(`\n`)
}

export interface StageInput {
  /** The strip before the add, in order. */
  strip: ComposerAttachmentKind[]
  /** The picked files, classified, in pick order. */
  add: ComposerPickKind[]
  text: string
  caret: number
}

export interface StageResult {
  /** The strip after the add. */
  strip: ComposerAttachmentKind[]
  /** For each `add` entry: true when it joined the strip. */
  taken: boolean[]
  /** The draft with one marker per TAKEN image at the caret. */
  text: string
  /** The caret behind the last inserted marker (unchanged without one). */
  caret: number
  /** Picks refused for size or type (`rejected`). */
  rejected: number
  /** Images over the image cap. */
  overflow: number
  /** Files over the file cap. */
  fileOverflow: number
}

export function imageCount(strip: readonly ComposerAttachmentKind[]): number {
  return strip.filter((kind) => kind === `image`).length
}

/** Stages a pick onto the strip. The image count BEFORE the add is the
 *  numbering base: image #1 is the strip's first IMAGE (files carry no
 *  marker and never count). */
export function stageAttachments(input: StageInput): StageResult {
  const base = imageCount(input.strip)
  let imageRoom = Math.max(0, COMPOSER_MAX_IMAGES - base)
  let fileRoom = Math.max(0, COMPOSER_MAX_FILES - (input.strip.length - base))
  let rejected = 0
  let overflow = 0
  let fileOverflow = 0
  const strip = [...input.strip]
  const taken: boolean[] = []
  let text = input.text
  let caret = Math.max(0, Math.min(input.caret, input.text.length))
  let number = base
  for (const kind of input.add) {
    if (kind === `rejected`) {
      rejected++
      taken.push(false)
    } else if (kind === `image`) {
      if (imageRoom === 0) {
        overflow++
        taken.push(false)
        continue
      }
      imageRoom--
      strip.push(kind)
      taken.push(true)
      number++
      const inserted = insertImageMarker(text, caret, number)
      text = inserted.text
      caret = inserted.caret
    } else if (fileRoom === 0) {
      fileOverflow++
      taken.push(false)
    } else {
      fileRoom--
      strip.push(kind)
      taken.push(true)
    }
  }
  return { strip, taken, text, caret, rejected, overflow, fileOverflow }
}

export interface DropInput {
  strip: ComposerAttachmentKind[]
  /** The position in the strip of the attachment the X removed. */
  at: number
  text: string
}

/** The strip's X. An image's markers leave the draft and every higher one
 *  slides down, so the numbers keep matching the strip; a file has none. */
export function dropAttachment(input: DropInput): { strip: ComposerAttachmentKind[]; text: string } {
  const kind = input.strip[input.at]
  if (kind === undefined) return { strip: input.strip, text: input.text }
  const strip = input.strip.filter((_, index) => index !== input.at)
  if (kind !== `image`) return { strip, text: input.text }
  const number = imageCount(input.strip.slice(0, input.at)) + 1
  return { strip, text: renumberImageMarkers(input.text, number) }
}

/** The toasts a stage owes, in order. */
export function stageToasts(result: Pick<StageResult, `rejected` | `overflow` | `fileOverflow`>): string[] {
  const toasts: string[] = []
  if (result.rejected > 0) toasts.push(ATTACHMENT_REJECTED_TOAST)
  if (result.overflow > 0) toasts.push(IMAGE_CAP_TOAST)
  if (result.fileOverflow > 0) toasts.push(FILE_CAP_TOAST)
  return toasts
}
