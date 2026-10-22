// FEED-131: the Drawer's drag-to-dismiss (vaul-like, not Radix).

import { untrack, type Accessor } from "solid-js"

/** Drawer drag-to-dismiss: a drag toward the drawer's edge past 80 px (or a
 *  flick) closes it; anything less springs back. */
export function createDragToDismiss(side: Accessor<string>, enabled: Accessor<boolean>, onDismiss: () => void, reducedMotion: Accessor<boolean>) {
  let panel: HTMLElement | undefined
  let drag: { id: number; x: number; y: number; at: number } | null = null
  const sign = () => (side() === `bottom` || side() === `right` ? 1 : -1)
  const axis = () => (side() === `left` || side() === `right` ? `x` : `y`)
  const distance = (e: PointerEvent) => (drag ? Math.max(0, (axis() === `x` ? e.clientX - drag.x : e.clientY - drag.y) * sign()) : 0)
  const paint = (px: number, animate: boolean) => {
    if (!panel) return
    panel.style.transition = animate && !reducedMotion() ? `transform var(--xui-motion-standard) var(--xui-ease-decelerate)` : `none`
    panel.style.transform = px ? `translate${axis() === `x` ? `X` : `Y`}(${px * sign()}px)` : ``
  }
  return {
    setPanel: (el: HTMLElement) => (panel = el),
    onPointerDown: (e: PointerEvent) => {
      if (!untrack(enabled) || e.button !== 0) return
      drag = { id: e.pointerId, x: e.clientX, y: e.clientY, at: performance.now() }
      ;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
    },
    onPointerMove: (e: PointerEvent) => {
      if (untrack(enabled) && drag?.id === e.pointerId) paint(distance(e), false)
    },
    onPointerUp: (e: PointerEvent) => {
      if (!untrack(enabled) || drag?.id !== e.pointerId) return
      const px = distance(e)
      const dt = performance.now() - drag.at
      drag = null
      if (px > 80 || (px > 24 && dt > 0 && px / dt > 0.6)) {
        paint(0, false)
        onDismiss()
      } else paint(0, true)
    },
    onPointerCancel: () => {
      if (!untrack(enabled)) return
      drag = null
      paint(0, true)
    },
  }
}
