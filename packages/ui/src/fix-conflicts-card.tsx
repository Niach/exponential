import type { ComponentProps, ReactNode } from "react"

import { cn } from "./cn"
import { GlassGroup } from "./glass-rows"
import { conceptIcon } from "./icons.generated"

// EXP-1233 — the Fix merge conflicts CARD, presentational half.
//
// A merge refused by a REAL conflict opens the composer on the Fix merge
// conflicts builtin with the pull request picked; the composer then draws
// that builtin with its own look instead of the generic action chip + a
// "Pull request" field. This file is the card's three pieces, byte-for-byte
// the same on every client (desktop `chat_screen::fix_conflicts_card`, iOS
// `FixConflictsCard`, Android `FixConflictsCard`):
//
//   ┌ ⑂ #2117  ⎇ exp/APP-14 → master                            ⌄ ┐  PR row
//   │ ⚠ Merge refused: the branch has conflicts.                   │  note
//   └──────────────────────────────────────────────────────────────┘
//
// The PR ROW is the picker's trigger (the chevron says so): the open-PR
// glyph in emerald, the number in mono foreground, the branch glyph and
// `branch → base` in mono at 70% (the base omitted while unknown,
// `branchLine`); nothing picked = the muted placeholder alone. The NOTE
// under a hairline shows only when a refused merge opened the composer
// (the seed's `conflict`) — a pick made from the action picker has nothing
// to explain. The surface is `GlassGroup`, the form ladder's own.
//
// The app binds the row to its `Combobox` (`renderTrigger`) and supplies the
// words from `contract.composerUi`; the styleguide island draws the pieces
// with the same words literal.

const PrOpenIcon = conceptIcon(`pr-open`)
const UiBranchIcon = conceptIcon(`ui-branch`)
const UiWarningIcon = conceptIcon(`ui-warning`)
const ChevronIcon = conceptIcon(`ui-chevron-down`)

/** `exp/APP-14 → master`, or the branch alone while the base is unknown;
 *  `` without a branch. Pure, so the arrow rule is a test (×4). */
export function branchLine(
  branch: string | null | undefined,
  baseBranch: string | null | undefined
): string {
  if (!branch) return ``
  return baseBranch ? `${branch} → ${baseBranch}` : branch
}

/** The picked pull request as the row shows it. */
export interface FixConflictsPr {
  prNumber: number | null
  branch: string | null
  baseBranch: string | null
}

/** The glass surface the row and the note stack in. */
export function FixConflictsCard({
  className,
  children,
  ...props
}: ComponentProps<`div`> & { children: ReactNode }) {
  return (
    <GlassGroup
      data-slot="fix-conflicts-card"
      className={className}
      {...props}
    >
      {children}
    </GlassGroup>
  )
}

/** The PR row — a button, so the host can wrap it as its picker's trigger
 *  (`asChild`). `pr` null = nothing picked, the placeholder alone. */
export function FixConflictsPrRow({
  pr,
  placeholder,
  open = false,
  className,
  ...props
}: Omit<ComponentProps<`button`>, `children`> & {
  pr: FixConflictsPr | null
  /** The row's words while nothing is picked (`composerUi.prPlaceholder`). */
  placeholder: string
  /** The picker is up (the host's popover state). */
  open?: boolean
}) {
  const line = pr ? branchLine(pr.branch, pr.baseBranch) : ``
  return (
    <button
      type="button"
      {...props}
      data-slot="fix-conflicts-pr-row"
      data-state={open ? `open` : `closed`}
      aria-label={
        pr
          ? pr.prNumber !== null
            ? `Pull request #${pr.prNumber}`
            : `Pull request`
          : placeholder
      }
      className={cn(
        `flex w-full items-center gap-3 px-4 py-3 text-left text-sm transition-colors duration-fast outline-none`,
        `hover:bg-glass-active/50 focus-visible:ring-[3px] focus-visible:ring-inset focus-visible:ring-ring/50`,
        `disabled:pointer-events-none disabled:opacity-50`,
        className
      )}
    >
      {pr ? (
        <>
          <PrOpenIcon aria-hidden className="size-4 shrink-0 text-emerald-500" />
          {pr.prNumber !== null && (
            <span className="shrink-0 font-mono text-foreground">
              #{pr.prNumber}
            </span>
          )}
          {line && (
            <span className="flex min-w-0 items-center gap-1.5 font-mono text-foreground/70">
              <UiBranchIcon aria-hidden className="size-3.5 shrink-0" />
              <span className="min-w-0 truncate">{line}</span>
            </span>
          )}
        </>
      ) : (
        <span className="min-w-0 flex-1 truncate text-muted-foreground">
          {placeholder}
        </span>
      )}
      <ChevronIcon
        aria-hidden
        className="ml-auto size-3.5 shrink-0 text-foreground/50"
      />
    </button>
  )
}

/** The refusal line under the row: the warning glyph and the contract's
 *  `conflictNote`, in the destructive tone. */
export function FixConflictsNote({
  children,
  className,
  ...props
}: ComponentProps<`div`> & { children: ReactNode }) {
  return (
    <div
      {...props}
      data-slot="fix-conflicts-note"
      className={cn(
        `flex items-center gap-2 px-4 py-2.5 text-sm text-destructive`,
        className
      )}
    >
      <UiWarningIcon aria-hidden className="size-4 shrink-0" />
      <span className="min-w-0 truncate">{children}</span>
    </div>
  )
}
