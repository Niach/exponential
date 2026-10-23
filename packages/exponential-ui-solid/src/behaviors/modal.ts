// FEED-131: Radix's modal content effects (`hideOthers` + the scroll lock).

import { onCleanup } from "solid-js"
import { hideOthers } from "aria-hidden"
import { useScrollLock } from "./scroll-lock"

/** Radix's modal content: the rest of the page `aria-hidden` (live regions
 *  and scripts kept), the focus guards, the scroll lock. */
export function useModalEffects(el: HTMLElement, lockScroll: boolean) {
  const undo = hideOthers(el)
  onCleanup(undo)
  if (lockScroll) useScrollLock()
}
