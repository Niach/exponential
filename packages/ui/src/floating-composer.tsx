import { useEffect, useRef, useState, type ReactNode } from "react"
import { cn } from "./cn"
import { MobileWorkBar, MobileWorkCapsule } from "./mobile-work-bar"

// EXP-1316: THE phone reply box ×3 (iOS `FloatingComposer`, Android
// `FloatingComposer.kt`): the thread conversation AND the session view on a
// phone. Collapsed = the floating bar's capsule reading the placeholder
// (`MobileWorkBar` / `MobileWorkCapsule`, over the content above the bottom
// edge fade, never pushing it); a tap expands it into THE Composer card the
// host hands in (text, + images/files, send, Stop while busy); focus leaving
// the card collapses it. The card stays MOUNTED while collapsed, so a draft
// survives (the capsule then reads the draft instead of the placeholder).
// Web md+ keeps the in-flow Composer box.

/** The scroll clearance a conversation reserves under the capsule. */
export { MOBILE_WORK_BAR_CLEARANCE as FLOATING_COMPOSER_CLEARANCE } from "./mobile-work-bar"

export function FloatingComposer({
  placeholder,
  children,
  defaultExpanded = false,
  hidden = false,
  testId = `floating-composer`,
}: {
  /** The capsule's words while collapsed (`Reply…`). */
  placeholder: string
  /** THE Composer card (the app's `SessionComposer`). */
  children: ReactNode
  defaultExpanded?: boolean
  /** Another bottom-edge owner is up (a card took the composer's place). */
  hidden?: boolean
  testId?: string
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

  return (
    <MobileWorkBar
      hidden={hidden}
      expanded={
        <div data-testid={testId} data-expanded={expanded}>
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
              const next = event.relatedTarget as Node | null
              if (next && event.currentTarget.contains(next)) return
              collapse()
            }}
            onKeyDown={(event) => {
              if (event.key === `Escape`) collapse()
            }}
          >
            {children}
          </div>
        </div>
      }
    />
  )
}
