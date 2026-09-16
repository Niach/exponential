import { useCallback, useEffect, useRef } from "react"
import { readTabMemory, writeTabMemory } from "@/lib/work-tab-memory"

/** How long a remount keeps trying to reach the remembered offset while its
 *  content is still laying out (deferred editors, synced rows, images). */
const RESTORE_WINDOW_MS = 1500

/**
 * EXP-894: a scroll container that comes back where it was left after a
 * work-tab switch unmounted it. Returns a ref callback for the scroller.
 *
 * The offset is written on scroll (a Map write — no React state, no render);
 * a remount restores it, and since the body often grows AFTER mount it
 * re-applies on content growth for a short window, stopping the moment the
 * reader scrolls by hand (wheel, touch, key) so it never fights them.
 */
export function useRememberedScroll(
  owner: string,
  slot: string
): (node: HTMLElement | null) => void {
  const cleanupRef = useRef<(() => void) | null>(null)

  const ref = useCallback(
    (node: HTMLElement | null) => {
      cleanupRef.current?.()
      cleanupRef.current = null
      if (!node) return

      const target = readTabMemory<number>(owner, slot) ?? 0
      let restoring = target > 0
      const stopRestoring = () => {
        restoring = false
      }
      const apply = () => {
        if (!restoring) return
        node.scrollTop = target
        if (Math.abs(node.scrollTop - target) < 1) restoring = false
      }
      const onScroll = () => {
        // Programmatic restores fire scroll too; only record once settled.
        if (restoring) return
        writeTabMemory(owner, slot, node.scrollTop > 0 ? node.scrollTop : null)
      }

      apply()
      node.addEventListener(`scroll`, onScroll, { passive: true })
      const userEvents = [`wheel`, `touchstart`, `keydown`, `pointerdown`]
      for (const type of userEvents) {
        node.addEventListener(type, stopRestoring, { passive: true })
      }
      let observer: ResizeObserver | null = null
      let timer: ReturnType<typeof setTimeout> | null = null
      if (restoring && typeof ResizeObserver !== `undefined`) {
        observer = new ResizeObserver(apply)
        for (const child of Array.from(node.children)) observer.observe(child)
        timer = setTimeout(() => {
          restoring = false
          observer?.disconnect()
        }, RESTORE_WINDOW_MS)
      }

      cleanupRef.current = () => {
        node.removeEventListener(`scroll`, onScroll)
        for (const type of userEvents) {
          node.removeEventListener(type, stopRestoring)
        }
        observer?.disconnect()
        if (timer) clearTimeout(timer)
      }
    },
    [owner, slot]
  )

  useEffect(() => () => cleanupRef.current?.(), [])

  return ref
}
