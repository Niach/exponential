import { useMemo, useState, type ReactNode } from "react"
import type { Issue } from "@/db/schema"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
  conceptIcon,
} from "@exp/ui"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"
import { PriorityIcon } from "@/components/issue-properties/priority-dropdown"
import { cn } from "@/lib/utils"
import { useIssueSearchResults } from "@/hooks/use-issue-search-results"

// EXP-825: the composer's issue picker — the launch dialog's Issues tab
// (EXP-257/EXP-768) as a popover off the card's `#` tool. Multi-select: a row
// toggles, checked rows pin to the top, the rest is the codeable pool the
// hook derived (repo-backed boards, codeable anchor status, not running).
// The row anatomy is the mobile one: selection glyph, priority, identifier,
// status, title. EXP-892: cmdk's own filter is OFF — the rows come ranked
// from the shared engine — and cmdk keeps the shared list contract (top row
// selected while typing, ↑/↓ step, Enter toggles, hover moves the selection).

// Cap the unchecked results so a huge board can't blow up the list.
const MAX_UNCHECKED = 50

// The natives' circle / circle-check selection glyph (EXP-721 row idiom).
const SelectedIcon = conceptIcon(`ui-selected`)
const UnselectedIcon = conceptIcon(`ui-unselected`)

export function IssuePickerList({
  teamId,
  eligible,
  checked,
  onToggle,
}: {
  /** The team `issues.search` runs against (EXP-892); undefined = local only. */
  teamId?: string
  /** The codeable pool, newest first (hook-derived). */
  eligible: Issue[]
  /** The checked issues' rows, pinned first. */
  checked: Issue[]
  onToggle: (issueId: string) => void
}) {
  const [query, setQuery] = useState(``)
  const checkedIds = useMemo(
    () => new Set(checked.map((issue) => issue.id)),
    [checked]
  )
  const unchecked = useMemo(
    () => eligible.filter((issue) => !checkedIds.has(issue.id)),
    [eligible, checkedIds]
  )
  const uncheckedById = useMemo(
    () => new Map(unchecked.map((issue) => [issue.id, issue])),
    [unchecked]
  )
  // EXP-892: the shared engine over the codeable pool — local ranking now,
  // the server's full-text hits (description + comment matches) spliced in
  // behind, RESTRICTED to the pool: a hit outside it is not codeable here.
  const { results: matches } = useIssueSearchResults({
    teamId,
    query,
    rows: unchecked,
    limit: MAX_UNCHECKED,
    resolveHit: (hit) => uncheckedById.get(hit.id) ?? null,
  })
  const rows = useMemo(() => [...checked, ...matches], [checked, matches])
  return (
    <Command shouldFilter={false}>
      <CommandInput
        placeholder="Search issues"
        value={query}
        onValueChange={setQuery}
      />
      <CommandList data-testid="agent-composer-issues-picker">
        <CommandEmpty>
          {eligible.length === 0
            ? `No codeable issues in repo-backed boards.`
            : `No matches. Only open issues are shown.`}
        </CommandEmpty>
        <CommandGroup>
          {rows.map((issue) => {
            const isChecked = checkedIds.has(issue.id)
            return (
              <CommandItem
                key={issue.id}
                value={issue.id}
                onSelect={() => onToggle(issue.id)}
                aria-pressed={isChecked}
                className={cn(
                  `flex items-center gap-2.5`,
                  isChecked && `bg-glass-active`
                )}
              >
                {isChecked ? (
                  <SelectedIcon className="size-4 shrink-0 text-foreground" />
                ) : (
                  <UnselectedIcon className="size-4 shrink-0 text-muted-foreground" />
                )}
                <PriorityIcon priority={issue.priority} className="size-4 shrink-0" />
                <span className="min-w-[3.75rem] shrink-0 font-mono text-xs text-muted-foreground">
                  {issue.identifier}
                </span>
                <IssueStatusIcon issue={issue} className="size-4 shrink-0" />
                <span className="min-w-0 flex-1 truncate text-sm">{issue.title}</span>
              </CommandItem>
            )
          })}
        </CommandGroup>
      </CommandList>
    </Command>
  )
}

/** The `#` tool's popover (a bottom sheet on phones). Stays open across
 * toggles — a batch is several picks. */
export function IssuePicker({
  teamId,
  eligible,
  checked,
  onToggle,
  disabled,
  children,
}: {
  teamId?: string
  eligible: Issue[]
  checked: Issue[]
  onToggle: (issueId: string) => void
  disabled?: boolean
  /** The trigger (a `ComposerTool`). */
  children: ReactNode
}) {
  const [open, setOpen] = useState(false)
  return (
    <MobilePopover open={open} onOpenChange={setOpen}>
      <MobilePopoverTrigger asChild disabled={disabled}>
        {children}
      </MobilePopoverTrigger>
      <MobilePopoverContent
        className="w-[22rem] p-0"
        align="start"
        mobileTitle="Issues"
      >
        <IssuePickerList
          teamId={teamId}
          eligible={eligible}
          checked={checked}
          onToggle={onToggle}
        />
      </MobilePopoverContent>
    </MobilePopover>
  )
}
