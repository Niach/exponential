// #EDE-95 (fixtures/text-highlight.json): search-match marks on genUI text
// (Text, Markdown, MessageBubble, Row `highlight`), the rule every renderer
// and its count twin replays. A match is the query found LITERALLY in the
// text after folding both per Unicode scalar: canonical decomposition (NFD),
// lower case, combining marks dropped (U+0300–036F, 1AB0–1AFF, 1DC0–1DFF,
// 20D0–20FF, FE20–FE2F), so case and diacritics never matter (`creme` finds
// `Crème`; `ß` stays `ß`). Matches run left to right and never overlap; a
// match ends after the scalars its last folded character came from plus the
// combining marks right after them (a decomposed `é` marks whole). A query
// that folds to nothing matches nothing. `current` = the 0-based index among
// the node's matches painted as the current one; `before` = text whose
// matches number BEFORE this node's (Row's subtitle gets the title), so this
// node's current match is `current - count(before)`.

/** #EDE-95: the `highlight` prop (catalog defs `highlight`). */
export interface HighlightSpec {
  query: string
  current?: number
  before?: string
}

/** A match: UTF-16 offsets into the original text, `end` exclusive. */
export interface HighlightRange {
  start: number
  end: number
}

/** A chip (or any piece that never matches) inside a searched text: no
 *  query contains it, so no match crosses it. */
export const HIGHLIGHT_BARRIER = `￼`

function isMark(cp: number): boolean {
  return (cp >= 0x300 && cp <= 0x36f) || (cp >= 0x1ab0 && cp <= 0x1aff) || (cp >= 0x1dc0 && cp <= 0x1dff) || (cp >= 0x20d0 && cp <= 0x20ff) || (cp >= 0xfe20 && cp <= 0xfe2f)
}

/** One scalar folded: NFD, lower case, marks dropped. */
function foldScalar(ch: string): string {
  let out = ``
  for (const c of ch.normalize(`NFD`).toLowerCase()) if (!isMark(c.codePointAt(0)!)) out += c
  return out
}

/** The folded text (an array of folded characters) with, per folded
 *  character, the UTF-16 span of the scalar it came from. */
function fold(text: string): { chars: string[]; from: number[]; to: number[] } {
  const chars: string[] = []
  const from: number[] = []
  const to: number[] = []
  let at = 0
  for (const ch of text) {
    const end = at + ch.length
    for (const c of foldScalar(ch)) {
      chars.push(c)
      from.push(at)
      to.push(end)
    }
    at = end
  }
  return { chars, from, to }
}

/** The query as folded characters (a barrier never matches). */
function foldQuery(query: string): string[] {
  return fold(query).chars.filter((c) => c !== HIGHLIGHT_BARRIER)
}

/** Every match of `query` in `text`, left to right, never overlapping. */
export function highlightRanges(text: string, query: string): HighlightRange[] {
  const q = foldQuery(query)
  if (!q.length || !text) return []
  const { chars, from, to } = fold(text)
  const out: HighlightRange[] = []
  let i = 0
  while (i + q.length <= chars.length) {
    let hit = true
    for (let k = 0; k < q.length; k++) {
      if (chars[i + k] !== q[k]) {
        hit = false
        break
      }
    }
    if (!hit) {
      i++
      continue
    }
    let end = to[i + q.length - 1]!
    // The combining marks right after the match (folded to nothing) join it.
    while (end < text.length) {
      const cp = text.codePointAt(end)!
      if (!isMark(cp)) break
      end += cp > 0xffff ? 2 : 1
    }
    out.push({ start: from[i]!, end })
    i += q.length
  }
  return out
}

/** How many matches `query` has in `text`. */
export function highlightCount(text: string, query: string): number {
  return highlightRanges(text, query).length
}

/** The prop as a spec: an object with a string `query` (else null). */
export function highlightSpec(value: unknown): HighlightSpec | null {
  if (!value || typeof value !== `object` || Array.isArray(value)) return null
  const v = value as Record<string, unknown>
  if (typeof v.query !== `string` || !foldQuery(v.query).length) return null
  const current = typeof v.current === `number` && Number.isFinite(v.current) ? Math.floor(v.current) : undefined
  return { query: v.query, ...(current !== undefined ? { current } : {}), ...(typeof v.before === `string` && v.before ? { before: v.before } : {}) }
}

/** The index among THIS node's matches painted current (none = -1):
 *  `current` minus the matches of `before`. */
export function highlightLocalCurrent(spec: HighlightSpec): number {
  if (spec.current === undefined || spec.current < 0) return -1
  const local = spec.current - (spec.before ? highlightCount(spec.before, spec.query) : 0)
  return local >= 0 ? local : -1
}
