// EXP-1832 (fixtures/list-reorder.json): a `reorderable` List on the web,
// framework-free so the React and Solid renderers (and the web app's own
// sidebar, which wears the same look) share it: the pointer drag (a mouse or
// pen past `tabDragThreshold`, a touch after `tabLongPressMs`), the drop rule
// (`listDropSlot`, the core's `list_reorder.rs` twin) and the PAINT every
// renderer shows: the dragged row dims, a lifted copy follows the pointer, a
// 2px primary line marks before/after (from the row's indent, a small ring at
// its start) and a primary ring + faint fill marks into. Esc cancels.

import { TAB_DRAG_THRESHOLD, TAB_LONG_PRESS_MS } from "../layout"
import { boardDropSlot, listDropSlot, listMoveContext, type BoardColumn, type BoardDrop, type ListDrop, type ListMoveContext, type ListReorderRow } from "../list-reorder"

/** The DOM contract of a reorderable row: the element carries
 *  `data-reorder-key` (+ `data-reorder-depth`, `data-reorder-group`,
 *  `data-reorder-indent` = px from the row's start to its content). */
export const REORDER_KEY_ATTR = `data-reorder-key`
export const REORDER_DEPTH_ATTR = `data-reorder-depth`
export const REORDER_GROUP_ATTR = `data-reorder-group`
export const REORDER_INDENT_ATTR = `data-reorder-indent`
/** genUI round 4 (fixtures/board-drop.json): a reorderable List that is a
 *  Board column carries `data-reorder-board` = its internal `board` prop;
 *  the Lists of one surface sharing it trade rows. */
export const REORDER_BOARD_ATTR = `data-reorder-board`

/** Each Board column List's own `move` (the TARGET column fires it). */
const boardMoves = new WeakMap<Element, (context: Record<string, string>) => void>()

/** A Board column List (its root element) and its `move`: registered while
 *  mounted; returns the unregister. */
export function registerBoardList(el: Element, move: (context: Record<string, string>) => void): () => void {
  boardMoves.set(el, move)
  return () => {
    if (boardMoves.get(el) === move) boardMoves.delete(el)
  }
}

/** The dragged row's opacity while it is lifted. */
export const LIST_DRAG_DIM = 0.4
/** The drop line's thickness and its start ring's diameter (px). */
export const LIST_DROP_LINE = 2
export const LIST_DROP_RING = 8

export interface ListDragSource {
  /** The list's rendered rows, in order (each with the DOM contract above). */
  rows: () => HTMLElement[]
  maxDepth: number
  /** The List's root when it is a Board column (it carries
   *  `data-reorder-board`): the drop then spans the Board's columns. */
  board?: Element
  /** The paint colour (a CSS colour; default the genUI primary). */
  color?: string
}

export interface ListDragHandlers {
  /** The drop: the `move` context {id, before | after | into}. Not called
   *  for a cancel or a release with no slot. */
  move: (context: ListMoveContext) => void
  /** Optional: the live drag state, for a renderer that paints itself. */
  state?: (state: { key: string; drop: ListDrop | null } | null) => void
}

const rowOf = (el: HTMLElement): ListReorderRow => {
  const rect = el.getBoundingClientRect()
  return {
    key: el.getAttribute(REORDER_KEY_ATTR) ?? ``,
    top: rect.top,
    height: rect.height,
    depth: Number(el.getAttribute(REORDER_DEPTH_ATTR) ?? 0) || 0,
    group: el.getAttribute(REORDER_GROUP_ATTR) === `true`,
  }
}

/** A List's OWN reorderable rows, in order: its flat rows
 *  (`.xui-list-row`) or its windowed ones (`.xui-list-window >
 *  .xui-list-item`), never a nested List's. */
export function listReorderRows(root: Element): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(`:scope > .xui-list-row[${REORDER_KEY_ATTR}], :scope > .xui-list-window > .xui-list-item[${REORDER_KEY_ATTR}]`))
}

const EDITABLE = `input, textarea, select, [contenteditable]:not([contenteditable="false"])`

/** A pointerdown at `target` inside the List `root`: the own row it lands
 *  on (null = outside one, a nested reorderable List's row, or inside a
 *  field, which keeps its own pointer). */
export function listReorderStart(root: Element, target: EventTarget | null): { key: string; row: HTMLElement } | null {
  const el = target instanceof Element ? target : null
  if (!el || el.closest(EDITABLE) || (el as HTMLElement).isContentEditable) return null
  const row = listReorderRows(root).find((r) => r.contains(el))
  if (!row) return null
  // A nested reorderable List's row takes its own drag.
  const inner = el.closest(`[${REORDER_KEY_ATTR}]`)
  if (inner && inner !== row) return null
  return { key: row.getAttribute(REORDER_KEY_ATTR) ?? ``, row }
}

/** The indicator's box in viewport px for a drop over `row`. */
export function listDropBox(row: DOMRect, indent: number, drop: ListDrop): { left: number; top: number; width: number; height: number } {
  if (drop.position === `into`) return { left: row.left, top: row.top, width: row.width, height: row.height }
  const y = drop.position === `before` ? row.top : row.bottom
  return { left: row.left + indent, top: y - LIST_DROP_LINE / 2, width: Math.max(row.width - indent, 0), height: LIST_DROP_LINE }
}

/**
 * A pointerdown on a reorderable row (`key`): starts the drag once it
 * travels the threshold (a touch: once it rests for the long press). The
 * rows are read when the drag STARTS; the pointer's y picks the drop.
 */
export function startListDrag(e: PointerEvent, key: string, source: ListDragSource, on: ListDragHandlers): void {
  if (e.button !== 0) return
  const startX = e.clientX
  const startY = e.clientY
  const touch = e.pointerType === `touch`
  const color = source.color ?? `var(--xui-color-primary)`
  let armed = !touch
  let timer: ReturnType<typeof setTimeout> | undefined
  let drag: { els: HTMLElement[]; rows: ListReorderRow[]; dragged: HTMLElement; ghost: HTMLElement; indicator: HTMLElement; offsetX: number; offsetY: number; drop: ListDrop | null } | null = null
  // A Board drag: its column Lists (measured at the start) and the drop.
  let board: { lists: Element[]; columns: BoardColumn[]; els: HTMLElement[][]; drop: BoardDrop | null } | null = null

  const begin = () => {
    const els = source.rows()
    const dragged = els.find((el) => el.getAttribute(REORDER_KEY_ATTR) === key)
    if (!dragged) return
    const rect = dragged.getBoundingClientRect()
    const ghost = dragged.cloneNode(true) as HTMLElement
    ghost.setAttribute(`aria-hidden`, `true`)
    ghost.removeAttribute(REORDER_KEY_ATTR)
    Object.assign(ghost.style, {
      position: `fixed`,
      left: `${rect.left + 12}px`,
      top: `${rect.top}px`,
      width: `${rect.width}px`,
      height: `${rect.height}px`,
      margin: `0`,
      pointerEvents: `none`,
      zIndex: `2147483000`,
      opacity: `0.95`,
      borderRadius: `6px`,
      boxShadow: `0 8px 24px rgb(0 0 0 / 0.35)`,
      background: `var(--xui-color-popover, var(--popover, #27272a))`,
    } satisfies Partial<CSSStyleDeclaration>)
    // Inside the row's genUI surface: its scoped theme (the `--xui-*`
    // colours, the font, the recipe classes) paints the copy and the drop;
    // under <body> neither would resolve. A host list outside a surface
    // (the app's sidebar) passes a resolvable `color`.
    const layer = dragged.closest(`.xui-surface`) ?? document.body
    layer.appendChild(ghost)
    const indicator = document.createElement(`div`)
    indicator.setAttribute(`aria-hidden`, `true`)
    indicator.setAttribute(`data-reorder-indicator`, ``)
    Object.assign(indicator.style, { position: `fixed`, pointerEvents: `none`, zIndex: `2147482999`, display: `none` })
    layer.appendChild(indicator)
    // A press-and-drag would select the rows' text.
    window.getSelection()?.removeAllRanges()
    dragged.style.opacity = String(LIST_DRAG_DIM)
    drag = { els, rows: els.map(rowOf), dragged, ghost, indicator, offsetX: startX - rect.left, offsetY: startY - rect.top, drop: null }
    const name = source.board?.getAttribute(REORDER_BOARD_ATTR)
    if (name) {
      const lists = Array.from((source.board!.closest(`.xui-surface`) ?? document).querySelectorAll(`[${REORDER_BOARD_ATTR}]`)).filter((l) => l.getAttribute(REORDER_BOARD_ATTR) === name)
      const rowsOf = lists.map((l) => listReorderRows(l))
      board = {
        lists,
        els: rowsOf,
        columns: lists.map((l, i) => {
          const r = l.getBoundingClientRect()
          return { key: String(i), left: r.left, width: r.width, rows: rowsOf[i].map(rowOf) }
        }),
        drop: null,
      }
    }
    on.state?.({ key, drop: null })
    try {
      ;(e.target as Element | null)?.setPointerCapture?.(e.pointerId)
    } catch {
      // A synthetic or ended pointer: the window listeners still follow it.
    }
  }

  const paint = (drop: ListDrop | null, column?: number) => {
    if (!drag) return
    drag.drop = drop
    on.state?.({ key, drop })
    const ind = drag.indicator
    // A Board drop on an empty column: the into highlight over its List.
    const empty = board && column !== undefined && !drop ? board.lists[column] : null
    if (!drop && !empty) {
      ind.style.display = `none`
      return
    }
    const rows = board && column !== undefined ? board.els[column] : drag.els
    const el = drop ? rows.find((row) => row.getAttribute(REORDER_KEY_ATTR) === drop.key) : null
    if (drop && !el) return
    const indent = el ? Number(el.getAttribute(REORDER_INDENT_ATTR) ?? 0) || 0 : 0
    const box = el && drop ? listDropBox(el.getBoundingClientRect(), indent, drop) : listDropBox(empty!.getBoundingClientRect(), 0, { key: ``, position: `into` })
    drop ??= { key: ``, position: `into` }
    Object.assign(ind.style, { display: `block`, left: `${box.left}px`, top: `${box.top}px`, width: `${box.width}px`, height: `${box.height}px` })
    ind.replaceChildren()
    if (drop.position === `into`) {
      Object.assign(ind.style, { background: `color-mix(in oklab, ${color} 14%, transparent)`, boxShadow: `inset 0 0 0 ${LIST_DROP_LINE}px ${color}`, borderRadius: `6px` })
    } else {
      Object.assign(ind.style, { background: color, boxShadow: `none`, borderRadius: `1px` })
      const ring = document.createElement(`div`)
      Object.assign(ring.style, {
        position: `absolute`,
        left: `${-LIST_DROP_RING / 2}px`,
        top: `${LIST_DROP_LINE / 2 - LIST_DROP_RING / 2}px`,
        width: `${LIST_DROP_RING}px`,
        height: `${LIST_DROP_RING}px`,
        borderRadius: `9999px`,
        boxSizing: `border-box`,
        border: `${LIST_DROP_LINE}px solid ${color}`,
        background: `var(--xui-color-background, var(--background, #09090b))`,
      })
      ind.appendChild(ring)
    }
  }

  const move = (ev: PointerEvent) => {
    if (ev.pointerId !== e.pointerId) return
    const far = Math.hypot(ev.clientX - startX, ev.clientY - startY) >= TAB_DRAG_THRESHOLD
    if (!drag) {
      if (!armed) {
        if (far) finish(false)
        return
      }
      if (!far) return
      begin()
      if (!drag) return finish(false)
    }
    ev.preventDefault()
    const d = drag as NonNullable<typeof drag>
    d.ghost.style.top = `${ev.clientY - d.offsetY}px`
    if (board) {
      // A Board card follows the pointer on both axes.
      d.ghost.style.left = `${ev.clientX - d.offsetX + 12}px`
      const drop = boardDropSlot(board.columns, key, ev.clientX, ev.clientY)
      board.drop = drop
      const column = drop ? Number(drop.column) : undefined
      paint(drop?.key && drop.position ? { key: drop.key, position: drop.position } : null, column)
      return
    }
    paint(listDropSlot(d.rows, key, ev.clientY, source.maxDepth))
  }
  const up = (ev: PointerEvent) => {
    if (ev.pointerId !== e.pointerId) return
    finish(ev.type === `pointerup`)
  }
  const keydown = (ev: KeyboardEvent) => {
    if (ev.key !== `Escape` || !drag) return
    ev.preventDefault()
    ev.stopPropagation()
    finish(false)
  }
  const finish = (commit: boolean) => {
    clearTimeout(timer)
    window.removeEventListener(`pointermove`, move)
    window.removeEventListener(`pointerup`, up)
    window.removeEventListener(`pointercancel`, up)
    window.removeEventListener(`keydown`, keydown, true)
    document.removeEventListener(`selectstart`, swallow, true)
    const d = drag
    drag = null
    if (!d) return
    d.ghost.remove()
    d.indicator.remove()
    d.dragged.style.opacity = ``
    on.state?.(null)
    const b = board
    board = null
    if (b) {
      // The TARGET column's List fires (its macro adds `column`).
      if (commit && b.drop) {
        const target = b.lists[Number(b.drop.column)]
        const context: Record<string, string> = b.drop.key && b.drop.position ? { id: key, [b.drop.position]: b.drop.key } : { id: key }
        boardMoves.get(target)?.(context)
      }
    } else if (commit && d.drop) on.move(listMoveContext(key, d.drop))
    // The release after a drag is no click.
    window.addEventListener(`click`, swallow, { capture: true, once: true })
    setTimeout(() => window.removeEventListener(`click`, swallow, { capture: true }), 0)
  }
  if (touch) {
    timer = setTimeout(() => {
      armed = true
      begin()
    }, TAB_LONG_PRESS_MS)
  }
  window.addEventListener(`pointermove`, move)
  window.addEventListener(`pointerup`, up)
  window.addEventListener(`pointercancel`, up)
  window.addEventListener(`keydown`, keydown, true)
  document.addEventListener(`selectstart`, swallow, true)
}

function swallow(e: Event) {
  e.stopPropagation()
  e.preventDefault()
}
