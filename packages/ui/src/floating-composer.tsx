import { useEffect, useRef, useState, type ReactNode } from "react"
import { cn } from "./cn"
import { MobileWorkBar, MobileWorkCapsule } from "./mobile-work-bar"

// EXP-1316: THE phone reply box ×3 (iOS `FloatingComposer`, Android
// `FloatingComposer.kt`): the thread conversation AND the session view on a
// phone. Collapsed = the floating bar's capsule reading the placeholder
// (`MobileWorkBar` / `MobileWorkCapsule`, over the content above the bottom
// edge fade, never pushing it); a tap expands it into THE Composer card the
// host hands in (text, + images/files, send, Stop while busy); a press
// outside the card (or Tab / Escape leaving it) collapses it. The card stays MOUNTED while collapsed, so a draft
// survives (the capsule then reads the draft instead of the placeholder).
// Web md+ keeps the in-flow Composer box.

/** The scroll clearance a conversation reserves under the capsule. */
export { MOBILE_WORK_BAR_CLEARANCE as FLOATING_COMPOSER_CLEARANCE } from "./mobile-work-bar"

/** A popover or sheet the card opened, portalled out of it. */
const POPUP_SELECTOR = `[data-radix-popper-content-wrapper], [role="dialog"]`

export function FloatingComposer({
  placeholder,
  children,
  defaultExpanded = false,
  hidden = false,
  testId = `floating-composer`,
  above,
}: {
  /** The capsule's words while collapsed (`Reply…`). */
  placeholder: string
  /** THE Composer card (the app's `SessionComposer`). */
  children: ReactNode
  defaultExpanded?: boolean
  /** Another bottom-edge owner is up (a card took the composer's place). */
  hidden?: boolean
  testId?: string
  /** EXP-1211: shown over the capsule AND the card (the quick replies). */
  above?: ReactNode
}) {
  const [expanded, setExpanded] = useState(defaultExpanded)
  const [draft, setDraft] = useState(``)
  const cardRef = useRef<HTMLDivElement | null>(null)

  // Expanding hands the field the focus (the keyboard comes up with it).
  useEffect(() => {
    if (!expanded) return
    cardRef.current?.querySelector<HTMLElement>(`textarea, [contenteditable="true"]`)?.focus()
  }, [expanded])

  const collapse = () => {
    const field = cardRef.current?.querySelector<HTMLTextAreaElement>(`textarea`)
    setDraft(field?.value.trim() ?? ``)
    setExpanded(false)
  }
  const collapseRef = useRef(collapse)
  collapseRef.current = collapse

  // A press OUTSIDE the card collapses it — from a document `pointerdown`,
  // not the field's blur: Safari (iOS and macOS) does not focus buttons on
  // tap, so a tap on Send or the file picker blurred the textarea with
  // relatedTarget null and the card went display:none between pointerdown
  // and click, swallowing the click. A press inside the card, or in a popover
  // / dialog it opened (portalled out of it), keeps it open.
  useEffect(() => {
    if (!expanded) return
    const onPointerDown = (event: Event) => {
      const card = cardRef.current
      const target = event.target
      if (!card || !(target instanceof Node)) return
      if (card.contains(target)) return
      if (
        target instanceof Element &&
        target.closest(POPUP_SELECTOR)
      ) {
        return
      }
      collapseRef.current()
    }
    document.addEventListener(`pointerdown`, onPointerDown, true)
    return () => document.removeEventListener(`pointerdown`, onPointerDown, true)
  }, [expanded])

  return (
    <MobileWorkBar
      hidden={hidden}
      expanded={
        <div data-testid={testId} data-expanded={expanded}>
          {above && <div className="pb-2 empty:hidden">{above}</div>}
          {!expanded && (
            <MobileWorkCapsule
              className={cn(`w-full`, draft && `text-foreground`)}
              data-testid={`${testId}-capsule`}
              onClick={() => setExpanded(true)}
            >
              <span className="min-w-0 truncate">{draft || placeholder}</span>
            </MobileWorkCapsule>
          )}
          <div
            ref={cardRef}
            className={cn(!expanded && `hidden`)}
            onBlur={(event) => {
              // Keyboard dismissal only (Tab leaving the card): a blur with
              // no relatedTarget is a Safari tap or the window losing focus,
              // and the pointerdown listener above owns taps.
              const next = event.relatedTarget as Node | null
              if (!next || event.currentTarget.contains(next)) return
              // The card's own popups (the model pin's sheet, the ring's
              // usage sheet) portal out of it: focus moving there stays in.
              if (next instanceof Element && next.closest(POPUP_SELECTOR)) return
              collapse()
            }}
            onKeyDown={(event) => {
              // An Escape the field's own menu (`/`, `@`) consumed is not
              // the reader leaving the card.
              if (event.key === `Escape` && !event.defaultPrevented) collapse()
            }}
          >
            {children}
          </div>
        </div>
      }
    />
  )
}
