import type { CSSProperties, ReactNode } from "react"

import { conceptIcon } from "./icons.generated"
import { StatusGlyph, type StatusGlyphProps } from "./status-glyph"
import { cn } from "./cn"

// EXP-862 — ONE issue group band on the web, presentational half. The board's
// big list and the sidebar's issue lists (the board list nav, My issues) draw
// the SAME header — status glyph, name, count, over the status tint, the whole
// strip folding its group — so a group reads identically wherever it is
// listed. Desktop `render_board_nav` / `render_my_issues_nav` take the big
// list's band the same way.
//
// Two densities: `list` is the board page's sticky, edge-to-edge band,
// `compact` the 17rem sidebar's rounded strip.
//
// The status is passed IN as a resolved glyph and the tint as a resolved wash,
// so this file stays free of the team's status rows; the app's
// `components/issue-group-header.tsx` is the binding that resolves both.

const ChevronRightGlyph = conceptIcon(`ui-chevron-right`)

/** The band's fill — a Tailwind class (builtin rows) or an inline wash off a
 *  custom row's own hex. Omitted entirely = a phone's plain header (EXP-620). */
export interface IssueGroupWash {
  className?: string
  style?: CSSProperties
}

export function IssueGroupBand({
  glyph,
  name,
  count,
  open,
  onToggle,
  trailing,
  wash,
  density = `list`,
  className,
}: {
  /** The group's status glyph, already resolved against the team's rows. */
  glyph: StatusGlyphProps
  name: string
  /** The group's full size — NOT the rendered row count (REV-46 windows the
   *  rows behind a "Show more"). */
  count: number
  open: boolean
  onToggle: () => void
  /** The group's own action — the board list's hover "+" (EXP-862: ghost, it
   *  is a secondary control). Rendered outside the fold button: a button
   *  inside a button is invalid. */
  trailing?: ReactNode
  /** The status tint. Omitted = un-tinted (EXP-620: the natives draw no tint
   *  on a phone, so neither does the web). */
  wash?: IssueGroupWash
  density?: `list` | `compact`
  className?: string
}) {
  const compact = density === `compact`
  return (
    <div
      data-slot="issue-group-header"
      className={cn(
        `group flex items-center justify-between`,
        compact
          ? `mb-1 rounded-md px-2 py-1`
          : // md+: backdrop-blur is load-bearing — the tint is translucent and
            // rows scroll under the sticky band. Below md there is no band and
            // no pinning (EXP-620), so the content just sits 24px in (the
            // 16px gutter plus 8px).
            `max-md:px-2 max-md:py-2 md:sticky md:top-0 md:z-10 md:border-b md:border-border/40 md:py-1.5 md:pl-3 md:pr-6 md:backdrop-blur-md`,
        wash?.className,
        className
      )}
      style={wash?.style}
    >
      <button
        type="button"
        aria-expanded={open}
        onClick={onToggle}
        className="flex min-w-0 flex-1 cursor-pointer items-center gap-1.5 rounded-md text-left outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
      >
        <ChevronRightGlyph
          className={cn(
            `size-3 shrink-0 text-muted-foreground transition-transform duration-fast ease-standard motion-reduce:transition-none`,
            open && `rotate-90`
          )}
        />
        <StatusGlyph {...glyph} className="size-3.5 shrink-0" />
        <span className="min-w-0 truncate text-sm font-medium">{name}</span>
        <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
          {count}
        </span>
      </button>
      {trailing}
    </div>
  )
}
