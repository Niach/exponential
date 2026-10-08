import * as React from "react"

// EXP-687 — drag-to-dismiss for mobile bottom sheets.
//
// Native sheets close when you flick them down; on mobile web the only exit
// used to be an ✕ or a tap outside. The gesture lives on the grabber (and the
// header area, which spreads the same props) rather than the whole panel so a
// scrolling list inside the sheet keeps its own touch handling.
//
// The `< sm` arm of ui/dialog.tsx and ui/sheet.tsx is the ONLY place this is
// active: the desktop panel is centered with a translate, which the drag
// transform would fight. The media query is the CSS `sm` breakpoint (640px),
// deliberately NOT `useIsMobile`'s 768 — this must match the arm that painted
// the sheet.
const SHEET_MEDIA_QUERY = `(max-width: 639px)`

/** Past this many px the sheet closes regardless of speed. */
const DISMISS_DISTANCE_PX = 80
/** A flick: px per ms, over the (much smaller) FLICK_DISTANCE_PX. */
const DISMISS_VELOCITY = 0.6
const FLICK_DISTANCE_PX = 24
/** Matches the panel's own slide-out duration. */
const DISMISS_ANIMATION_MS = 200

/**
 * Pure decision half of the gesture: did a downward drag of `dy` px over
 * `dtMs` ms earn a dismissal? Upward drags never do (the panel rubber-bands
 * back instead).
 */
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
  onPointerDown: (event: React.PointerEvent<HTMLElement>) => void
  onPointerMove: (event: React.PointerEvent<HTMLElement>) => void
  onPointerUp: (event: React.PointerEvent<HTMLElement>) => void
  onPointerCancel: (event: React.PointerEvent<HTMLElement>) => void
  style: React.CSSProperties
}

export function useSheetDrag({
  panelRef,
  onDismiss,
}: {
  panelRef: React.RefObject<HTMLElement | null>
  onDismiss: () => void
}): { handleProps: SheetDragHandleProps } {
  const drag = React.useRef<{ pointerId: number; y: number; at: number } | null>(
    null
  )

  const paint = React.useCallback(
    (transform: string, transition: string) => {
      const panel = panelRef.current
      if (!panel) {
        return
      }
      panel.style.transition = transition
      panel.style.transform = transform
    },
    [panelRef]
  )

  const release = React.useCallback(() => {
    const panel = panelRef.current
    if (!panel) {
      return
    }
    panel.style.transition = ``
    panel.style.transform = ``
  }, [panelRef])

  const handleProps = React.useMemo<SheetDragHandleProps>(
    () => ({
      // The grabber owns the touch, so the page behind never scrolls with it.
      style: { touchAction: `none` },
      onPointerDown: (event) => {
        if (
          typeof window === `undefined` ||
          typeof window.matchMedia !== `function` ||
          !window.matchMedia(SHEET_MEDIA_QUERY).matches
        ) {
          return
        }
        drag.current = {
          pointerId: event.pointerId,
          y: event.clientY,
          at: Date.now(),
        }
        const target = event.currentTarget
        if (typeof target.setPointerCapture === `function`) {
          target.setPointerCapture(event.pointerId)
        }
      },
      onPointerMove: (event) => {
        const started = drag.current
        if (!started || started.pointerId !== event.pointerId) {
          return
        }
        // Upward drags resist (÷4) instead of lifting the sheet off its edge.
        const dy = event.clientY - started.y
        paint(`translateY(${dy > 0 ? dy : dy / 4}px)`, `none`)
      },
      onPointerUp: (event) => {
        const started = drag.current
        if (!started || started.pointerId !== event.pointerId) {
          return
        }
        drag.current = null
        const dy = event.clientY - started.y
        if (shouldDismissSheet(dy, Date.now() - started.at)) {
          paint(`translateY(100%)`, `transform ${DISMISS_ANIMATION_MS}ms ease-in`)
          window.setTimeout(() => {
            // Close while the panel is still off-screen: Radix's exit
            // keyframes only carry a `to` frame, so they start from the
            // CURRENT transform. Releasing first would snap the sheet back to
            // y=0 and play the slide-out a second time. Release only if the
            // close was vetoed (the panel is still mounted and open).
            onDismiss()
            window.requestAnimationFrame(() => {
              const panel = panelRef.current
              if (panel && panel.dataset.state === `open`) {
                release()
              }
            })
          }, DISMISS_ANIMATION_MS)
          return
        }
        paint(`translateY(0px)`, `transform 200ms ease-out`)
        window.setTimeout(release, 200)
      },
      onPointerCancel: (event) => {
        if (!drag.current || drag.current.pointerId !== event.pointerId) {
          return
        }
        drag.current = null
        release()
      },
    }),
    [onDismiss, paint, panelRef, release]
  )

  return { handleProps }
}
