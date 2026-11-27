// #EXP-2204 (docs/ci-components-contract.md §1-§3, §6): the pure rules of
// LogView/LogLines and the CodeBlock anchors + blame, the same in the Rust
// core (`eui-core` `ci_log.rs`), Swift (`CiLog.swift`) and Kotlin
// (`CiLog.kt`), all replaying fixtures/ci-log.json. NO import, so the web
// renderers take it without the expression evaluator (src/tree-key.ts's
// rule); the constants below are gated equal to catalog/style.json by
// src/ci-log.test.ts.

// ---------------------------------------------------------------------------
// §1 The status table
// ---------------------------------------------------------------------------

/** The `ciStatus` enum, in catalogue order. */
export const CI_STATUSES = [`passed`, `failed`, `running`, `queued`, `cancelled`, `skipped`] as const
export type CiStatus = (typeof CI_STATUSES)[number]

/** One row of THE status table: the glyph and spin (style.json `ciStatus`),
 *  the mark colour and the text colour (the theme's `when: {status}`
 *  recipes, neutral values here), the a11y string id. */
export interface CiStatusEntry {
  status: CiStatus
  icon: string
  spin: boolean
  color: string
  text: string
  label: string
}

/** style.json `ciStatus.spinMs`: one turn of the running glyph. */
export const CI_STATUS_SPIN_MS = 1000

export const CI_STATUS_TABLE: Readonly<Record<CiStatus, CiStatusEntry>> = {
  passed: { status: `passed`, icon: `ci-passed`, spin: false, color: `$color.success`, text: `$color.foreground`, label: `statusPassed` },
  failed: { status: `failed`, icon: `ci-failed`, spin: false, color: `$color.destructive`, text: `$color.foreground`, label: `statusFailed` },
  running: { status: `running`, icon: `ci-running`, spin: true, color: `$color.info`, text: `$color.foreground`, label: `statusRunning` },
  queued: { status: `queued`, icon: `ci-queued`, spin: false, color: `$color.mutedForeground`, text: `$color.foreground`, label: `statusQueued` },
  cancelled: { status: `cancelled`, icon: `ci-skipped`, spin: false, color: `$color.mutedForeground`, text: `$color.mutedForeground`, label: `statusCancelled` },
  skipped: { status: `skipped`, icon: `ci-skipped`, spin: false, color: `$color.mutedForeground`, text: `$color.mutedForeground`, label: `statusSkipped` },
}

/** A status value → a `ciStatus`: unknown or missing = `queued`. */
export function ciStatusOf(value: unknown): CiStatus {
  return typeof value === `string` && (CI_STATUSES as readonly string[]).includes(value) ? (value as CiStatus) : `queued`
}

/** THE table row of a status (unknown or missing = `queued`). */
export function ciStatusEntry(value: unknown): CiStatusEntry {
  return CI_STATUS_TABLE[ciStatusOf(value)]
}

// ---------------------------------------------------------------------------
// §2 Line anchors
// ---------------------------------------------------------------------------

export interface LineRange {
  start: number
  end: number
}

/** The longest line number an anchor spells (9 digits): longer = no anchor. */
const ANCHOR_MAX_DIGITS = 9

function anchorNumber(text: string, from: number): { value: number; next: number } | null {
  let i = from
  while (i < text.length && text.charCodeAt(i) >= 0x30 && text.charCodeAt(i) <= 0x39) i++
  if (i === from || i - from > ANCHOR_MAX_DIGITS) return null
  const value = Number(text.slice(from, i))
  return value >= 1 ? { value, next: i } : null
}

/** `L12` / `L6-L9` (an optional leading `#`) → the normalised range; else null. */
export function parseLineAnchor(text: unknown): LineRange | null {
  if (typeof text !== `string`) return null
  let i = text.startsWith(`#`) ? 1 : 0
  if (text[i] !== `L`) return null
  const a = anchorNumber(text, i + 1)
  if (!a) return null
  i = a.next
  if (i === text.length) return { start: a.value, end: a.value }
  if (text[i] !== `-` || text[i + 1] !== `L`) return null
  const b = anchorNumber(text, i + 2)
  if (!b || b.next !== text.length) return null
  return { start: Math.min(a.value, b.value), end: Math.max(a.value, b.value) }
}

/** `L<start>` when equal (or `end` omitted), else `L<min>-L<max>`. */
export function formatLineAnchor(start: number, end: number = start): string {
  const lo = Math.min(start, end)
  const hi = Math.max(start, end)
  return lo === hi ? `L${lo}` : `L${lo}-L${hi}`
}

/** A press on line `line`: no shift or no valid current anchor → `L<line>`;
 *  shift → the range from the current START to `line`, normalised. */
export function extendLineAnchor(current: unknown, line: number, shift: boolean): string {
  const range = shift ? parseLineAnchor(current) : null
  return range ? formatLineAnchor(range.start, line) : formatLineAnchor(line)
}

// ---------------------------------------------------------------------------
// §3 The log model
// ---------------------------------------------------------------------------

/** One content line (marker lines get none). */
export interface LogLine {
  /** 1-based content line number. */
  n: number
  /** The open section's index, -1 outside every section. */
  section: number
  /** `HH:MM:SS` of a leading timestamp, as written; null without one. */
  time: string | null
  /** The codeToken kind a `##[error]`-style prefix gives the whole line, else null. */
  kind: `error` | `warning` | `comment` | null
  /** The line without the timestamp and the prefix, ANSI escapes kept. */
  text: string
}

export interface LogSection {
  index: number
  title: string
  /** First content line number (`end + 1` when empty). */
  start: number
  /** Last content line number (`start - 1` when empty). */
  end: number
  count: number
  status: CiStatus
  duration: string | null
}

export interface LogModel {
  lines: LogLine[]
  sections: LogSection[]
}

/** Per-section metadata a LogView's `sections` prop carries. */
export interface LogSectionMeta {
  status?: unknown
  duration?: unknown
}

const isCsiParam = (c: number) => c >= 0x30 && c <= 0x3f
const isCsiIntermediate = (c: number) => c >= 0x20 && c <= 0x2f
const isCsiFinal = (c: number) => c >= 0x40 && c <= 0x7e

/** The end index of a COMPLETE CSI escape starting at `i` (ESC `[` params
 *  intermediates final), else -1: src/code.ts ansiRuns' rule. */
function csiEnd(text: string, i: number): number {
  if (text.charCodeAt(i) !== 0x1b || text[i + 1] !== `[`) return -1
  let j = i + 2
  while (j < text.length && isCsiParam(text.charCodeAt(j))) j++
  while (j < text.length && isCsiIntermediate(text.charCodeAt(j))) j++
  return j < text.length && isCsiFinal(text.charCodeAt(j)) ? j + 1 : -1
}

/** The text with every complete ANSI CSI escape removed (an incomplete one stays). */
export function stripAnsi(text: string): string {
  let out = ``
  let i = 0
  while (i < text.length) {
    const end = csiEnd(text, i)
    if (end >= 0) {
      i = end
      continue
    }
    out += text[i]
    i++
  }
  return out
}

/** `text` with its first `count` escape-free UTF-16 units removed, every
 *  escape kept where it was. */
function cutVisible(text: string, count: number): string {
  let out = ``
  let removed = 0
  let i = 0
  while (i < text.length) {
    const end = csiEnd(text, i)
    if (end >= 0) {
      out += text.slice(i, end)
      i = end
      continue
    }
    if (removed < count) removed++
    else out += text[i]
    i++
  }
  return out
}

const TIMESTAMP = /^\d{4}-\d{2}-\d{2}T(\d{2}:\d{2}:\d{2})(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2}) /
const PREFIXES: [string, LogLine[`kind`]][] = [
  [`##[error]`, `error`],
  [`##[warning]`, `warning`],
  [`##[notice]`, `comment`],
  [`##[debug]`, `comment`],
]

/** ASCII space and tab trimmed off both ends. */
function trimSpaces(text: string): string {
  let a = 0
  let b = text.length
  while (a < b && (text[a] === ` ` || text[a] === `\t`)) a++
  while (b > a && (text[b - 1] === ` ` || text[b - 1] === `\t`)) b--
  return text.slice(a, b)
}

/** Raw log lines (+ per-section `{status, duration}`) → numbered content
 *  lines and their `##[group]` sections (contract §3). */
export function logModel(lines: readonly unknown[], sectionsMeta?: readonly unknown[] | null): LogModel {
  const out: LogLine[] = []
  const sections: LogSection[] = []
  let open = -1
  let closedLast = true
  const meta = Array.isArray(sectionsMeta) ? sectionsMeta : []
  for (const raw of Array.isArray(lines) ? lines : []) {
    let text = typeof raw === `string` ? raw : ``
    const stamp = TIMESTAMP.exec(text)
    const time = stamp ? stamp[1]! : null
    if (stamp) text = text.slice(stamp[0].length)
    const plain = stripAnsi(text)
    if (plain.startsWith(`##[group]`)) {
      if (open >= 0) sections[open]!.end = out.length
      open = sections.length
      closedLast = false
      sections.push({ index: open, title: trimSpaces(plain.slice(`##[group]`.length)), start: out.length + 1, end: out.length, count: 0, status: `passed`, duration: null })
      continue
    }
    if (plain.startsWith(`##[endgroup]`)) {
      if (open >= 0) {
        sections[open]!.end = out.length
        open = -1
        closedLast = true
      }
      continue
    }
    let kind: LogLine[`kind`] = null
    for (const [prefix, k] of PREFIXES) {
      if (plain.startsWith(prefix)) {
        kind = k
        text = cutVisible(text, prefix.length)
        break
      }
    }
    out.push({ n: out.length + 1, section: open, time, kind, text })
  }
  if (open >= 0) sections[open]!.end = out.length
  for (const s of sections) {
    s.count = s.end - s.start + 1
    const m = meta[s.index]
    const given = m && typeof m === `object` ? (m as LogSectionMeta) : null
    if (given && given.status !== undefined && given.status !== null) s.status = ciStatusOf(given.status)
    else s.status = !closedLast && s.index === sections.length - 1 ? `running` : `passed`
    s.duration = given && typeof given.duration === `string` ? given.duration : null
  }
  return { lines: out, sections }
}

/** The sections that open on their own: `failed` or `running`. */
export function logOpenDefault(model: LogModel): number[] {
  return model.sections.filter((s) => s.status === `failed` || s.status === `running`).map((s) => s.index)
}

export type LogRow = { kind: `line`; n: number } | { kind: `section`; section: number }

/** The flat rows a LogLines body windows: lines outside sections, each
 *  section's head row, then its lines when its index is in `open`. */
export function logRows(model: LogModel, open: readonly unknown[]): LogRow[] {
  const isOpen = new Set(Array.isArray(open) ? open : [])
  const rows: LogRow[] = []
  let line = 0
  const flushTo = (n: number) => {
    while (line < model.lines.length && model.lines[line]!.n < n) {
      rows.push({ kind: `line`, n: model.lines[line]!.n })
      line++
    }
  }
  for (const s of model.sections) {
    flushTo(s.start)
    rows.push({ kind: `section`, section: s.index })
    const shown = isOpen.has(s.index)
    while (line < model.lines.length && model.lines[line]!.n <= s.end) {
      if (shown) rows.push({ kind: `line`, n: model.lines[line]!.n })
      line++
    }
  }
  flushTo(Number.POSITIVE_INFINITY)
  return rows
}

export interface LogMatch {
  line: number
  /** Code-point offsets into the line's escape-free text, end exclusive. */
  start: number
  end: number
}

const foldAscii = (cp: number) => (cp >= 0x41 && cp <= 0x5a ? cp + 0x20 : cp)
const codePoints = (text: string) => Array.from(text, (c) => foldAscii(c.codePointAt(0)!))

/** Every non-overlapping match of `query` (ASCII case-insensitive, code-point
 *  offsets) in every content line's escape-free text, folded sections too. */
export function logSearch(model: LogModel, query: unknown): LogMatch[] {
  if (typeof query !== `string` || query === ``) return []
  const q = codePoints(query)
  const out: LogMatch[] = []
  for (const line of model.lines) {
    const t = codePoints(stripAnsi(line.text))
    let i = 0
    while (i + q.length <= t.length) {
      let k = 0
      while (k < q.length && t[i + k] === q[k]) k++
      if (k === q.length) {
        out.push({ line: line.n, start: i, end: i + q.length })
        i += q.length
      } else i++
    }
  }
  return out
}

/** `open` plus the section holding `line` (sorted, unique, integers only). */
export function logReveal(model: LogModel, open: readonly unknown[], line: number): number[] {
  const set = new Set<number>()
  for (const v of Array.isArray(open) ? open : []) if (typeof v === `number` && Number.isInteger(v)) set.add(v)
  const at = model.lines[line - 1]
  if (at && at.n === line && at.section >= 0) set.add(at.section)
  return [...set].sort((a, b) => a - b)
}

// ---------------------------------------------------------------------------
// §6 CodeBlock blame
// ---------------------------------------------------------------------------

export interface BlameLine {
  /** Index into the `blame` array, -1 before the first entry. */
  block: number
  /** The block's first line (shows author + age). */
  first: boolean
  /** A first line of every block but the first: the hairline top border. */
  border: boolean
}

/** Per line (1…lineCount) its blame block: entries sorted by `start`
 *  (stable), each running to the next entry's start − 1 (the last to the
 *  end); an entry whose `start` is no integer ≥ 1 is ignored. */
export function blameBlocks(blame: readonly unknown[], lineCount: number): BlameLine[] {
  const entries: { index: number; start: number }[] = []
  ;(Array.isArray(blame) ? blame : []).forEach((e, index) => {
    const start = e && typeof e === `object` ? (e as { start?: unknown }).start : undefined
    if (typeof start === `number` && Number.isInteger(start) && start >= 1) entries.push({ index, start })
  })
  entries.sort((a, b) => a.start - b.start)
  const out: BlameLine[] = []
  let at = -1
  let firstShown = -1
  for (let n = 1; n <= lineCount; n++) {
    let first = false
    while (at + 1 < entries.length && entries[at + 1]!.start <= n) {
      at++
      first = true
    }
    if (first && firstShown < 0) firstShown = n
    out.push({ block: at >= 0 ? entries[at]!.index : -1, first, border: first && n !== firstShown })
  }
  return out
}
