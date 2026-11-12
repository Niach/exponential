// #FEED-224: THE column fit of a Markdown table (fixtures/table-fit.json),
// the TS reference every renderer mirrors (Rust `eui_core::table_fit`,
// Swift `MarkdownTableFit`, Kotlin `MarkdownTableFit`, the web's
// `web/table-fit.ts` measures the DOM and calls this).

import fixture from "../fixtures/table-fit.json" with { type: "json" }

/** The widest a column's min-content (its longest word or chip) counts. */
export const TABLE_WORD_CAP: number = fixture.wordCap

/** One column's measured content widths: `min` = its widest unbreakable
 *  piece (a word, a chip, inline code), `max` = its widest cell on one line,
 *  over the header and the body cells. */
export interface TableFitColumn {
  min: number
  max: number
}

/** The content width of each column `available` px of content wide (the
 *  box less the renderer's cell padding and borders): whole pixels; their
 *  sum is under `available` when every column fits on one line (the table
 *  hugs its content) and over it when the minimums do not fit (the table
 *  scrolls sideways). */
export function fitTableColumns(columns: readonly TableFitColumn[], available: number, cap: number = TABLE_WORD_CAP): number[] {
  const avail = Math.max(0, Math.floor(available))
  const lo = columns.map((c) => Math.min(Math.ceil(Math.max(0, c.min)), cap))
  const hi = columns.map((c, i) => Math.max(Math.ceil(Math.max(0, c.max)), lo[i]!))
  const sum = (xs: readonly number[]) => xs.reduce((s, x) => s + x, 0)
  if (sum(lo) >= avail) return lo
  if (sum(hi) <= avail) return hi
  // Water-fill: one level L with Σ clamp(L, lo, hi) = avail, so a column
  // that fits under it keeps its one line and the rest share what is left.
  const at = (level: number) => sum(lo.map((l, i) => Math.min(Math.max(level, l), hi[i]!)))
  const points = [...new Set([...lo, ...hi])].sort((a, b) => a - b)
  let k = 0
  while (at(points[k + 1]!) < avail) k++
  const a = points[k]!
  const growing = lo.flatMap((l, i) => (l <= a && hi[i]! > a ? [i] : []))
  const level = a + (avail - at(a)) / growing.length
  const out = lo.map((l, i) => Math.min(Math.max(Math.floor(level), l), hi[i]!))
  let left = avail - sum(out)
  for (const i of growing) {
    if (left <= 0) break
    if (out[i]! < hi[i]!) {
      out[i]!++
      left--
    }
  }
  return out
}
