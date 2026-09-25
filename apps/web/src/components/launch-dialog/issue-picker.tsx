import { useMemo, useState, type ReactNode } from "react"
import type { Issue } from "@/db/schema"
import {
  IssuePicker as SharedIssuePicker,
  PickerItemBody,
  PickerList,
  issuePickerItems,
  type IssuePickerIssue,
  type PickerItem,
} from "@exp/ui"
import { toStatusPickerStatus } from "@/components/issue-properties/status-dropdown"
import { PriorityIcon } from "@/components/issue-properties/priority-dropdown"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import { useIssueSearchResults } from "@/hooks/use-issue-search-results"

// EXP-825: the composer's issue picker — the launch dialog's Issues tab
// (EXP-257/EXP-768) as a popover off the card's `#` tool. Multi-select: a row
// toggles, checked rows pin to the top, the rest is the codeable pool the
// hook derived (repo-backed boards, codeable anchor status, not running).
// EXP-892: the list's own filter is OFF — the rows come ranked from the shared
// engine (`shouldFilter={false}`, so the primitive renders them verbatim and
// never reorders) — and cmdk keeps the shared list contract (top row selected
// while typing, ↑/↓ step, Enter toggles, hover moves the selection).
//
// EXP-1030: the shell is the SHARED issue picker (`@exp/ui` `IssuePicker` /
// `PickerList` in `multi` mode, EXP-1021) — the same rows the relations
// linker and the duplicate picker draw, marked the primitive's way (the
// picked row's own highlight, never a leading circle). The row BODY stays the
// composer's, because the primitive's default one would lose two things this
// list needs: the priority glyph and the identifier in its own mono column.
// Everything inside it is still the primitive's: the status glyph and its
// colour come from the shared item (`icon`/`color`, resolved against the
// team's synced statuses exactly as every list row resolves them) and are
// drawn by `PickerItemBody`, so this file owns no second glyph rule.

// Cap the unchecked results so a huge board can't blow up the list.
const MAX_UNCHECKED = 50

/** The ranked rows + their bodies, shared by the inline list and the popover. */
function useIssueRows({
  teamId,
  eligible,
  checked,
}: {
  teamId?: string
  eligible: Issue[]
  checked: Issue[]
}) {
  const { resolve } = useTeamStatusesContext()
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
  // The caller owns the ordering: checked first, then the ranked matches.
  const rows = useMemo(() => [...checked, ...matches], [checked, matches])
  const rowsById = useMemo(
    () => new Map(rows.map((issue) => [issue.id, issue])),
    [rows]
  )
  // The shared picker's rows: `IDENT Title` behind the issue's own status
  // glyph, in the status' colour (a builtin's token class or a custom row's
  // hex — `PickerItemBody` knows which is which).
  const issues = useMemo<IssuePickerIssue[]>(
    () =>
      rows.map((issue) => {
        const status = toStatusPickerStatus(resolve(issue))
        return {
          id: issue.id,
          identifier: issue.identifier,
          title: issue.title,
          icon: status.icon,
          color: status.colorHex ?? undefined,
        }
      }),
    [rows, resolve]
  )
  // The surface-less arm is handed the same rows, through the typed picker's
  // own row builder — one row shape, two shells.
  const items = useMemo(() => issuePickerItems(issues), [issues])
  const values = useMemo(
    () => rows.filter((issue) => checkedIds.has(issue.id)).map((i) => i.id),
    [rows, checkedIds]
  )

  const renderItem = (item: PickerItem) => {
    const issue = rowsById.get(item.value)
    if (!issue) return null
    return (
      <>
        <PriorityIcon priority={issue.priority} className="size-4 shrink-0" />
        <span className="min-w-[3.75rem] shrink-0 font-mono text-xs text-muted-foreground">
          {issue.identifier}
        </span>
        {/* The primitive's own body, minus the identifier this row already
            prints in its mono column: the status glyph in its colour, then
            the title. */}
        <PickerItemBody item={{ ...item, label: issue.title }} />
      </>
    )
  }

  const emptyText =
    eligible.length === 0
      ? `No codeable issues in repo-backed boards.`
      : `No matches. Only open issues are shown.`

  return { query, setQuery, issues, items, values, renderItem, emptyText }
}

/** Diff the primitive's whole-selection change back into one toggled id. */
function toggledId(before: readonly string[], after: readonly string[]) {
  return (
    after.find((id) => !before.includes(id)) ??
    before.find((id) => !after.includes(id))
  )
}

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
  const { query, setQuery, items, values, renderItem, emptyText } =
    useIssueRows({ teamId, eligible, checked })
  return (
    // EXP-946: the list SHRINKS with its host — the composer sits low, so the
    // popover flips above and has to fit in whatever is left over the card.
    <PickerList
      mode="multi"
      items={items}
      value={values}
      onChange={(next) => {
        const changed = toggledId(values, next)
        if (changed) onToggle(changed)
      }}
      search
      shouldFilter={false}
      query={query}
      onQueryChange={setQuery}
      searchPlaceholder="Search issues"
      emptyText={emptyText}
      renderItem={renderItem}
    />
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
  const { query, setQuery, issues, values, renderItem, emptyText } =
    useIssueRows({ teamId, eligible, checked })
  return (
    // EXP-946: the picker used to open above the composer and run off the TOP
    // of the window — nothing capped it, so Radix had no fitting side to
    // choose and the panel simply overflowed. The primitive caps every popover
    // to the space its chosen side has (with a 12px gutter).
    <SharedIssuePicker
      mode="multi"
      issues={issues}
      value={values}
      onChange={(next) => {
        const changed = toggledId(values, next)
        if (changed) onToggle(changed)
      }}
      shouldFilter={false}
      query={query}
      onQueryChange={setQuery}
      disabled={disabled}
      width="xl"
      mobileTitle="Issues"
      searchPlaceholder="Search issues"
      emptyText={emptyText}
      data-testid="agent-composer-issues-picker"
      renderItem={renderItem}
      trigger={children}
    />
  )
}
