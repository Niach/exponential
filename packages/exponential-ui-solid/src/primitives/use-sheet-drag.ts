import type { JSX } from "solid-js"

// EXP-687 (React's use-sheet-drag.ts): drag-to-dismiss for the mobile bottom
// sheets, on the grabber and the header area (which spreads the same props),
// active below `sm` only; the same thresholds and the same hidden-Close path.
const SHEET_MEDIA_QUERY = `(max-width: 639px)`

/** Past this many px the sheet closes regardless of speed. */
const DISMISS_DISTANCE_PX = 80
/** A flick: px per ms, over the (much smaller) FLICK_DISTANCE_PX. */
const DISMISS_VELOCITY = 0.6
const FLICK_DISTANCE_PX = 24
/** The panel's own slide-out duration (the `standard` motion token). */
const DISMISS_ANIMATION_MS = 180
const DISMISS_TRANSITION = `transform var(--motion-duration-standard, ${DISMISS_ANIMATION_MS}ms) var(--motion-ease-accelerate, cubic-bezier(0.3, 0, 1, 1))`
const SNAP_BACK_TRANSITION = `transform var(--motion-duration-standard, ${DISMISS_ANIMATION_MS}ms) var(--motion-ease-decelerate, cubic-bezier(0, 0, 0.2, 1))`

/** Did a downward drag of `dy` px over `dtMs` ms earn a dismissal? Upward
 *  drags never do. */
export function shouldDismissSheet(dy: number, dtMs: number): boolean {
  if (dy > DISMISS_DISTANCE_PX) {
    return true
  }
  if (dy <= FLICK_DISTANCE_PX) {
    return false
  }
  return dtMs > 0 && dy / dtMs > DISMISS_VELOCITY
}

export interface SheetDragHandleProps {
  onPointerDown: (event: PointerEvent & { currentTarget: HTMLElement }) => void
  onPointerMove: (event: PointerEvent) => void
  onPointerUp: (event: PointerEvent) => void
  onPointerCancel: (event: PointerEvent) => void
  style: JSX.CSSProperties
}

/** React's `useSheetDrag`: `panel()` = the sheet's panel element. */
export function createSheetDrag(opts: { panel: () => HTMLElement | null | undefined; onDismiss: () => void }): { handleProps: SheetDragHandleProps } {
  let drag: { pointerId: number; y: number; at: number } | null = null

  const paint = (transform: string, transition: string) => {
    const panel = opts.panel()
    if (!panel) {
      return
    }
    panel.style.transition = transition
    panel.style.transform = transform
  }

  const release = () => {
    const panel = opts.panel()
    if (!panel) {
      return
    }
    panel.style.transition = ``
    panel.style.transform = ``
  }

  const handleProps: SheetDragHandleProps = {
    // The grabber owns the touch, so the page behind never scrolls with it.
    style: { "touch-action": `none` },
    onPointerDown: (event) => {
      if (typeof window === `undefined` || typeof window.matchMedia !== `function` || !window.matchMedia(SHEET_MEDIA_QUERY).matches) {
        return
      }
      drag = { pointerId: event.pointerId, y: event.clientY, at: Date.now() }
      const target = event.currentTarget
      if (typeof target.setPointerCapture === `function`) {
        target.setPointerCapture(event.pointerId)
      }
    },
    onPointerMove: (event) => {
      const started = drag
      if (!started || started.pointerId !== event.pointerId) {
        return
      }
      // Upward drags resist (÷4) instead of lifting the sheet off its edge.
      const dy = event.clientY - started.y
      paint(`translateY(${dy > 0 ? dy : dy / 4}px)`, `none`)
    },
    onPointerUp: (event) => {
      const started = drag
      if (!started || started.pointerId !== event.pointerId) {
        return
      }
      drag = null
      const dy = event.clientY - started.y
      if (shouldDismissSheet(dy, Date.now() - started.at)) {
        paint(`translateY(100%)`, DISMISS_TRANSITION)
        window.setTimeout(() => {
          // Close while the panel is still off-screen (the exit keyframes
          // start from the CURRENT transform); release only if the close was
          // vetoed (the panel is still mounted and open).
          opts.onDismiss()
          window.requestAnimationFrame(() => {
            const panel = opts.panel()
            if (panel && panel.dataset.state === `open`) {
              release()
            }
          })
        }, DISMISS_ANIMATION_MS)
        return
      }
      paint(`translateY(0px)`, SNAP_BACK_TRANSITION)
      window.setTimeout(release, DISMISS_ANIMATION_MS)
    },
    onPointerCancel: (event) => {
      if (!drag || drag.pointerId !== event.pointerId) {
        return
      }
      drag = null
      release()
    },
  }

  return { handleProps }
}
