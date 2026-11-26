// Round 4 group B (fixtures/text-runs.json): a Text's `runs` (inline spans)
// and its `dropCap`, the rule every renderer and the Rust core
// (eui-core `text_runs`, Swift `TextRuns`, Kotlin `TextRuns`) replays.
// `runs` set (an array) REPLACES `text`: one paragraph of spans that wrap
// together. A run is a string (= `{text}`) or `{text, weight?, tone?, mark?,
// link?}`; its `text` is a string, number or boolean (a number or boolean
// shows as its display string); an unknown `weight` / `tone` / `mark` or a
// link that is not a non-empty string is dropped; an empty run drops;
// adjacent runs with the same attributes merge. `dropCap` = the lines the
// first letter spans: a number rounds and clamps to 2..4 (below 2 = none),
// `true` = 3; never with `lines`, `truncate` or `shimmer`.

import { displayString } from "./format"

export const RUN_WEIGHTS = [`regular`, `medium`, `semibold`, `bold`] as const
export const RUN_MARKS = [`highlight`, `code`, `strike`, `underline`] as const
export const RUN_TONES = [`neutral`, `primary`, `success`, `warning`, `danger`, `info`] as const

export type RunWeight = (typeof RUN_WEIGHTS)[number]
export type RunMark = (typeof RUN_MARKS)[number]

/** One normalised span (only the attributes it sets). */
export interface TextRun {
  text: string
  weight?: RunWeight
  tone?: string
  mark?: RunMark
  link?: string
}

/** The drop cap's fixed numbers (fixtures/text-runs.json `dropCap`). */
export const DROP_CAP = { min: 2, max: 4, lines: 3, ratio: 1.375, gap: `$spacing.xs` } as const

function runText(v: unknown): string | null {
  if (typeof v === `string`) return v
  if (typeof v === `number` || typeof v === `boolean`) return displayString(v)
  return null
}

function same(a: TextRun, b: TextRun): boolean {
  return a.weight === b.weight && a.tone === b.tone && a.mark === b.mark && a.link === b.link
}

/** `runs` normalised (null = not an array: the Text shows `text`). */
export function textRuns(value: unknown): TextRun[] | null {
  if (!Array.isArray(value)) return null
  const out: TextRun[] = []
  for (const entry of value) {
    let run: TextRun | null = null
    if (typeof entry === `string`) run = { text: entry }
    else if (entry && typeof entry === `object` && !Array.isArray(entry)) {
      const e = entry as Record<string, unknown>
      const text = runText(e.text)
      if (text !== null) {
        run = { text }
        if (typeof e.weight === `string` && (RUN_WEIGHTS as readonly string[]).includes(e.weight)) run.weight = e.weight as RunWeight
        if (typeof e.tone === `string` && (RUN_TONES as readonly string[]).includes(e.tone)) run.tone = e.tone
        if (typeof e.mark === `string` && (RUN_MARKS as readonly string[]).includes(e.mark)) run.mark = e.mark as RunMark
        if (typeof e.link === `string` && e.link.length) run.link = e.link
      }
    }
    if (!run || !run.text) continue
    const last = out[out.length - 1]
    if (last && same(last, run)) last.text += run.text
    else out.push(run)
  }
  return out
}

/** The runs' text joined: what measures, searches and is announced. */
export function runsText(runs: readonly TextRun[]): string {
  return runs.map((r) => r.text).join(``)
}

/** `dropCap` as lines (0 = none) for a Text's props. */
export function dropCapLines(props: Record<string, unknown>): number {
  if (props.shimmer === true || props.truncate === `end` || props.truncate === `middle`) return 0
  if (typeof props.lines === `number` && props.lines > 0) return 0
  const v = props.dropCap
  if (v === true) return DROP_CAP.lines
  if (typeof v !== `number` || !Number.isFinite(v)) return 0
  const n = Math.round(v)
  return n < DROP_CAP.min ? 0 : Math.min(n, DROP_CAP.max)
}

function isJoiner(cp: number): boolean {
  return (cp >= 0x300 && cp <= 0x36f) || (cp >= 0x1ab0 && cp <= 0x1aff) || (cp >= 0x1dc0 && cp <= 0x1dff) || (cp >= 0x20d0 && cp <= 0x20ff) || (cp >= 0xfe20 && cp <= 0xfe2f) || cp === 0xfe0e || cp === 0xfe0f
}

const LEADING = new Set([` `, `\t`, `\n`, `\r`])

/** The drop cap off the runs: leading ASCII spaces, tabs and line breaks
 *  drop; the cap = the first scalar plus the combining marks and variation
 *  selectors right after it, in the weight and tone of the run holding it
 *  (no mark, no link); `rest` = the runs after it. Null = no letter. */
export function dropCapSplit(runs: readonly TextRun[]): { cap: TextRun; rest: TextRun[] } | null {
  const rest = runs.map((r) => ({ ...r }))
  while (rest.length) {
    const r = rest[0]!
    let i = 0
    while (i < r.text.length && LEADING.has(r.text[i]!)) i++
    r.text = r.text.slice(i)
    if (r.text) break
    rest.shift()
  }
  const first = rest[0]
  if (!first) return null
  const chars = Array.from(first.text)
  let n = 1
  while (n < chars.length && isJoiner(chars[n]!.codePointAt(0)!)) n++
  const capText = chars.slice(0, n).join(``)
  const cap: TextRun = { text: capText, ...(first.weight ? { weight: first.weight } : {}), ...(first.tone ? { tone: first.tone } : {}) }
  first.text = first.text.slice(capText.length)
  if (!first.text) rest.shift()
  return { cap, rest }
}

/** The cap's font size: `fontSize + (lines - 1) × lineHeight × 1.375`,
 *  rounded to a whole px (its baseline then sits on line `lines`'s). */
export function dropCapSize(fontSize: number, lineHeight: number, lines: number): number {
  return Math.round(fontSize + (lines - 1) * lineHeight * DROP_CAP.ratio)
}

/** A Text's spans: its normalised `runs`, else `text` as one run (null =
 *  nothing to show), and the drop cap lines. */
export function textSpans(props: Record<string, unknown>): { runs: TextRun[]; rich: boolean; dropCap: number } {
  const runs = textRuns(props.runs)
  const text = runText(props.text) ?? ``
  return { runs: runs ?? (text ? [{ text }] : []), rich: runs !== null, dropCap: dropCapLines(props) }
}
