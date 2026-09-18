import type { CSSProperties, ReactNode } from "react"

import { statusColorClass } from "@/components/issue-properties/status-dropdown"
import { type IssueStatus } from "@/lib/domain"
import { IssueGroupBand, hexWithAlpha } from "@exp/ui"
import type { StatusRowOption } from "@/lib/team-statuses"

// EXP-862: ONE issue group band on the web. The board's big list and the
// sidebar's issue lists (the board list nav, My issues) draw the SAME header
// — status glyph, name, count, over the status tint, the whole strip folding
// its group — so a group reads identically wherever it is listed. Desktop
// `render_board_nav` / `render_my_issues_nav` take the big list's band the
// same way.
//
// EXP-961: the band itself is `IssueGroupBand` in @exp/ui; this file is the
// DATA half — it resolves a team status row into the band's glyph and wash,
// the way `components/issue-chip.tsx` does for `IssueChip`.

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
  return (
    <IssueGroupBand
      glyph={{
        icon: status.icon,
        colorClass: statusColorClass(status),
        colorHex: status.builtinKey ? undefined : status.colorHex,
      }}
      name={status.name}
      count={count}
      open={open}
      onToggle={onToggle}
      trailing={trailing}
      wash={tinted ? statusGroupWash(status) : undefined}
      density={density}
      className={className}
    />
  )
}
