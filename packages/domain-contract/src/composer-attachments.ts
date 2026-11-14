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
  return renumberWithCaret(text, removedIndex, 0).text
}

/** Collapses each run of 2+ spaces/tabs to one space, mapping `at`: a caret
 *  inside a run (past its first character) lands behind the single space. */
function collapseRuns(line: string, at: number): { text: string; at: number } {
  let out = ``
  let mapped = at >= line.length ? -1 : at
  let i = 0
  while (i < line.length) {
    const ch = line[i]
    if (ch === ` ` || ch === `\t`) {
      let j = i
      while (j < line.length && (line[j] === ` ` || line[j] === `\t`)) j++
      if (j - i >= 2) {
        if (at === i) mapped = out.length
        else if (at > i && at <= j) mapped = out.length + 1
        out += ` `
        i = j
        continue
      }
    }
    if (at === i) mapped = out.length
    out += ch
    i++
  }
  return { text: out, at: mapped === -1 ? out.length : mapped }
}

/** [renumberImageMarkers] with the caret carried along (the X keeps it
 *  where it was): it moves left by what the removal took out before it, so
 *  a caret right behind the removed placeholder lands where it began. A
 *  caret INSIDE a marker counts as at that marker's start. */
export function renumberWithCaret(
  text: string,
  removedIndex: number,
  caret: number
): { text: string; caret: number } {
  let at = Math.max(0, Math.min(caret, text.length))
  for (const match of text.matchAll(IMAGE_MARKER_PATTERN)) {
    const start = match.index ?? 0
    if (at > start && at < start + match[0].length) at = start
  }
  const lines = text.split(`\n`)
  const out: string[] = []
  let lineStart = 0
  let mappedCaret = 0
  let outOffset = 0
  for (const line of lines) {
    const local = at >= lineStart && at <= lineStart + line.length ? at - lineStart : -1
    let dropped = false
    let next = ``
    let cursor = 0
    let mapped = -1
    for (const match of line.matchAll(IMAGE_MARKER_PATTERN)) {
      const start = match.index ?? 0
      if (local >= cursor && local <= start && mapped === -1) mapped = next.length + (local - cursor)
      next += line.slice(cursor, start)
      const index = Number(match[1])
      if (index === removedIndex) dropped = true
      else next += index > removedIndex ? imageMarker(index - 1) : match[0]
      cursor = start + match[0].length
      if (local === cursor && mapped === -1) mapped = next.length
    }
    if (local >= cursor && mapped === -1) mapped = next.length + (local - cursor)
    next += line.slice(cursor)
    if (dropped) {
      const collapsed = collapseRuns(next, mapped)
      next = collapsed.text
      mapped = collapsed.at
      const trimmedEnd = next.replace(/[ \t]+$/, ``)
      next = trimmedEnd
      mapped = Math.min(mapped, next.length)
      if (line.startsWith(imageMarker(removedIndex))) {
        const lead = next.length - next.replace(/^[ \t]+/, ``).length
        next = next.slice(lead)
        mapped = Math.max(0, mapped - lead)
      }
    }
    if (local !== -1 && mapped !== -1) mappedCaret = outOffset + mapped
    out.push(next)
    outOffset += next.length + 1
    lineStart += line.length + 1
  }
  return { text: out.join(`\n`), caret: mappedCaret }
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
  /** The field's caret when the X was pressed. */
  caret: number
}

export interface DropResult {
  strip: ComposerAttachmentKind[]
  text: string
  /** The caret after: where it was, moved left by what left before it. */
  caret: number
}

/** The strip's X. An image's markers leave the draft and every higher one
 *  slides down, so the numbers keep matching the strip; a file has none. The
 *  caret stays where it was (`renumberWithCaret`). */
export function dropAttachment(input: DropInput): DropResult {
  const caret = Math.max(0, Math.min(input.caret, input.text.length))
  const kind = input.strip[input.at]
  if (kind === undefined) return { strip: input.strip, text: input.text, caret }
  const strip = input.strip.filter((_, index) => index !== input.at)
  if (kind !== `image`) return { strip, text: input.text, caret }
  const number = imageCount(input.strip.slice(0, input.at)) + 1
  const renumbered = renumberWithCaret(input.text, number, caret)
  return { strip, text: renumbered.text, caret: renumbered.caret }
}

/** The toasts a stage owes, in order. */
export function stageToasts(result: Pick<StageResult, `rejected` | `overflow` | `fileOverflow`>): string[] {
  const toasts: string[] = []
  if (result.rejected > 0) toasts.push(ATTACHMENT_REJECTED_TOAST)
  if (result.overflow > 0) toasts.push(IMAGE_CAP_TOAST)
  if (result.fileOverflow > 0) toasts.push(FILE_CAP_TOAST)
  return toasts
}
