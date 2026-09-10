import { useMemo, useState, type ReactNode } from "react"
import type { Issue } from "@/db/schema"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import {
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
} from "@/components/mobile-popover"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"
import { PriorityIcon } from "@/components/issue-properties/priority-dropdown"
import { conceptIcon } from "@/lib/icons.generated"
import { cn } from "@/lib/utils"

// EXP-825: the composer's issue picker — the launch dialog's Issues tab
// (EXP-257/EXP-768) as a popover off the card's `#` tool. Multi-select: a row
// toggles, checked rows pin to the top, the rest is the codeable pool the
// hook derived (repo-backed boards, codeable anchor status, not running).
// The row anatomy is the mobile one: selection glyph, priority, identifier,
// status, title.

// Cap the unchecked results so a huge board can't blow up the list.
const MAX_UNCHECKED = 50

// The natives' circle / circle-check selection glyph (EXP-721 row idiom).
const SelectedIcon = conceptIcon(`ui-selected`)
const UnselectedIcon = conceptIcon(`ui-unselected`)

export function IssuePickerList({
  eligible,
  checked,
  onToggle,
}: {
  /** The codeable pool, newest first (hook-derived). */
  eligible: Issue[]
  /** The checked issues' rows, pinned first. */
  checked: Issue[]
  onToggle: (issueId: string) => void
}) {
  const checkedIds = useMemo(
    () => new Set(checked.map((issue) => issue.id)),
    [checked]
  )
  const rows = useMemo(
    () => [
      ...checked,
      ...eligible.filter((issue) => !checkedIds.has(issue.id)).slice(0, MAX_UNCHECKED),
    ],
    [checked, eligible, checkedIds]
  )
  return (
    <Command>
      <CommandInput placeholder="Search issues" />
      <CommandList data-testid="agent-composer-issues-picker">
        <CommandEmpty>No codeable issues in repo-backed boards.</CommandEmpty>
        <CommandGroup>
          {rows.map((issue) => {
            const isChecked = checkedIds.has(issue.id)
            return (
              <CommandItem
                key={issue.id}
                value={issue.id}
                keywords={[issue.identifier, issue.title]}
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
  eligible,
  checked,
  onToggle,
  disabled,
  children,
}: {
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
        <IssuePickerList eligible={eligible} checked={checked} onToggle={onToggle} />
      </MobilePopoverContent>
    </MobilePopover>
  )
}
