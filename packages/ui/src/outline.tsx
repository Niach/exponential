import type { ReactNode } from "react"
import { PLAN_PAGE_LAYOUT } from "@exp/domain-contract/plan-page"
import { cn } from "./cn"

// #EXP-1493: THE Outline (styleguide `outline`, eUI `Outline`): an anchor
// list. A caption, one row per entry (the title on one line, a muted
// trailing count), ONE active row (the accent fill + a foreground bar on its
// leading edge), a press selects. Scroll-spy belongs to the host.

export interface OutlineEntry {
  id: string
  title: string
  /** A muted `· n` after the title (0/absent = none). */
  count?: number
}

export function Outline({
  caption,
  entries,
  active,
  onSelect,
  footer,
  className,
}: {
  caption?: string
  entries: readonly OutlineEntry[]
  active: string | null
  onSelect: (id: string) => void
  /** Rows under a separator (the plan page's Questions row). */
  footer?: ReactNode
  className?: string
}) {
  return (
    <nav data-testid="outline" className={cn(`flex min-w-0 flex-col gap-0.5`, className)}>
      {caption && (
        <div className="px-2 pb-2 text-[11px] font-medium tracking-wider text-muted-foreground uppercase">{caption}</div>
      )}
      {entries.map((entry) => {
        const on = entry.id === active
        return (
          <button
            key={entry.id}
            type="button"
            data-slot="outline-row"
            data-id={entry.id}
            data-active={on || undefined}
            onClick={() => onSelect(entry.id)}
            className={cn(
              `flex min-w-0 items-center gap-1 rounded-md px-2 text-left text-xs outline-none transition-colors focus-visible:ring-[3px] focus-visible:ring-ring/50`,
              on ? `bg-accent text-foreground` : `text-muted-foreground hover:bg-glass-row hover:text-foreground`
            )}
            style={{
              minHeight: PLAN_PAGE_LAYOUT.outlineRowHeight,
              boxShadow: on ? `inset ${PLAN_PAGE_LAYOUT.outlineActiveBar}px 0 0 var(--foreground)` : undefined,
            }}
          >
            <span className="min-w-0 truncate">{entry.title}</span>
            {entry.count ? <span className="shrink-0 text-muted-foreground">· {entry.count}</span> : null}
          </button>
        )
      })}
      {footer && (
        <>
          <div className="my-2 h-px bg-border" />
          {footer}
        </>
      )}
    </nav>
  )
}
