// #EXP-2204 (docs/ci-components-contract.md §4–§6): the framework-free
// painting helpers LogLines, JobGraph and the CodeBlock additions share on
// web (React and Solid call the SAME functions, so their DOM stays equal).
// The rules themselves (anchors, the log model, rows, search, reveal, blame,
// the graph layout) are the core's (src/ci-log.ts, src/job-graph.ts); this
// file only turns them into what a painter places: the search marks over
// the ansi tokens, the fixed-height window, the reveal offset, the compact
// test and the geometry of style.json.

import styleJson from "../../generated/style.runtime.json" with { type: "json" }
import { stripAnsi, virtualWindow, WINDOW_OVERSCAN, type LogMatch, type WindowRange } from "@exponential_app/eui"

/** style.json `logView` (px / lines). */
export const LOG_VIEW: Readonly<{ searchWidth: number; indent: number; maxLines: number }> = styleJson.logView
/** style.json `codeBlock` (px). */
export const CODE_BLOCK_GEOMETRY: Readonly<{ blameWidth: number }> = styleJson.codeBlock

/** Pixels from the bottom that still count as "at the bottom" (sub-pixel
 *  scroll positions, a zoomed page): Follow and the CodeBlock follow. */
export const CI_FOLLOW_SLACK = 4

/** The surface breakpoint is below `md` (JobGraph's list mode): no
 *  breakpoint (below the first) or one whose width is under `md`'s. */
export function ciCompact(breakpoint: string | null, breakpoints: Readonly<Record<string, number>> | undefined): boolean {
  if (!breakpoint) return true
  const md = breakpoints?.md
  const at = breakpoints?.[breakpoint]
  if (typeof md !== `number` || typeof at !== `number`) return false
  return at < md
}

export interface CiToken {
  kind: string
  text: string
}

/** One painted run of a log line: plain tokens, or tokens inside a search
 *  mark (`match`; `current` = the current match, the `selected` state).
 *  `index` = the match's index in the whole search (its part id). */
export type CiSegment = { mark: null; tokens: CiToken[] } | { mark: `match` | `current`; index: number; tokens: CiToken[] }

/** The tokens of one line split at its search marks (code-point offsets
 *  into the escape-free text, as `logSearch` reports them). `marks` are the
 *  line's matches in order, `first` = the search index of the first one,
 *  `current` = the current match's search index (-1 = none). */
export function ciSegments(tokens: readonly CiToken[], marks: readonly LogMatch[], first: number, current: number): CiSegment[] {
  if (!marks.length) return [{ mark: null, tokens: tokens.slice() }]
  const out: CiSegment[] = []
  let m = 0
  let at = 0
  let seg: CiSegment | null = null
  const push = (kind: string, text: string, mark: number) => {
    if (!text) return
    const want = mark < 0 ? null : first + mark
    const cur = seg && (seg.mark === null ? null : (seg as { index: number }).index)
    if (!seg || cur !== want) {
      seg = want === null ? { mark: null, tokens: [] } : { mark: want === current ? `current` : `match`, index: want, tokens: [] }
      out.push(seg)
    }
    const last = seg.tokens[seg.tokens.length - 1]
    if (last && last.kind === kind) last.text += text
    else seg.tokens.push({ kind, text })
  }
  for (const t of tokens) {
    const cps = Array.from(t.text)
    let i = 0
    while (i < cps.length) {
      while (m < marks.length && marks[m]!.end <= at) m++
      const mk = marks[m]
      let stop: number
      let inMark: number
      if (mk && at >= mk.start) {
        stop = Math.min(cps.length, i + (mk.end - at))
        inMark = m
      } else {
        stop = mk ? Math.min(cps.length, i + (mk.start - at)) : cps.length
        inMark = -1
      }
      push(t.kind, cps.slice(i, stop).join(``), inMark)
      at += stop - i
      i = stop
    }
  }
  return out
}

/** The widest content line in characters (escape-free, a tab = 2: the
 *  `tab-size` of the code), so every row is that wide and the body's
 *  horizontal scroll never changes with the window. */
export function ciColumns(texts: readonly string[]): number {
  let max = 0
  for (const t of texts) {
    let n = 0
    for (const c of stripAnsi(t)) n += c === `\t` ? 2 : 1
    if (n > max) max = n
  }
  return max
}

/** Row offsets for fixed extents (`offsets[count]` = the content height). */
export function ciOffsets(extents: readonly number[]): number[] {
  const out = new Array<number>(extents.length + 1)
  let at = 0
  for (let i = 0; i < extents.length; i++) {
    out[i] = at
    at += extents[i]!
  }
  out[extents.length] = at
  return out
}

/** The rows to build for a viewport (both in content coordinates): the
 *  core's `virtualWindow` (rows intersecting it ± `windowOverscan`). */
export function ciWindow(extents: readonly number[], offsets: readonly number[], scroll: number, viewport: number): WindowRange {
  return virtualWindow(extents, 0, Math.max(0, scroll), Math.max(1, viewport), WINDOW_OVERSCAN, offsets)
}

/** The scroll offset that shows a row starting at `top` (`size` tall) in
 *  the TOP THIRD of a `viewport` (a revealed selection; `always` = also
 *  when it is already in view: a selection, else only when it is not: a
 *  search step), clamped to the content. */
export function ciRevealOffset(top: number, size: number, scroll: number, viewport: number, total: number, always: boolean): number {
  if (!always && top >= scroll && top + size <= scroll + viewport) return scroll
  return Math.max(0, Math.min(Math.max(0, total - viewport), top - viewport / 3))
}

/** The body's bottom offset (Follow). */
export const ciBottom = (total: number, viewport: number): number => Math.max(0, total - viewport)

/** The node has a height of its own (a style height, max height or flex):
 *  its body fills it and scrolls (LogLines, a windowed CodeBlock). */
export function ciSized(style: unknown): boolean {
  if (!style || typeof style !== `object`) return false
  const s = style as Record<string, unknown>
  return s.height !== undefined || s.maxHeight !== undefined || s.flex !== undefined || s.flexGrow !== undefined
}
