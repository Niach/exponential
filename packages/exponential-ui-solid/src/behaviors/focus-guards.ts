// FEED-131: Radix `FocusGuards`.

import { onCleanup } from "solid-js"

let guardCount = 0
export function createGuard(): HTMLSpanElement {
  const el = document.createElement(`span`)
  el.setAttribute(`data-radix-focus-guard`, ``)
  el.tabIndex = 0
  el.style.outline = `none`
  el.style.opacity = `0`
  el.style.position = `fixed`
  el.style.pointerEvents = `none`
  return el
}
/** Radix `useFocusGuards`: two focusable spans at the body's edges while
 *  any overlay that needs them is up. */
export function useFocusGuards() {
  const edges = document.querySelectorAll(`[data-radix-focus-guard]`)
  document.body.insertAdjacentElement(`afterbegin`, edges[0] ?? createGuard())
  document.body.insertAdjacentElement(`beforeend`, edges[1] ?? createGuard())
  guardCount++
  onCleanup(() => {
    if (guardCount === 1) document.querySelectorAll(`[data-radix-focus-guard]`).forEach((n) => n.remove())
    guardCount--
  })
}
