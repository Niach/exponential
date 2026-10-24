import * as React from "react"
import { Slot } from "radix-ui"
import { CHATS_SHELL } from "@exp/domain-contract/chats"

import { cn } from "./cn"
import { FAB_CHROME_CLASS } from "./fab-chrome"

// Thread 58cb32df (#EXP-1373, fixture `chats.json` `shell.tabs`): the PHONE
// bottom bar's tab capsule — on Android and web-mobile the look-alike of the
// iOS Liquid Glass tab bar, painted on THE floating chrome recipe
// (`FAB_CHROME_CLASS`): each tab an icon over its label, the selected one on
// the glass-active pill, an optional dot (Chats = unread). 52px tall, the
// height of the two-armed new chat / new issue capsule beside it. iOS = the
// system tab bar (ExpUI `PhoneTabBar` look), Android `ui/components`
// `PhoneTabBar`.
//
// #EXP-1384 (`shell.bar`): a tab is as wide as its label (label + 2 ×
// `tabPaddingX`, at least `tabMinWidth`, the label truncating past
// `labelMaxWidth`), the capsule the sum of its tabs, shrinking (the widest
// label first) only when the screen cannot hold it. DRAG (`shell.bar.drag`,
// the native iOS tab bar's behaviour): a press lifts the selection LENS
// (the glass-active pill scaled to `lensScale`) under the finger; dragging
// moves it along the bar (clamped, no animation), the tab under its centre
// is the CANDIDATE; lift selects it (the same as tapping it: a programmatic
// click on the tab, so a Link navigates and the Board tab's caret rule
// holds); a lift farther than `cancelDistance` above or below the bar
// cancels. A plain tap is the tab's own native click.

const BAR = CHATS_SHELL.bar
/** Movement (px) that turns a press into a drag; below it the tab's native
 *  click is the selection. */
const DRAG_SLOP = 6

export const PHONE_TAB_BAR_CLASS = `group/phone-bar pointer-events-auto relative flex h-[52px] min-w-0 touch-none select-none items-center rounded-full p-1 ${FAB_CHROME_CLASS}`

/** `prefers-reduced-motion: reduce`, live. */
export function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = React.useState(false)
  React.useEffect(() => {
    if (typeof window === `undefined` || !window.matchMedia) return
    const query = window.matchMedia(`(prefers-reduced-motion: reduce)`)
    const sync = () => setReduced(query.matches)
    sync()
    query.addEventListener?.(`change`, sync)
    return () => query.removeEventListener?.(`change`, sync)
  }, [])
  return reduced
}

interface Lens {
  left: number
  width: number
  scale: number
  /** Settling at `standard` / `decelerate` (lift, cancel); else it moves
   *  with the finger. */
  settling: boolean
}

interface Gesture {
  pointerId: number
  startX: number
  startY: number
  dragging: boolean
}

function tabsOf(nav: HTMLElement): HTMLElement[] {
  return [...nav.querySelectorAll<HTMLElement>(`[data-slot=phone-tab]`)]
}

export function PhoneTabBar({
  className,
  onPointerDown,
  onClickCapture,
  children,
  ...props
}: React.ComponentProps<`nav`>) {
  const navRef = React.useRef<HTMLElement | null>(null)
  const gesture = React.useRef<Gesture | null>(null)
  const suppressClickUntil = React.useRef(0)
  const settleTimer = React.useRef<ReturnType<typeof setTimeout> | null>(null)
  const [lens, setLens] = React.useState<Lens | null>(null)
  const reduced = usePrefersReducedMotion()
  const scale = reduced ? 1 : BAR.drag.lensScale

  const markCandidate = React.useCallback((candidate: HTMLElement | null) => {
    const nav = navRef.current
    if (!nav) return
    for (const tab of tabsOf(nav)) {
      if (tab === candidate) tab.dataset.candidate = `true`
      else delete tab.dataset.candidate
    }
  }, [])

  const endGesture = React.useCallback(() => {
    const nav = navRef.current
    gesture.current = null
    if (nav) delete nav.dataset.dragging
    markCandidate(null)
  }, [markCandidate])

  /** The lens over `tab` (nav-local), settling there, then gone. */
  const settleOn = React.useCallback(
    (tab: HTMLElement | null) => {
      const nav = navRef.current
      if (!nav || !tab) {
        setLens(null)
        return
      }
      const navRect = nav.getBoundingClientRect()
      const rect = tab.getBoundingClientRect()
      setLens({ left: rect.left - navRect.left, width: rect.width, scale: 1, settling: true })
      if (settleTimer.current) clearTimeout(settleTimer.current)
      settleTimer.current = setTimeout(() => setLens(null), 200)
    },
    []
  )

  React.useEffect(
    () => () => {
      if (settleTimer.current) clearTimeout(settleTimer.current)
    },
    []
  )

  const handlePointerDown = (event: React.PointerEvent<HTMLElement>) => {
    onPointerDown?.(event)
    if (event.defaultPrevented || event.button !== 0) return
    const nav = navRef.current
    if (!nav) return
    const pressed = (event.target as HTMLElement).closest<HTMLElement>(`[data-slot=phone-tab]`)
    if (!pressed || !nav.contains(pressed)) return
    const navRect = nav.getBoundingClientRect()
    const rect = pressed.getBoundingClientRect()
    if (settleTimer.current) clearTimeout(settleTimer.current)
    gesture.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      dragging: false,
    }
    setLens({ left: rect.left - navRect.left, width: rect.width, scale, settling: false })
    // The press lifts the selection: the pill leaves the selected tab, the
    // pressed one is the candidate.
    nav.dataset.dragging = `true`
    markCandidate(pressed)

    const pointerId = event.pointerId
    const candidateAt = (clientX: number): { tab: HTMLElement | null; left: number; width: number } => {
      const box = nav.getBoundingClientRect()
      const tabs = tabsOf(nav)
      const pad = BAR.padding
      const x = Math.min(Math.max(clientX, box.left + pad), box.right - pad)
      let tab: HTMLElement | null = null
      for (const each of tabs) {
        const r = each.getBoundingClientRect()
        if (x >= r.left && x <= r.right) tab = each
      }
      if (!tab) {
        // Between tabs (or on the padding): the nearest one.
        let best = Infinity
        for (const each of tabs) {
          const r = each.getBoundingClientRect()
          const d = Math.min(Math.abs(x - r.left), Math.abs(x - r.right))
          if (d < best) {
            best = d
            tab = each
          }
        }
      }
      const width = tab ? tab.getBoundingClientRect().width : rect.width
      const left = Math.min(Math.max(x - box.left - width / 2, pad), box.width - pad - width)
      return { tab, left, width }
    }

    const onMove = (move: PointerEvent) => {
      const current = gesture.current
      if (!current || move.pointerId !== pointerId) return
      if (!current.dragging) {
        if (Math.hypot(move.clientX - current.startX, move.clientY - current.startY) < DRAG_SLOP) return
        current.dragging = true
      }
      const next = candidateAt(move.clientX)
      markCandidate(next.tab)
      setLens({ left: next.left, width: next.width, scale, settling: false })
    }

    const detach = () => {
      window.removeEventListener(`pointermove`, onMove)
      window.removeEventListener(`pointerup`, onUp)
      window.removeEventListener(`pointercancel`, onCancel)
    }

    const selected = () => tabsOf(nav).find((tab) => tab.dataset.selected === `true`) ?? null

    const onUp = (up: PointerEvent) => {
      const current = gesture.current
      if (!current || up.pointerId !== pointerId) return
      detach()
      if (!current.dragging) {
        // A plain tap: the tab's own native click selects it.
        endGesture()
        settleOn(pressed)
        return
      }
      const box = nav.getBoundingClientRect()
      const off =
        up.clientY < box.top ? box.top - up.clientY : up.clientY > box.bottom ? up.clientY - box.bottom : 0
      const { tab } = candidateAt(up.clientX)
      endGesture()
      if (off > BAR.drag.cancelDistance || !tab) {
        settleOn(selected())
      } else {
        settleOn(tab)
        // Lift = the same as tapping it; then eat the browser's own click
        // that follows a drag (it would land on the pressed tab).
        tab.click()
      }
      suppressClickUntil.current = Date.now() + 400
    }

    const onCancel = (cancel: PointerEvent) => {
      if (cancel.pointerId !== pointerId) return
      detach()
      endGesture()
      settleOn(selected())
    }

    window.addEventListener(`pointermove`, onMove)
    window.addEventListener(`pointerup`, onUp)
    window.addEventListener(`pointercancel`, onCancel)
  }

  const handleClickCapture = (event: React.MouseEvent<HTMLElement>) => {
    if (Date.now() < suppressClickUntil.current) {
      suppressClickUntil.current = 0
      event.preventDefault()
      event.stopPropagation()
      return
    }
    onClickCapture?.(event)
  }

  return (
    <nav
      ref={navRef}
      aria-label="Primary"
      data-slot="phone-tab-bar"
      className={cn(PHONE_TAB_BAR_CLASS, className)}
      onPointerDown={handlePointerDown}
      onClickCapture={handleClickCapture}
      {...props}
    >
      {lens && (
        <span
          aria-hidden
          data-slot="phone-tab-lens"
          className={cn(
            `pointer-events-none absolute top-1 h-11 rounded-full bg-glass-active`,
            lens.settling &&
              `transition-[left,width,transform] duration-standard ease-decelerate`
          )}
          style={{ left: lens.left, width: lens.width, transform: `scale(${lens.scale})` }}
        />
      )}
      {children}
    </nav>
  )
}

/** The dot a tab wears (top-right of its icon). */
export function PhoneTabDot({ className }: { className?: string }) {
  return (
    <span
      aria-hidden
      data-slot="phone-tab-dot"
      className={cn(
        `pointer-events-none absolute left-[calc(50%+5px)] top-1 size-2 rounded-full ring-2 ring-popover`,
        className
      )}
    />
  )
}

export function PhoneTab({
  label,
  icon,
  selected = false,
  dot,
  trailing,
  asChild = false,
  className,
  style,
  children,
  ...props
}: Omit<React.ComponentProps<`button`>, `children`> & {
  label: string
  /** The glyph (a concept icon at size-5, or the team's avatar). */
  icon: React.ReactNode
  selected?: boolean
  /** The dot's colour class (`bg-yellow-400`); absent = none. */
  dot?: string | null
  /** After the label (the Board tab's caret on the board view). */
  trailing?: React.ReactNode
  /** Render as the single child (a `Link`/`<a>`); its own children are
   *  replaced by the tab's body. */
  asChild?: boolean
  children?: React.ReactElement
}) {
  const body = (
    <>
      <span className="flex h-5 items-center justify-center">{icon}</span>
      <span className="flex min-w-0 max-w-full items-center gap-0.5">
        <span
          className="min-w-0 truncate text-[10px] font-medium leading-none"
          style={{ maxWidth: BAR.labelMaxWidth }}
        >
          {label}
        </span>
        {trailing}
      </span>
      {dot && <PhoneTabDot className={dot} />}
    </>
  )
  const tabClassName = cn(
    `relative z-[1] flex h-11 min-w-0 shrink flex-col items-center justify-center gap-1 rounded-full outline-none transition-colors duration-fast focus-visible:ring-[3px] focus-visible:ring-ring/50`,
    selected
      ? `bg-glass-active text-foreground group-data-[dragging=true]/phone-bar:bg-transparent group-data-[dragging=true]/phone-bar:text-muted-foreground`
      : `text-muted-foreground`,
    `group-data-[dragging=true]/phone-bar:data-[candidate=true]:text-foreground`,
    className
  )
  const tabStyle: React.CSSProperties = {
    minWidth: BAR.tabMinWidth,
    paddingInline: BAR.tabPaddingX,
    ...style,
  }
  if (asChild && React.isValidElement<{ children?: React.ReactNode }>(children)) {
    return (
      <Slot.Root
        data-slot="phone-tab"
        data-selected={selected}
        aria-label={label}
        aria-current={selected ? `page` : undefined}
        className={tabClassName}
        style={tabStyle}
        draggable={false}
        {...(props as Record<string, unknown>)}
      >
        {React.cloneElement(children, undefined, body)}
      </Slot.Root>
    )
  }
  return (
    <button
      type="button"
      data-slot="phone-tab"
      data-selected={selected}
      aria-label={label}
      aria-current={selected ? `page` : undefined}
      className={tabClassName}
      style={tabStyle}
      {...props}
    >
      {body}
    </button>
  )
}
