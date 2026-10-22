// FEED-131: Radix `useSize`. (The renderer's own `createElementSize` in
// platform.ts is the surface's measured box: a different contract.)

import { createEffect, createSignal, onCleanup, type Accessor } from "solid-js"

/** Radix's `useSize`: the element's border-box size once mounted, then
 *  followed by a ResizeObserver (undefined before the first measure). */
export function createElementSize(el: Accessor<HTMLElement | undefined>): Accessor<{ width: number; height: number } | undefined> {
  const [size, setSize] = createSignal<{ width: number; height: number } | undefined>(undefined, { equals: (a, b) => a?.width === b?.width && a?.height === b?.height })
  createEffect(() => {
    const node = el()
    if (!node) return setSize(undefined)
    setSize({ width: node.offsetWidth, height: node.offsetHeight })
    if (typeof ResizeObserver === `undefined`) return
    const ro = new ResizeObserver((entries) => {
      const entry = entries[0]
      if (!entry) return
      let width: number
      let height: number
      if (`borderBoxSize` in entry && entry.borderBoxSize) {
        const box = (Array.isArray(entry.borderBoxSize) ? entry.borderBoxSize[0] : entry.borderBoxSize) as ResizeObserverSize
        width = box.inlineSize
        height = box.blockSize
      } else {
        width = node.offsetWidth
        height = node.offsetHeight
      }
      setSize({ width, height })
    })
    ro.observe(node, { box: `border-box` })
    onCleanup(() => ro.unobserve(node))
  })
  return size
}
