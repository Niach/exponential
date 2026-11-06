// VAPP-279 (docs/round-7-contract.md): a `variant: strip` Tabs on the web,
// framework-free so the React and Solid renderers share it: the fit (which
// tabs keep an item, measured off the DOM, the core's rule) and the reorder
// drag (pointer drag; a touch drags after a long press), the twin of the
// core's `surface/tabs.rs`.

import { TAB_DRAG_THRESHOLD, TAB_LONG_PRESS_MS } from "../layout"
import { moveTab, partitionTabs, tabDropSlot, tabKeySlot, tabReorder } from "../tabs-strip"
import { layoutSize } from "./primitives/measure"

/** The visible SLOTS of a strip (positions in strip order): `items` = every
 *  item element in strip order, folded ones too (they stay laid out, out of
 *  flow, so their widths are known); `more` = the `+N` button, `add` = the
 *  + button; `active` = the active slot. */
export function stripFit(list: HTMLElement, items: readonly HTMLElement[], more: HTMLElement | null, add: HTMLElement | null, active: number | null): number[] {
  const style = getComputedStyle(list)
  const gap = Number.parseFloat(style.columnGap) || 0
  const room = layoutSize(list).width - (Number.parseFloat(style.paddingLeft) || 0) - (Number.parseFloat(style.paddingRight) || 0)
  const addW = add ? layoutSize(add).width + gap : 0
  return partitionTabs(
    items.map((el) => layoutSize(el).width),
    room - addW,
    gap,
    more ? layoutSize(more).width : 0,
    active
  )
}

/** The `reorder` payload of a key on a strip tab, or null: the painter calls
 *  it for Ctrl/Cmd+Shift+ArrowLeft/ArrowRight only (`isReorderKey`). */
export function stripKeyMove(slots: readonly number[], index: number, key: string, rtl: boolean): { from: number; to: number } | null {
  const from = slots.indexOf(index)
  if (from < 0) return null
  const to = tabKeySlot(slots.length, from, key, rtl)
  return to === null ? null : tabReorder(slots, from, to)
}

/** Ctrl/Cmd+Shift+ArrowLeft/ArrowRight: the strip's keyboard reorder. */
export function isReorderKey(e: Pick<KeyboardEvent, `key` | `shiftKey` | `ctrlKey` | `metaKey` | `altKey`>): boolean {
  return (e.key === `ArrowLeft` || e.key === `ArrowRight`) && e.shiftKey && (e.ctrlKey || e.metaKey) && !e.altKey
}

export interface StripDragSource {
  /** The `tabs` array indexes in strip order (hidden ones too). */
  order: readonly number[]
  /** The `tabs` array indexes of the visible slots, in strip order. */
  slots: readonly number[]
  /** The item element of a `tabs` array index. */
  item: (index: number) => HTMLElement | null
  rtl: boolean
}

export interface StripDragHandlers {
  /** The live preview order (array indexes), or null = the array order. */
  preview: (order: number[] | null) => void
  /** The drop: the `reorder` payload, null = no move. */
  drop: (move: { from: number; to: number } | null) => void
}

/**
 * A pointerdown on a strip tab `target` (`index` = its array index): a mouse or pen
 * drags once it travels `tabDragThreshold`; a touch drags after resting
 * `tabLongPressMs` (moving first = a scroll or a press, never a drag). The
 * source is read when the drag STARTS (the core's `drag {phase: start}`):
 * the dragged centre + the pointer delta passes the other centres
 * (`tabDropSlot`), the order previews, the release drops.
 */
export function startStripDrag(e: PointerEvent, target: HTMLElement | null, index: number, source: () => StripDragSource, on: StripDragHandlers): void {
  if (e.button !== 0) return
  const startX = e.clientX
  const startY = e.clientY
  const touch = e.pointerType === `touch`
  let state: { from: number; slots: readonly number[]; centers: number[]; order: readonly number[]; rtl: boolean } | null = null
  let armed = !touch
  let timer: ReturnType<typeof setTimeout> | undefined
  const begin = () => {
    const s = source()
    const from = s.slots.indexOf(index)
    if (from < 0) return
    const centers = s.slots.map((i) => {
      const r = s.item(i)?.getBoundingClientRect()
      return r ? r.left + r.width / 2 : 0
    })
    state = { from, slots: s.slots, centers, order: s.order, rtl: s.rtl }
    try {
      target?.setPointerCapture(e.pointerId)
    } catch {
      // A synthetic or ended pointer: the window listeners still follow it.
    }
  }
  const slotAt = (x: number) => (state ? tabDropSlot(state.centers, state.from, x - startX, state.rtl) : 0)
  const move = (ev: PointerEvent) => {
    if (ev.pointerId !== e.pointerId) return
    const dx = ev.clientX - startX
    const far = Math.hypot(dx, ev.clientY - startY) >= TAB_DRAG_THRESHOLD
    if (!state) {
      if (!armed) {
        if (far) finish(null)
        return
      }
      if (!far) return
      begin()
      if (!state) return finish(null)
    }
    ev.preventDefault()
    const s = state as { from: number; slots: readonly number[]; order: readonly number[] }
    const moved = tabReorder(s.slots, s.from, slotAt(ev.clientX))
    on.preview(moved ? moveTab(s.order, s.order.indexOf(moved.from), s.order.indexOf(moved.to)) : null)
  }
  const up = (ev: PointerEvent) => {
    if (ev.pointerId !== e.pointerId) return
    finish(state && ev.type === `pointerup` ? tabReorder(state.slots, state.from, slotAt(ev.clientX)) : null)
  }
  const finish = (result: { from: number; to: number } | null) => {
    clearTimeout(timer)
    window.removeEventListener(`pointermove`, move)
    window.removeEventListener(`pointerup`, up)
    window.removeEventListener(`pointercancel`, up)
    const dragged = state !== null
    state = null
    if (dragged) {
      on.preview(null)
      on.drop(result)
      // The release after a drag is no click.
      window.addEventListener(`click`, swallow, { capture: true, once: true })
      setTimeout(() => window.removeEventListener(`click`, swallow, { capture: true }), 0)
    }
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
}

function swallow(e: Event) {
  e.stopPropagation()
  e.preventDefault()
}
