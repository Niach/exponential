import type { CSSProperties, ReactNode } from "react"

import { statusColorClass } from "@/components/issue-properties/status-dropdown"
import {
  BUILTIN_STATUS_WASH_CLASS,
  IssueGroupBand,
  hexWithAlpha,
} from "@exp/ui"
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

// Status-tinted washes: BUILTIN rows keep the package's exact classes (keyed
// on the builtin key, so the default team's headers are byte-identical to
// before, EXP-314); CUSTOM rows get a 10%-alpha inline wash from their own hex.
type Wash = { className: string; style?: CSSProperties }

/** The band's fill for a status row — its builtin class, or a 10%-alpha wash
 *  off a custom row's own hex. */
export function statusGroupWash(status: StatusRowOption): Wash {
  if (status.builtinKey) {
    return { className: BUILTIN_STATUS_WASH_CLASS[status.builtinKey] ?? `bg-zinc-500/10` }
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
  overlay,
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
  /** EXP-998: a paint-only layer over the band (the blocks rail's lanes). */
  overlay?: ReactNode
}) {
  return (
    <IssueGroupBand
      overlay={overlay}
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
