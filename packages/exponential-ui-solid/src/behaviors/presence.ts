// FEED-131: Radix `Presence`: a closing part stays mounted while the exit
// animation its closed state starts runs.

import { createEffect, createSignal, on, onCleanup, untrack, type Accessor } from "solid-js"

/** Radix `Presence`: mounted while `present`, and on close until the exit
 *  animation the closed state starts has ended (none = at once). */
export function createPresence(present: Accessor<boolean>): { mounted: Accessor<boolean>; ref: (el: HTMLElement) => void } {
  const [mounted, setMounted] = createSignal(untrack(present))
  let node: HTMLElement | undefined
  let openName = `none`
  let stop: (() => void) | undefined
  const nameOf = (el: HTMLElement) => getComputedStyle(el).animationName || `none`
  createEffect(
    on(present, (p) => {
      stop?.()
      stop = undefined
      if (p) {
        setMounted(true)
        return
      }
      const el = node
      if (!el || !el.isConnected) return setMounted(false)
      const styles = getComputedStyle(el)
      const current = styles.animationName || `none`
      if (current === `none` || styles.display === `none` || current === openName) return setMounted(false)
      const done = (e: AnimationEvent) => {
        if (e.target !== el || !nameOf(el).split(`,`).map((s) => s.trim()).includes(e.animationName)) return
        stop?.()
        setMounted(false)
      }
      el.addEventListener(`animationend`, done)
      el.addEventListener(`animationcancel`, done)
      stop = () => {
        el.removeEventListener(`animationend`, done)
        el.removeEventListener(`animationcancel`, done)
      }
    }, { defer: true })
  )
  onCleanup(() => stop?.())
  return {
    mounted,
    ref: (el) => {
      node = el
      el.addEventListener(`animationstart`, (e) => {
        if (e.target === el && untrack(present)) openName = nameOf(el)
      })
    },
  }
}
