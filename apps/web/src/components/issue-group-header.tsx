import type { CSSProperties, ReactNode } from "react"

import {
  statusColorClass,
  statusColorStyle,
} from "@/components/issue-properties/status-dropdown"
import { type IssueStatus } from "@/lib/domain"
import { conceptIcon, ICON_COMPONENTS } from "@/lib/icons.generated"
import { hexWithAlpha } from "@/lib/status-icons"
import type { StatusRowOption } from "@/lib/team-statuses"
import { cn } from "@/lib/utils"

// EXP-862: ONE issue group band on the web. The board's big list and the
// sidebar's issue lists (the board list nav, My issues) draw the SAME header
// — status glyph, name, count, over the status tint, the whole strip folding
// its group — so a group reads identically wherever it is listed. Desktop
// `render_board_nav` / `render_my_issues_nav` take the big list's band the
// same way.
//
// Two densities: `list` is the board page's sticky, edge-to-edge band,
// `compact` the 17rem sidebar's rounded strip.

const ChevronRightGlyph = conceptIcon(`ui-chevron-right`)

// Status-tinted washes — the Tailwind palette colors the old rgba literals
// encoded (zinc-500/zinc-300/yellow-500/green-500/blue-500), matching the
// status icon hues in lib/domain.ts. EXP-314: BUILTIN rows keep these exact
// classes (keyed on the builtin key, so the default team's headers are
// byte-identical to before); CUSTOM rows get a 10%-alpha inline wash from
// their own hex.
const statusHeaderBg: Record<IssueStatus, string> = {
  backlog: `bg-zinc-500/10`,
  in_progress: `bg-yellow-500/10`,
  in_review: `bg-green-500/10`,
  done: `bg-blue-500/10`,
  cancelled: `bg-zinc-500/10`,
  duplicate: `bg-zinc-500/10`,
}

type Wash = { className: string; style?: CSSProperties }

/** The band's fill for a status row — its builtin class, or a 10%-alpha wash
 *  off a custom row's own hex. */
export function statusGroupWash(status: StatusRowOption): Wash {
  if (status.builtinKey) {
    return { className: statusHeaderBg[status.builtinKey] ?? `bg-zinc-500/10` }
  }
  return {
    className: ``,
    style: { backgroundColor: hexWithAlpha(status.colorHex, 0.1) },
  }
}

/** Mobile's un-tinted header (EXP-620). */
const NO_WASH: Wash = { className: `` }

export function IssueGroupHeader({
  status,
  count,
  open,
  onToggle,
  trailing,
  tinted = true,
  density = `list`,
  className,
}: {
  status: StatusRowOption
  /** The group's full size — NOT the rendered row count (REV-46 windows the
   *  rows behind a "Show more"). */
  count: number
  open: boolean
  onToggle: () => void
  /** The group's own action — the board list's hover "+" (EXP-862: ghost, it
   *  is a secondary control). Rendered outside the fold button: a button
   *  inside a button is invalid. */
  trailing?: ReactNode
  /** EXP-620: the natives draw no tint on a phone, so neither does the web. */
  tinted?: boolean
  density?: `list` | `compact`
  className?: string
}) {
  const Icon = ICON_COMPONENTS[status.icon]
  const wash = tinted ? statusGroupWash(status) : NO_WASH
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
        wash.className,
        className
      )}
      style={wash.style}
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
        {Icon && (
          <Icon
            className={cn(`size-3.5 shrink-0`, statusColorClass(status))}
            style={statusColorStyle(status)}
          />
        )}
        <span className="min-w-0 truncate text-sm font-medium">
          {status.name}
        </span>
        <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
          {count}
        </span>
      </button>
      {trailing}
    </div>
  )
}
