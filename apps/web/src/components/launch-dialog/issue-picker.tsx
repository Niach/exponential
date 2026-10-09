import { useMemo, useState, type ReactNode } from "react"
import type { Issue } from "@/db/schema"
import { PickerItemBody, type PickerItem } from "@exp/ui"
import { PriorityIcon } from "@/components/issue-properties/priority-dropdown"
import { IssuePickerHost } from "@/components/issue-picker-host"

// EXP-825: the composer's issue picker — a popover off the card's `#` tool.
// Multi-select: a row toggles, checked rows pin to the top, the rest is the
// codeable pool the hook derived (repo-backed boards, codeable anchor status,
// not running), ranked by the shared engine (EXP-892).
//
// The UI cleanup batch: THE issue picker host (`issue-picker-host.tsx`) in its
// `popover` shell (a bottom sheet on phones). The row BODY stays the
// composer's: the priority glyph and the identifier in its own mono column
// before the primitive's own body (the status glyph is still the shared
// item's, drawn by `PickerItemBody`).

// Cap the unchecked results so a huge board can't blow up the list.
const MAX_UNCHECKED = 50

function renderComposerRow(issue: Issue, item: PickerItem) {
  return (
    <>
      <PriorityIcon priority={issue.priority} className="size-4 shrink-0" />
      <span className="min-w-[3.75rem] shrink-0 font-mono text-xs text-muted-foreground">
        {issue.identifier}
      </span>
      <PickerItemBody item={{ ...item, label: issue.title }} />
    </>
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
  /** The team `issues.search` runs against (EXP-892); undefined = local only. */
  teamId?: string
  /** The codeable pool, newest first (hook-derived). */
  eligible: Issue[]
  /** The checked issues' rows, pinned first. */
  checked: Issue[]
  onToggle: (issueId: string) => void
  disabled?: boolean
  /** The trigger (a `ComposerTool`). */
  children: ReactNode
}) {
  const [open, setOpen] = useState(false)
  const byId = useMemo(
    () => new Map(eligible.map((issue) => [issue.id, issue])),
    [eligible]
  )
  return (
    <IssuePickerHost
      shell="popover"
      mode="multi"
      open={disabled ? false : open}
      onOpenChange={(next) => setOpen(disabled ? false : next)}
      title="Issues"
      trigger={children}
      teamId={teamId}
      rows={eligible}
      checked={checked}
      onToggle={onToggle}
      limit={MAX_UNCHECKED}
      // The pool is a subset of the team: a hit outside it is not codeable.
      resolveHit={(hit) => byId.get(hit.id) ?? null}
      searchPlaceholder="Search issues"
      emptyText={() =>
        eligible.length === 0
          ? `No codeable issues in repo-backed boards.`
          : `No matches. Only open issues are shown.`
      }
      renderItem={renderComposerRow}
      data-testid="agent-composer-issues-picker"
    />
  )
}
