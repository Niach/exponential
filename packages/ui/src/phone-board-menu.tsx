import * as React from "react"
import { CHATS_COPY, CHATS_SHELL, CHATS_TEST_IDS } from "@exp/domain-contract/chats"

import { cn } from "./cn"
import { FAB_CHROME_CLASS } from "./fab-chrome"
import { conceptIcon } from "./icons.generated"
import { usePrefersReducedMotion } from "./phone-tab-bar"

// #EXP-1384 (fixture `chats.json` `shell.boardMenu`, Linear's iOS "more"
// menu): on the board view the Board tab's tap MORPHS THE WHOLE bottom strip
// (the tab capsule and the new chat / new issue capsule beside it) into ONE
// glass context menu panel. The panel grows from the strip's frame (same
// left, right and bottom edges; the corner radius from the strip's to
// `radius`) over `motion.open` / `motion.openEase` while the strip's content
// fades out and the rows fade in; it collapses back at `motion.close` /
// `motion.closeEase`. Reduce motion = a plain cross-fade. Rows: the active
// team's boards (no team name, no header), `rowHeight` each, the active one
// on the glass-active pill with `ui-check`, then the muted New board row for
// a viewer who may create boards; past `maxVisibleRows` the rows scroll
// inside the panel. A tap outside (a transparent scrim) or Escape collapses.
//
// `PhoneBoardMenuStrip` wraps the strip (its children ARE the strip) so the
// panel shares its frame; web-mobile mounts it in mobile-tab-bar.tsx.

const MENU = CHATS_SHELL.boardMenu
const BAR = CHATS_SHELL.bar
const CheckIcon = conceptIcon(`ui-check`)
const AddIcon = conceptIcon(`ui-add`)

const DURATION: Record<string, string> = {
  fast: `var(--motion-duration-fast)`,
  standard: `var(--motion-duration-standard)`,
  slow: `var(--motion-duration-slow)`,
}
const EASE: Record<string, string> = {
  standard: `var(--motion-ease-standard)`,
  decelerate: `var(--motion-ease-decelerate)`,
  accelerate: `var(--motion-ease-accelerate)`,
}
const CLOSE_MS: Record<string, number> = { fast: 120, standard: 180, slow: 280 }

export interface PhoneBoardMenuRow {
  id: string
  name: string
  /** The board's glyph (BoardGlyph). */
  glyph: React.ReactNode
  active?: boolean
}

/** The panel's open height: the rows (New board included) up to
 *  `maxVisibleRows`, plus the strip's padding above and below. */
export function phoneBoardMenuHeight(rowCount: number): number {
  return Math.min(rowCount, MENU.maxVisibleRows) * MENU.rowHeight + BAR.padding * 2
}

const ROW_CLASS = `flex w-full shrink-0 items-center gap-3 rounded-full px-4 text-left text-[15px] outline-none transition-colors duration-fast active:bg-glass-active/60 focus-visible:bg-glass-active/60`

export function PhoneBoardMenuStrip({
  open,
  onOpenChange,
  rows,
  onSelect,
  onNewBoard,
  children,
  className,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  rows: PhoneBoardMenuRow[]
  onSelect: (id: string) => void
  /** Present = the viewer may create boards (the muted New board row). */
  onNewBoard?: () => void
  /** The strip itself: the tab capsule and the capsule beside it. */
  children: React.ReactNode
  className?: string
}) {
  const reduced = usePrefersReducedMotion()
  // `mounted` keeps the panel through its collapse; `expanded` drives the
  // morph (false = the strip's frame, true = the menu's).
  const [mounted, setMounted] = React.useState(open)
  const [expanded, setExpanded] = React.useState(open)
  const activeRef = React.useRef<HTMLButtonElement | null>(null)

  React.useEffect(() => {
    if (open) {
      setMounted(true)
      // Two frames: the panel paints at the strip's frame first, then grows.
      let second = 0
      const first = requestAnimationFrame(() => {
        second = requestAnimationFrame(() => setExpanded(true))
      })
      return () => {
        cancelAnimationFrame(first)
        cancelAnimationFrame(second)
      }
    }
    setExpanded(false)
    const timer = setTimeout(() => setMounted(false), CLOSE_MS[MENU.motion.close] ?? 180)
    return () => clearTimeout(timer)
  }, [open])

  React.useEffect(() => {
    if (!open) return
    const onKey = (event: KeyboardEvent) => {
      if (event.key === `Escape`) {
        event.preventDefault()
        onOpenChange(false)
      }
    }
    window.addEventListener(`keydown`, onKey)
    return () => window.removeEventListener(`keydown`, onKey)
  }, [open, onOpenChange])

  React.useEffect(() => {
    if (expanded) activeRef.current?.focus({ preventScroll: true })
  }, [expanded])

  const rowCount = rows.length + (onNewBoard ? 1 : 0)
  const openHeight = phoneBoardMenuHeight(rowCount)
  const stripHeight = BAR.height
  const duration = expanded ? DURATION[MENU.motion.open] : DURATION[MENU.motion.close]
  const ease = expanded ? EASE[MENU.motion.openEase] : EASE[MENU.motion.closeEase]
  const transition = reduced
    ? `opacity ${duration} ${ease}`
    : `height ${duration} ${ease}, border-radius ${duration} ${ease}, opacity ${duration} ${ease}`

  return (
    <div className={cn(`relative flex w-full min-w-0 items-center`, className)}>
      {mounted && (
        // The transparent scrim: a tap outside the panel collapses it.
        <div
          aria-hidden
          data-slot="phone-board-menu-scrim"
          className="pointer-events-auto fixed inset-0"
          onPointerDown={(event) => {
            event.preventDefault()
            onOpenChange(false)
          }}
        />
      )}
      <div
        className="flex w-full min-w-0 items-center"
        style={{
          gap: BAR.gap,
          opacity: expanded ? 0 : 1,
          transition: `opacity ${duration} ${ease}`,
        }}
        aria-hidden={expanded || undefined}
        inert={expanded || undefined}
      >
        {children}
      </div>
      {mounted && (
        <div
          role="menu"
          data-testid={CHATS_TEST_IDS.boardMenu}
          data-state={expanded ? `open` : `closed`}
          className={cn(
            `pointer-events-auto absolute inset-x-0 bottom-0 z-10 overflow-hidden text-foreground`,
            FAB_CHROME_CLASS,
            // The glass MENU fill (menu-surface), so the page under a tall
            // panel never reads through its rows.
            `bg-glass-card-opaque`
          )}
          style={{
            height: reduced || expanded ? openHeight : stripHeight,
            borderRadius: reduced || expanded ? MENU.radius : stripHeight / 2,
            opacity: reduced ? (expanded ? 1 : 0) : 1,
            transition,
          }}
        >
          <div
            className="flex h-full flex-col overflow-y-auto"
            style={{
              padding: BAR.padding,
              opacity: expanded ? 1 : 0,
              transition: `opacity ${duration} ${ease}`,
            }}
          >
            {rows.map((row) => (
              <button
                key={row.id}
                ref={row.active ? activeRef : undefined}
                type="button"
                role="menuitemradio"
                aria-checked={row.active ?? false}
                data-testid={CHATS_TEST_IDS.boardMenuRow}
                className={cn(ROW_CLASS, row.active && `bg-glass-active`)}
                style={{ height: MENU.rowHeight }}
                onClick={() => {
                  onSelect(row.id)
                  onOpenChange(false)
                }}
              >
                <span className="flex size-5 shrink-0 items-center justify-center">{row.glyph}</span>
                <span className="min-w-0 flex-1 truncate">{row.name}</span>
                {row.active && <CheckIcon className="size-4 shrink-0" />}
              </button>
            ))}
            {onNewBoard && (
              <button
                type="button"
                role="menuitem"
                data-testid={CHATS_TEST_IDS.boardMenuRow}
                data-row="new-board"
                className={cn(ROW_CLASS, `text-muted-foreground`)}
                style={{ height: MENU.rowHeight }}
                onClick={() => {
                  onOpenChange(false)
                  onNewBoard()
                }}
              >
                <span className="flex size-5 shrink-0 items-center justify-center">
                  <AddIcon className="size-4" />
                </span>
                <span className="min-w-0 flex-1 truncate">{CHATS_COPY.boardMenu.newBoard}</span>
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
