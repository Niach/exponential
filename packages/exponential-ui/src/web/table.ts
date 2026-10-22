// Round 1 (FEED-128: hoisted from the React renderer's natives/table.tsx):
// a Table's column shape and the renderer's LOCAL row order for an unbound
// sort (`interactions.json` sort cases): missing values last, numbers and
// dates by value, booleans false-first, text by a numeric, base-sensitivity
// collator in the surface locale, ties in source order.

import { num } from "./coerce"

export interface TableColumn {
  key: string
  /** The header text (a slot or checkbox column may have none). */
  label?: string
  type?: `text` | `number` | `date` | `boolean` | `badge` | `slot` | `currency` | `percent` | `relativeTime`
  slot?: string
  /** ISO 4217 code of a `currency` column. */
  currency?: string
  /** Fixed fraction digits of a number / currency / percent column. */
  decimals?: number
  width?: number
  align?: `start` | `center` | `end`
  sortable?: boolean
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
    else if (column.type === `date`) c = new Date(String(va)).getTime() - new Date(String(vb)).getTime()
    else if (typeof va === `boolean` || typeof vb === `boolean`) c = Number(Boolean(va)) - Number(Boolean(vb))
    else c = collator.compare(String(va), String(vb))
    return c === 0 ? a - b : c * dir
  })
}
