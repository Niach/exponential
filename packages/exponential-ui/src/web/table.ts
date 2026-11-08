// Round 1 (FEED-128: hoisted from the React renderer's natives/table.tsx):
// a Table's column shape and the renderer's LOCAL row order for an unbound
// sort (`interactions.json` sort cases): missing values last, numbers and
// dates by value, booleans false-first, text by a numeric, base-sensitivity
// collator in the surface locale, ties in source order.

import { TABLE_COLUMN_MIN } from "../layout"
import { num } from "./coerce"

export interface TableColumn {
  key: string
  /** The header text (a slot or checkbox column may have none). */
  label?: string
  type?: `text` | `number` | `date` | `boolean` | `badge` | `slot` | `currency` | `percent` | `relativeTime` | `dateTime`
  slot?: string
  /** ISO 4217 code of a `currency` column. */
  currency?: string
  /** Fixed fraction digits of a number / currency / percent column. */
  decimals?: number
  width?: number
  align?: `start` | `center` | `end`
  sortable?: boolean
  /** EXP-1548: one line with an ellipsis; cells wrap otherwise. */
  truncate?: boolean
  /** FEED-202: a `date` / `dateTime` column's TR35 pattern (`dd.MM.yy`). */
  format?: string
}

/** The checkbox column of a `multiple` table: as wide as the checkbox. */
const CHECK_COLUMN = `var(--xui-control-checkbox, 16px)`

/** Every row's grid columns (round 2 §7): a fixed `width` keeps its px, the
 *  rest share the width, EXP-1548: never under `tableColumnMin`. */
export function tableTemplate(columns: readonly TableColumn[], multiple: boolean): string {
  return [multiple ? CHECK_COLUMN : null, ...columns.map((c) => (c.width ? `${c.width}px` : `minmax(${TABLE_COLUMN_MIN}px, 1fr)`))].filter(Boolean).join(` `)
}

/** EXP-1548: the narrowest the columns fit in: header and body are at least
 *  this wide, so a narrower table scrolls (its root is the scroller). */
export function tableMinWidth(columns: readonly TableColumn[], multiple: boolean): string {
  const px = columns.reduce((sum, c) => sum + (c.width ? num(c.width) : TABLE_COLUMN_MIN), 0)
  return multiple ? `calc(${px}px + ${CHECK_COLUMN})` : `${px}px`
}

export interface TableSort {
  key: string
  direction: `asc` | `desc`
}

/** The renderer's local order of `rows` for a sort (unbound sort). */
export function sortRows(rows: readonly Record<string, unknown>[], sort: TableSort | null, column: TableColumn | undefined, locale: string): number[] {
  const order = rows.map((_, i) => i)
  if (!sort || !column) return order
  const collator = new Intl.Collator(locale, { numeric: true, sensitivity: `base` })
  const dir = sort.direction === `desc` ? -1 : 1
  const missing = (v: unknown) => v === undefined || v === null || v === ``
  return order.sort((a, b) => {
    const va = rows[a]?.[sort.key]
    const vb = rows[b]?.[sort.key]
    if (missing(va) && missing(vb)) return a - b
    if (missing(va)) return 1
    if (missing(vb)) return -1
    let c: number
    if (typeof va === `number` && typeof vb === `number`) c = va - vb
    else if (column.type === `number`) c = num(va) - num(vb)
    else if (column.type === `date` || column.type === `dateTime`) c = new Date(String(va)).getTime() - new Date(String(vb)).getTime()
    else if (typeof va === `boolean` || typeof vb === `boolean`) c = Number(Boolean(va)) - Number(Boolean(vb))
    else c = collator.compare(String(va), String(vb))
    return c === 0 ? a - b : c * dir
  })
}

/** FEED-202: the `totals` row, one entry per column: a `number` or
 *  `currency` column's sum over `rows` (each value as `num`: a numeric
 *  string counts, anything else is 0), else null. The renderer shows
 *  `$string.total` in the first column when it sums nothing. */
export function tableTotals(rows: readonly Record<string, unknown>[], columns: readonly TableColumn[]): (number | null)[] {
  return columns.map((c) => (c.type === `number` || c.type === `currency` ? rows.reduce((sum, r) => sum + num(r?.[c.key]), 0) : null))
}
