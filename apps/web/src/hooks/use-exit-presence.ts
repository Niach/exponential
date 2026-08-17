import { useEffect, useRef, useState } from "react"

// EXP-523: keeps a value mounted for the length of ONE exit animation after it
// goes away, so a panel can animate out instead of vanishing on unmount.
//
// Web motion is CSS, so `prefers-reduced-motion` normally has authority on its
// own — but the UNMOUNT is JS, and a user who asked for stillness should not
// wait out a transition that is not playing. So the linger is read from the
// media query at effect time (not via a reactive hook: this is a one-shot
// decision per close, and a `useSyncExternalStore` subscription here would
// re-render every consumer on a setting change for no benefit).

function exitDelay(durationMs: number): number {
  if (typeof window === `undefined` || !window.matchMedia) return durationMs
  return window.matchMedia(`(prefers-reduced-motion: reduce)`).matches
    ? 0
    : durationMs
}

export interface ExitPresence<T> {
  /** The value to render — the live one, or the last one while it exits. */
  value: T | null
  /** False on the first frame and while exiting: drive the CSS from this. */
  open: boolean
}

/**
 * @param value the live value, or null when it should go away
 * @param key   a stable identity for `value` (object identity churns per render)
 * @param durationMs how long the exit animation runs
 */
export function useExitPresence<T>(
  value: T | null,
  key: string | null,
  durationMs: number
): ExitPresence<T> {
  const [open, setOpen] = useState(false)
  const [mountedKey, setMountedKey] = useState<string | null>(null)
  // Retains the outgoing value: by the time `key` is null the caller's live
  // query has already dropped the row, so there would be nothing left to draw.
  const lastValue = useRef<T | null>(null)
  if (value !== null) lastValue.current = value

  useEffect(() => {
    if (key !== null) {
      setMountedKey(key)
      // Open on the NEXT frame so the element mounts in its closed state and
      // the browser has a from-value to transition from.
      const raf = requestAnimationFrame(() => setOpen(true))
      return () => cancelAnimationFrame(raf)
    }
    setOpen(false)
    const timer = setTimeout(() => {
      setMountedKey(null)
      lastValue.current = null
    }, exitDelay(durationMs))
    return () => clearTimeout(timer)
  }, [key, durationMs])

  return {
    value: mountedKey === null ? null : (value ?? lastValue.current),
    open: open && mountedKey !== null,
  }
}
