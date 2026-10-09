import { useMemo, useState } from "react"
import type { Issue } from "@/db/schema"
import { PickerItemBody, type PickerItem } from "@exp/ui"
import { PriorityIcon } from "@/components/issue-properties/priority-dropdown"
import { IssuePickerBody } from "@/components/issue-picker-host"

// EXP-825: the composer's issue picker. Multi-select: a row toggles, checked
// rows pin to the top, the rest is the codeable pool the hook derived
// (repo-backed boards, codeable anchor status, not running), ranked by the
// shared engine (EXP-892).
//
// EXP-1249: the composer's "+" menu hosts it as the "Implement issue"
// submenu body (a pushed page in the phone sheet), so this is THE issue
// picker host's BODY (`IssuePickerBody`) with no surface of its own. The row
// BODY stays the composer's: the priority glyph and the identifier in its own
// mono column before the primitive's own body (the status glyph is still the
// shared item's, drawn by `PickerItemBody`).

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

/** The searchable multi list; it stays put across toggles — a batch is
 *  several picks. */
export function ComposerIssuePicker({
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
  const byId = useMemo(
    () => new Map(eligible.map((issue) => [issue.id, issue])),
    [eligible]
  )
  return (
    <div
      data-testid="agent-composer-issues-picker"
      className="flex max-h-[min(24rem,var(--radix-dropdown-menu-content-available-height,24rem))] min-h-0 flex-col"
    >
      <IssuePickerBody
        mode="multi"
        teamId={teamId}
        rows={eligible}
        checked={checked}
        onToggle={onToggle}
        query={query}
        onQueryChange={setQuery}
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
      />
    </div>
  )
}
