import { useEffect, useState } from "react"
import {
  KEYBOARD_MIN_OCCLUSION,
  keyboardOcclusion,
} from "@/lib/keyboard-caret"

// EXP-568 — how many pixels of the LAYOUT viewport the on-screen keyboard
// currently covers, so a bar pinned to the bottom of the page can ride above
// it. Mobile keyboards overlay the layout viewport without resizing it
// (see lib/keyboard-caret.ts), so `bottom: 0` alone parks the bar underneath
// the keys; `bottom: inset` lands it on the keyboard's top edge.
//
// Both visualViewport events matter: `resize` fires when the keyboard opens
// or closes, `scroll` when iOS PANS the visual viewport (offsetTop moves
// while the height stays put) — missing the latter leaves the bar drifting.

export interface KeyboardInset {
  /** Pixels hidden behind the keyboard; 0 when it is closed. */
  inset: number
  keyboardOpen: boolean
}

export function useKeyboardInset(): KeyboardInset {
  const [inset, setInset] = useState(0)

  useEffect(() => {
    const viewport = window.visualViewport
    if (!viewport) return
    let frame = 0
    // Both events can fire several times per keyboard animation frame —
    // collapse the burst into one measurement.
    const measure = () => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(() => {
        setInset(
          keyboardOcclusion(
            window.innerHeight,
            viewport.offsetTop,
            viewport.height
          )
        )
      })
    }
    measure()
    viewport.addEventListener(`resize`, measure)
    viewport.addEventListener(`scroll`, measure)
    return () => {
      cancelAnimationFrame(frame)
      viewport.removeEventListener(`resize`, measure)
      viewport.removeEventListener(`scroll`, measure)
    }
  }, [])

  return { inset, keyboardOpen: inset >= KEYBOARD_MIN_OCCLUSION }
}
