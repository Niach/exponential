// #FEED-224 (fixtures/table-fit.json): a Markdown table's columns on the
// web, shared by the React and Solid renderers. The browser's auto table
// layout at 100% width shared the width by max-content and, under the
// Markdown root's `overflow-wrap: anywhere`, let any column shrink to one
// letter per line beside an unbreakable chip. Here every cell's text box
// (`.xui-Markdown-cellText`) is measured at max- and min-content, the shared
// `fitTableColumns` decides, and each column's text boxes get that width:
// the table hugs its content and its scroller scrolls sideways when the
// minimums do not fit.

import { fitTableColumns, type TableFitColumn } from "../table-fit"

/** The class of a cell's text box (its width is the column's). */
export const MARKDOWN_CELL_TEXT = `xui-Markdown-cellText`

const px = (v: string) => Number.parseFloat(v) || 0

/** Fit the table inside `scroller` (a `.xui-Markdown-tableScroll`) to its width now. */
export function fitMarkdownTable(scroller: HTMLElement): void {
  const table = scroller.querySelector(`table`)
  if (!table) return
  const rows = Array.from(table.rows)
  const cells = rows.map((r) => Array.from(r.cells).map((c) => c.querySelector<HTMLElement>(`:scope > .${MARKDOWN_CELL_TEXT}`)))
  const n = Math.max(0, ...cells.map((r) => r.length))
  if (n === 0) return
  const boxes = cells.flatMap((r) => r.flatMap((b, c) => (b ? [{ b, c }] : [])))
  // Two layout passes for every cell at once (writes, then reads).
  const measure = (w: string) => {
    for (const { b } of boxes) b.style.width = w
    return boxes.map(({ b }) => b.getBoundingClientRect().width)
  }
  const max = measure(`max-content`)
  const min = measure(`min-content`)
  const columns: TableFitColumn[] = Array.from({ length: n }, () => ({ min: 0, max: 0 }))
  boxes.forEach(({ c }, i) => {
    columns[c]!.min = Math.max(columns[c]!.min, min[i]!)
    columns[c]!.max = Math.max(columns[c]!.max, max[i]!)
  })
  // The chrome: each column's cell paddings, the collapsed borders between.
  const first = rows[0]?.cells[0]
  const cs = first ? getComputedStyle(first) : null
  const pad = cs ? px(cs.paddingLeft) + px(cs.paddingRight) : 0
  const border = cs ? px(cs.borderLeftWidth) : 0
  const widths = fitTableColumns(columns, scroller.clientWidth - n * pad - (n + 1) * border)
  boxes.forEach(({ b, c }) => {
    b.style.width = `${widths[c] ?? 0}px`
  })
}

/** Keep the table inside `scroller` fitted: now, when its box resizes, when
 *  its cells change (a streamed reply) and when the fonts load. Returns
 *  the cleanup. */
export function watchMarkdownTable(scroller: HTMLElement): () => void {
  let width = -1
  let live = true
  const run = (force: boolean) => {
    if (!live || !scroller.isConnected) return
    const w = scroller.clientWidth
    if (!force && w === width) return
    width = w
    fitMarkdownTable(scroller)
  }
  run(true)
  const resize = typeof ResizeObserver === `undefined` ? null : new ResizeObserver(() => run(false))
  resize?.observe(scroller)
  const mutate = typeof MutationObserver === `undefined` ? null : new MutationObserver(() => run(true))
  mutate?.observe(scroller, { childList: true, subtree: true, characterData: true })
  void document.fonts?.ready.then(() => run(true))
  return () => {
    live = false
    resize?.disconnect()
    mutate?.disconnect()
  }
}
