import { useMemo, useState, type ReactNode } from "react"
import type { Issue } from "@/db/schema"
import {
  Combobox,
  ComboboxList,
  type PickerOption,
} from "@exp/ui"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"
import { PriorityIcon } from "@/components/issue-properties/priority-dropdown"
import { useIssueSearchResults } from "@/hooks/use-issue-search-results"

// EXP-825: the composer's issue picker — the launch dialog's Issues tab
// (EXP-257/EXP-768) as a popover off the card's `#` tool. Multi-select: a row
// toggles, checked rows pin to the top, the rest is the codeable pool the
// hook derived (repo-backed boards, codeable anchor status, not running).
// The row anatomy is the mobile one: selection glyph, priority, identifier,
// status, title. EXP-892: the list's own filter is OFF — the rows come ranked
// from the shared engine (`shouldFilter={false}`, so the primitive renders
// them verbatim and never reorders) — and cmdk keeps the shared list contract
// (top row selected while typing, ↑/↓ step, Enter toggles, hover moves the
// selection). EXP-941: the shell is the shared `Combobox`.

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
  const options = useMemo<PickerOption[]>(
    () => rows.map((issue) => ({ value: issue.id, label: issue.title })),
    [rows]
  )
  const values = useMemo(
    () => rows.filter((issue) => checkedIds.has(issue.id)).map((i) => i.id),
    [rows, checkedIds]
  )

  const renderOption = (option: PickerOption) => {
    const issue = rowsById.get(option.value)
    if (!issue) return null
    return (
      <>
        <PriorityIcon priority={issue.priority} className="size-4 shrink-0" />
        <span className="min-w-[3.75rem] shrink-0 font-mono text-xs text-muted-foreground">
          {issue.identifier}
        </span>
        <IssueStatusIcon issue={issue} className="size-4 shrink-0" />
        <span className="min-w-0 flex-1 truncate text-sm">{issue.title}</span>
      </>
    )
  }

  const emptyText =
    eligible.length === 0
      ? `No codeable issues in repo-backed boards.`
      : `No matches. Only open issues are shown.`

  return { query, setQuery, options, values, renderOption, emptyText }
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
  const { query, setQuery, options, values, renderOption, emptyText } =
    useIssueRows({ teamId, eligible, checked })
  return (
    // EXP-946: the list SHRINKS with its host — the composer sits low, so the
    // popover flips above and has to fit in whatever is left over the card.
    <ComboboxList
      multiple
      options={options}
      value={values}
      onChange={(next) => {
        const changed = toggledId(values, next)
        if (changed) onToggle(changed)
      }}
      shouldFilter={false}
      query={query}
      onQueryChange={setQuery}
      placeholder="Search issues"
      emptyText={emptyText}
      renderOption={renderOption}
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
  const { query, setQuery, options, values, renderOption, emptyText } =
    useIssueRows({ teamId, eligible, checked })
  return (
    // EXP-946: the picker used to open above the composer and run off the TOP
    // of the window — nothing capped it, so Radix had no fitting side to
    // choose and the panel simply overflowed. The primitive caps every popover
    // to the space its chosen side has (with a 12px gutter).
    <Combobox
      multiple
      options={options}
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
      placeholder="Search issues"
      emptyText={emptyText}
      data-testid="agent-composer-issues-picker"
      renderOption={renderOption}
      renderTrigger={() => children}
    />
  )
}
