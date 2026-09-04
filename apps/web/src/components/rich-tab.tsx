import type { ReactNode } from "react"
import { conceptIcon } from "@/lib/icons.generated"
import { cn } from "@/lib/utils"

// EXP-698 — the ONE rich tab on the web: a status dot, a mono identifier, a
// truncating title, an optional trailing badge and an optional close glyph,
// all inside ONE hover target. The wrapper IS the click target (the close
// button only stops propagation), so hovering anywhere — the X included —
// lights the whole tab; the old dock tab split that across two sibling
// buttons and the X hovered dead.
//
// The agent dock's session strip is its only mount today; anything tab-shaped
// on the web belongs here rather than in a second hand-rolled row.

const CloseIcon = conceptIcon(`ui-close`)

interface RichTabProps {
  /** The tab the panel is showing: filled chrome, full-strength label. */
  active?: boolean
  /** The host machine is offline — the run is parked, not gone (EXP-550). */
  paused?: boolean
  /** A `bg-*` class for the 6px dot, or a ready-made glyph (a live ping). */
  status?: string | ReactNode
  /** Mono prefix — an issue identifier, a run name. */
  identifier?: string | null
  title: string
  /** Trailing muted detail (the device a run rides on). */
  badge?: ReactNode
  /** The hover title of the whole tab. */
  tooltip?: string
  onSelect: () => void
  /** Omit to render no close glyph at all. */
  onClose?: () => void
  closeLabel?: string
  /** Middle-click, the browser-tab habit for "close this". */
  onMiddleClick?: () => void
}

export function RichTab({
  active = false,
  paused = false,
  status,
  identifier,
  title,
  badge,
  tooltip,
  onSelect,
  onClose,
  closeLabel = `Close tab`,
  onMiddleClick,
}: RichTabProps) {
  return (
    <div
      role="tab"
      tabIndex={0}
      aria-selected={active}
      title={tooltip}
      onClick={onSelect}
      onKeyDown={(event) => {
        // A keydown bubbling from the nested close button is its own click.
        if (event.defaultPrevented || event.target !== event.currentTarget) return
        if (event.key === `Enter` || event.key === ` `) {
          event.preventDefault()
          onSelect()
        }
      }}
      onAuxClick={(event) => {
        if (event.button !== 1 || !onMiddleClick) return
        event.preventDefault()
        onMiddleClick()
      }}
      className={cn(
        `group flex h-[26px] shrink-0 cursor-pointer select-none items-center gap-1.5 rounded-[10px] px-2.5 text-sm transition-colors duration-fast`,
        active
          ? `bg-glass-active text-foreground`
          : `text-muted-foreground hover:bg-glass-row`,
        paused && `opacity-60`
      )}
    >
      {typeof status === `string` ? (
        <span className={cn(`size-1.5 shrink-0 rounded-full`, status)} />
      ) : (
        status
      )}
      {identifier && (
        <span className="shrink-0 font-mono text-xs text-foreground/50">
          {identifier}
        </span>
      )}
      <span className="max-w-[180px] truncate">{title}</span>
      {badge}
      {onClose && (
        <button
          type="button"
          aria-label={closeLabel}
          title={closeLabel}
          className="flex size-5 shrink-0 items-center justify-center rounded-full text-muted-foreground transition-colors duration-fast hover:text-foreground"
          onClick={(event) => {
            // The tab toggles the panel; the X must not.
            event.stopPropagation()
            onClose()
          }}
        >
          <CloseIcon className="size-3" />
        </button>
      )}
    </div>
  )
}
