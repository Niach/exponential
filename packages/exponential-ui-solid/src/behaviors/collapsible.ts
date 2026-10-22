// FEED-131: Radix `Collapsible`'s content (the Accordion item's content).

import { createEffect, createSignal, on, onCleanup, onMount, untrack, type Accessor } from "solid-js"

/** Radix `CollapsibleContent`'s state: `shown()` = painted (open, or
 *  closing while its exit animation runs; the painter writes `hidden` and
 *  drops the children otherwise), `ref` = the content element, `style()` =
 *  Radix's variables in Radix's order (the measured
 *  `--radix-collapsible-content-*` appended by the render after a toggle,
 *  and the mount measurement by the render after mount;
 *  `transition-duration: 0s; animation-name: none` left in place on an item
 *  open at mount). `alias` = the outer primitive whose
 *  `--radix-<alias>-content-*` variables alias them (Accordion: `accordion`). */
export function createCollapsibleContent(open: Accessor<boolean>, alias?: string): { shown: Accessor<boolean>; ref: (el: HTMLElement) => void; style: () => Record<string, string> } {
  const [present, setPresent] = createSignal(untrack(open))
  const shown = () => open() || present()
  const [dims, setDims] = createSignal<{ h: number; w: number }>({ h: 0, w: 0 })
  let content: HTMLElement | undefined
  let original: { transitionDuration: string; animationName: string } | null = null
  let measured = { h: 0, w: 0 }
  let mountPrevented = untrack(shown)
  const measure = () => {
    const node = content
    if (!node) return
    original ??= { transitionDuration: node.style.transitionDuration, animationName: node.style.animationName }
    node.style.transitionDuration = `0s`
    node.style.animationName = `none`
    const rect = node.getBoundingClientRect()
    measured = { h: rect.height, w: rect.width }
    if (!mountPrevented) {
      node.style.transitionDuration = original.transitionDuration
      node.style.animationName = original.animationName
    }
  }
  // Radix Presence: a closing content stays painted while its exit
  // animation (`xui-accordion-close`) runs, until its `animationend`.
  let prevAnimation = `none`
  let waiting: (() => void) | null = null
  const animationName = () => (content ? getComputedStyle(content).animationName || `none` : `none`)
  const unmount = () => {
    measure()
    if (untrack(present)) {
      setPresent(false)
      setDims(measured)
    }
  }
  onMount(() => {
    measure()
    // React's surface re-renders once after mount (its measured box): that
    // render writes the mount measurement.
    setDims(measured)
    prevAnimation = untrack(shown) ? animationName() : `none`
    const onStart = (e: AnimationEvent) => {
      if (e.target === content) prevAnimation = animationName()
    }
    content?.addEventListener(`animationstart`, onStart)
    const raf = requestAnimationFrame(() => (mountPrevented = false))
    onCleanup(() => {
      cancelAnimationFrame(raf)
      content?.removeEventListener(`animationstart`, onStart)
      waiting?.()
    })
  })
  createEffect(
    on(
      open,
      (o) => {
        waiting?.()
        measure()
        if (o) {
          if (!untrack(present)) {
            setPresent(true)
            setDims(measured)
          }
          prevAnimation = animationName()
          return
        }
        const node = content
        const name = animationName()
        if (!node || name === `none` || getComputedStyle(node).display === `none` || prevAnimation === name) return unmount()
        const done = (e: AnimationEvent) => {
          if (e.target !== node || !animationName().includes(CSS.escape(e.animationName))) return
          waiting?.()
          prevAnimation = `none`
          unmount()
        }
        node.addEventListener(`animationend`, done)
        node.addEventListener(`animationcancel`, done)
        waiting = () => {
          node.removeEventListener(`animationend`, done)
          node.removeEventListener(`animationcancel`, done)
          waiting = null
        }
      },
      { defer: true }
    )
  )
  const style = () => {
    const d = dims()
    const out: Record<string, string> = {}
    if (d.h) out[`--radix-collapsible-content-height`] = `${d.h}px`
    if (d.w) out[`--radix-collapsible-content-width`] = `${d.w}px`
    if (alias) {
      out[`--radix-${alias}-content-height`] = `var(--radix-collapsible-content-height)`
      out[`--radix-${alias}-content-width`] = `var(--radix-collapsible-content-width)`
    }
    return out
  }
  return { shown, ref: (el) => (content = el), style }
}
