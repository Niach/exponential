import * as React from "react"

import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"

// EXP-962 — the DISCLOSURE header: the one-line fold toggle a feed row,
// a subagent lane, a tool group or a workflow agent opens and closes with.
// Muted at rest and brightening on hover, a 12px chevron that points right
// when folded and down when open, `aria-expanded` stating the fold, the
// whole line the click target. Eight call sites in the steer feed and the
// workflow card drew it by hand before this.
//
// It is NOT the group band: `GlassSectionHeader foldable` / `IssueGroupBand`
// is a filled strip heading a LIST; this is bare text heading a fold INSIDE
// a row. Chevron placement: `leading` (the default) sits before the label,
// the way a tree folds; `trailing` parks it at the far edge after a spacer
// — a row whose siblings carry no chevron must not indent out of line
// with them (the tool row's output fold, iOS/Android parity).
//
// The header may NOT contain another button: a fold's own action (a
// group's "+", a row's menu) renders beside it, not inside it.

const ChevronRightGlyph = conceptIcon(`ui-chevron-right`)
const ChevronDownGlyph = conceptIcon(`ui-chevron-down`)

export function DisclosureHeader({
  open,
  onToggle,
  chevron = `leading`,
  className,
  children,
  ...props
}: Omit<
  React.ComponentProps<`button`>,
  `onClick` | `type` | `aria-expanded` | `children`
> & {
  open: boolean
  onToggle: () => void
  chevron?: `leading` | `trailing`
  children: React.ReactNode
}) {
  const Glyph = open ? ChevronDownGlyph : ChevronRightGlyph
  const glyph = <Glyph data-slot="disclosure-chevron" className="size-3 shrink-0" />
  return (
    <button
      type="button"
      data-slot="disclosure-header"
      data-chevron={chevron}
      aria-expanded={open}
      onClick={onToggle}
      className={cn(
        `flex w-full min-w-0 cursor-pointer items-center gap-2 rounded-sm text-left text-muted-foreground transition-colors duration-fast outline-none hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50`,
        className
      )}
      {...props}
    >
      {chevron === `leading` && glyph}
      {children}
      {chevron === `trailing` && (
        <>
          <span className="min-w-0 flex-1" />
          {glyph}
        </>
      )}
    </button>
  )
}
