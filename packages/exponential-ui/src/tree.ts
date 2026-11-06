// #VAPP-285 (docs/round-7-contract.md): the Tree's two pure rules, the same
// in the Rust core (`eui-core` `tree.rs`) and locked by fixtures/tree.json.
//
// treeRows: nested items → the flat rows the `Tree` macro draws (one Row
// each, depth → the tree guides), depth-first. A node with `children` (an
// array, even empty) or `hasChildren: true` is a BRANCH; one with
// `hasChildren` and no `children` array is LAZY: opening it fires `load`
// instead of `toggle`, and while it is open a LOADING row sits under it. A
// literal `expanded` (ids) decides now: only the shown rows come out, each
// `open` a boolean. A bound `expanded` (a binding or a call) cannot: EVERY
// row comes out, `shown`/`open`/`next` become `expr` calls over it (CEL
// `exists`) and `toggled` the list a fold writes back (the two-way `$set`).
//
// treeKey: the WAI-ARIA tree keys over the VISIBLE rows (level 1 = a root,
// `expanded` true/false on a branch, absent on a leaf): Up/Down move, Home/End
// go to the ends, the open key (ArrowRight, ArrowLeft in rtl) opens a closed
// branch or enters an open one (its first child), the close key closes an
// open branch or goes to the parent. Enter/Space stay the Box's press.

import { isDynamic, MAX_RANGE_ITEMS } from "./expr"

export type TreeRowKind = `branch` | `leaf` | `loading`

/** One flat row of a Tree (what the macro's `$each` iterates). */
export interface TreeRow {
  id: string
  title: string
  icon?: unknown
  meta?: string
  /** 0 = a root. */
  depth: number
  /** depth + 1 (aria-level). */
  level: number
  kind: TreeRowKind
  /** A branch only: whether its children show (a call when bound). */
  open?: unknown
  /** A branch only: the fold's next state, `!open` (a call when bound). */
  next?: unknown
  /** A branch only, bound `expanded` only: the list a fold writes back. */
  toggled?: unknown
  /** true, or (bound `expanded`) the call testing every ancestor is open. */
  shown: unknown
  /** The routing the macro reads (`""` = none): `pressEvent` = the author
   *  event a press fires (`select` on a leaf, the fold event on a branch);
   *  `fold` = the fold event (`toggle`, `load` on a lazy branch); the part
   *  events that write back: `selectOn` (`press` on a leaf: `value`),
   *  `foldOn` (`press` on a branch: `expanded`), `toggleOn` (`toggle` on a
   *  branch: the fold keys). The loading row routes nothing. */
  pressEvent: string
  fold: string
  selectOn: string
  foldOn: string
  toggleOn: string
  pressable: boolean
}

interface Item {
  id?: unknown
  title?: unknown
  icon?: unknown
  meta?: unknown
  children?: unknown
  hasChildren?: unknown
}

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)

const expr = (text: string, vars: Record<string, unknown>, fallback: unknown) => ({ call: `expr`, args: { expr: text, vars, default: fallback } })

/** `e.exists(x, x == a0) && …` over the ids (a bound `expanded` = `e`). */
function allOpen(expanded: unknown, ids: string[]): unknown {
  if (ids.length === 0) return true
  const vars: Record<string, unknown> = { e: expanded }
  ids.forEach((id, i) => (vars[`a${i}`] = id))
  return expr(ids.map((_, i) => `e.exists(x, x == a${i})`).join(` && `), vars, false)
}

/** The flat rows of nested `items` under `expanded` (module comment). */
export function treeRows(items: unknown, expanded: unknown): TreeRow[] {
  const bound = isDynamic(expanded)
  const open = new Set(Array.isArray(expanded) ? expanded.filter((v) => typeof v === `string`) : [])
  const rows: TreeRow[] = []
  const walk = (list: unknown, depth: number, ancestors: string[]) => {
    if (!Array.isArray(list)) return
    for (const raw of list) {
      if (rows.length >= MAX_RANGE_ITEMS) return
      if (!isObj(raw)) continue
      const item = raw as Item
      if (typeof item.id !== `string` || item.id === ``) continue
      const id = item.id
      const hasArray = Array.isArray(item.children)
      const branch = hasArray || item.hasChildren === true
      const lazy = item.hasChildren === true && !hasArray
      const shown = bound ? allOpen(expanded, ancestors) : true
      const row: TreeRow = {
        id,
        title: typeof item.title === `string` ? item.title : id,
        depth,
        level: depth + 1,
        kind: branch ? `branch` : `leaf`,
        shown,
        pressEvent: branch ? (lazy ? `load` : `toggle`) : `select`,
        fold: branch ? (lazy ? `load` : `toggle`) : ``,
        selectOn: branch ? `` : `press`,
        foldOn: branch ? `press` : ``,
        toggleOn: branch ? `toggle` : ``,
        pressable: true,
      }
      if (item.icon !== undefined) row.icon = item.icon
      if (typeof item.meta === `string` && item.meta !== ``) row.meta = item.meta
      let isOpen: unknown = false
      if (branch) {
        if (bound) {
          isOpen = expr(`e.exists(x, x == i)`, { e: expanded, i: id }, false)
          row.open = isOpen
          row.next = { call: `not`, args: { value: isOpen } }
          row.toggled = expr(`e.exists(x, x == i) ? e.filter(x, x != i) : e + [i]`, { e: expanded, i: id }, [id])
        } else {
          isOpen = open.has(id)
          row.open = isOpen
          row.next = !isOpen
        }
      }
      // The default glyphs: a folder (open while open), a file.
      if (row.icon === undefined) row.icon = !branch ? `ui-file` : bound ? { call: `cond`, args: { if: isOpen, then: `ui-folder-open`, else: `ui-folder` } } : isOpen === true ? `ui-folder-open` : `ui-folder`
      rows.push(row)
      if (!branch || (!bound && isOpen !== true)) continue
      const inner = [...ancestors, id]
      if (lazy) {
        if (rows.length >= MAX_RANGE_ITEMS) return
        rows.push({ id: `${id}/loading`, title: `$string.loading`, depth: depth + 1, level: depth + 2, kind: `loading`, shown: bound ? allOpen(expanded, inner) : true, pressEvent: ``, fold: ``, selectOn: ``, foldOn: ``, toggleOn: ``, pressable: false })
        continue
      }
      walk(item.children, depth + 1, inner)
    }
  }
  walk(items, 0, [])
  return rows
}

/** One visible row as treeKey reads it. */
export interface TreeKeyRow {
  level: number
  /** true/false on a branch, absent on a leaf (and the loading row). */
  expanded?: boolean
}

/** What a key does on row `index`: move the focus, fold the row, or nothing. */
export type TreeKeyResult = { focus: number } | { toggle: true } | null

/** The WAI-ARIA tree keys (module comment). `key` = a DOM key name. */
export function treeKey(rows: readonly TreeKeyRow[], index: number, key: string, rtl: boolean): TreeKeyResult {
  const row = rows[index]
  if (!row) return null
  const openKey = rtl ? `ArrowLeft` : `ArrowRight`
  const closeKey = rtl ? `ArrowRight` : `ArrowLeft`
  const last = rows.length - 1
  if (key === `ArrowDown`) return index < last ? { focus: index + 1 } : null
  if (key === `ArrowUp`) return index > 0 ? { focus: index - 1 } : null
  if (key === `Home`) return index > 0 ? { focus: 0 } : null
  if (key === `End`) return index < last ? { focus: last } : null
  if (key === openKey) {
    if (row.expanded === false) return { toggle: true }
    if (row.expanded === true) {
      const next = rows[index + 1]
      return next && next.level === row.level + 1 ? { focus: index + 1 } : null
    }
    return null
  }
  if (key === closeKey) {
    if (row.expanded === true) return { toggle: true }
    for (let i = index - 1; i >= 0; i--) {
      if (rows[i]!.level < row.level) return rows[i]!.level === row.level - 1 ? { focus: i } : null
    }
    return null
  }
  return null
}
