import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { Issue, Label, Board, User } from "@/db/schema"
import { boardCollection } from "@/lib/collections"
import {
  StatusDropdown,
  statusColorClass,
  statusColorStyle,
} from "@/components/issue-properties/status-dropdown"
import { PriorityDropdown } from "@/components/issue-properties/priority-dropdown"
import { AssigneeDropdown } from "@/components/issue-properties/assignee-dropdown"
import { IssueRowContextMenu } from "@/components/issue-row-menu/context-menu"
import { EmptyState } from "@/components/empty-state"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Skeleton } from "@/components/ui/skeleton"
import { Collapsible as CollapsiblePrimitive } from "radix-ui"
import {
  CalendarDays,
  Plus,
  ChevronRight,
  ListTodo,
  SearchX,
} from "lucide-react"
import { formatDate } from "@/lib/utils"
import { useIsMobile } from "@/hooks/use-mobile"
import { type IssueStatus } from "@/lib/domain"
import { useToday } from "@/hooks/use-now"
import { dueDateToneClass } from "@/lib/issue-due-date"
import { ICON_COMPONENTS } from "@/lib/icons.generated"
import { hexWithAlpha } from "@/lib/status-icons"
import type { StatusRowOption } from "@/lib/team-statuses"
import type { IssueGroup } from "@/lib/board-view"

// REV-46: the desktop IDE virtualizes this exact list (issue_list.rs
// v_virtual_list — "the list can be long; virtualization is mandatory"). The
// web analog follows diff-view.tsx instead: cap + expand, so a board with
// thousands of issues never mounts thousands of interactive rows (each row is
// a Radix context menu around three dropdown components) in one commit.
// Each group renders this many rows before a "Show more" button takes over.
const GROUP_ROW_CAP = 100
// Every "Show more" click reveals this many additional rows.
const GROUP_ROW_CHUNK = 400

// Status-tinted washes for the sticky group headers — the Tailwind palette
// colors the old rgba literals encoded (zinc-500/zinc-300/yellow-500/
// green-500/blue-500), matching the status icon hues in lib/domain.ts.
// EXP-314: BUILTIN rows keep these exact classes (keyed on the builtin key, so
// the default team's headers are byte-identical to before); CUSTOM rows get a
// 10%-alpha inline wash from their own hex.
const statusHeaderBg: Record<IssueStatus, string> = {
  backlog: `bg-zinc-500/10`,
  todo: `bg-zinc-300/10`,
  in_progress: `bg-yellow-500/10`,
  in_review: `bg-green-500/10`,
  done: `bg-blue-500/10`,
  cancelled: `bg-zinc-500/10`,
  duplicate: `bg-zinc-500/10`,
}

function groupHeaderWash(option: StatusRowOption): {
  className: string
  style?: React.CSSProperties
} {
  if (option.builtinKey) {
    return { className: statusHeaderBg[option.builtinKey] ?? `bg-zinc-500/10` }
  }
  return {
    className: ``,
    style: { backgroundColor: hexWithAlpha(option.colorHex, 0.1) },
  }
}

interface IssueListProps {
  groups: IssueGroup[]
  issueLabelMap: Map<string, Label[]>
  labels: Label[]
  users: User[]
  userMap: Map<string, User>
  onNewIssue: (status?: StatusRowOption) => void
  onIssueClick: (issue: Issue) => void
  canCreate?: boolean
  canMutateIssue?: (issue: Issue) => boolean
  // Moderator-only row controls (status, priority, assignee, due date) are
  // disabled when false. Title/description/labels remain mutable by anyone
  // whose canMutateIssue is true.
  canModerate?: boolean
  // True while the Electric issues collection is still loading its first
  // snapshot — renders skeleton rows instead of an empty state.
  isLoading?: boolean
  // Distinguish "the board has no issues" from "filters hide everything".
  hasAnyIssues?: boolean
  hasActiveFilters?: boolean
  onClearFilters?: () => void
  // Rendered below the genuine "No issues yet" empty state only (never the
  // filtered-empty one) — the board passes the member-only "Getting
  // started" cards here (EXP-88).
  emptyStateExtra?: React.ReactNode
  // Optional trailing per-row action cell. Rendered in its own
  // click-isolated grid column. Rows are memoized (REV-46), so the output
  // must be a function of the issue row alone — external state it closes
  // over won't re-render untouched rows.
  renderRowAction?: (issue: Issue) => React.ReactNode
  // Enables bulk selection (hover checkboxes on md+, shift-click ranges,
  // Cmd/Ctrl+A, Esc). Undefined = bulk select off. Selection also requires
  // canModerate.
  bulkTeamId?: string
  // Selection state is owned by the parent so it can render the BulkActionBar
  // in the header region above the scroll container (EXP-251). The setter is
  // a state dispatcher because the prune effect and keyboard handlers rely on
  // functional updates.
  selectedIds?: Set<string>
  onSelectedIdsChange?: React.Dispatch<React.SetStateAction<Set<string>>>
}

const EMPTY_SELECTION = new Set<string>()
const noopSetSelectedIds: React.Dispatch<React.SetStateAction<Set<string>>> =
  () => {}
// Shared empty-labels instance — a fresh `[]` per label-less row would break
// the row memo on every parent render.
const NO_LABELS: Label[] = []

function IssueListSkeleton() {
  return (
    <div data-testid="issue-list-skeleton">
      <div className="flex items-center gap-2 pl-3 pr-3 md:pr-6 py-2 border-b border-border/50 bg-accent/20">
        <Skeleton className="h-3.5 w-3.5 rounded-full" />
        <Skeleton className="h-3.5 w-24" />
      </div>
      {Array.from({ length: 5 }, (_, i) => (
        <div
          key={i}
          className="flex items-center gap-3 h-12 md:h-10 px-3 md:px-6 border-b border-border/30"
        >
          <Skeleton className="h-4 w-4 rounded-full" />
          <Skeleton className="hidden md:block h-3 w-14" />
          <Skeleton className="h-4 w-4 rounded-full" />
          <Skeleton className="h-3.5 flex-1 max-w-72" />
        </div>
      ))}
    </div>
  )
}

interface IssueRowProps {
  issue: Issue
  issueLabels: Label[]
  labels: Label[]
  users: User[]
  userMap: Map<string, User>
  teamBoards?: Board[]
  rowGridClass: string
  today: string
  isSolo: boolean
  bulkEnabled: boolean
  mobileSelectionActive: boolean
  isSelected: boolean
  // Any selection exists — keeps every row's checkbox visible (not just
  // hovered) while a selection is in progress.
  anySelected: boolean
  // rowCanMutate && canModerate, precomputed so the memo compares a boolean.
  canMutateRow: boolean
  onOpen: (issue: Issue) => void
  onToggleSelect: (issueId: string, shiftKey: boolean) => void
  renderRowAction?: (issue: Issue) => React.ReactNode
}

// REV-46: memoized so a selection toggle reconciles only the toggled row —
// `selectedIds` lives in the route, so every checkbox click re-renders the
// whole page; without the memo each click re-rendered every row's context
// menu + three dropdowns. All props are primitives or referentially stable
// (the callbacks come from the latest-ref wrappers in IssueList).
const IssueRow = memo(function IssueRow({
  issue,
  issueLabels,
  labels,
  users,
  userMap,
  teamBoards,
  rowGridClass,
  today,
  isSolo,
  bulkEnabled,
  mobileSelectionActive,
  isSelected,
  anySelected,
  canMutateRow,
  onOpen,
  onToggleSelect,
  renderRowAction,
}: IssueRowProps) {
  return (
    <IssueRowContextMenu
      issue={issue}
      issueLabels={issueLabels}
      labels={labels}
      users={users}
      userMap={userMap}
      boards={teamBoards}
      onOpenIssue={() => onOpen(issue)}
      onToggleSelect={
        bulkEnabled ? () => onToggleSelect(issue.id, false) : undefined
      }
      isSelected={isSelected}
    >
      <div
        className={`grid ${rowGridClass} items-center h-12 md:h-10 px-3 md:px-6 hover:bg-glass-row border-b border-border/30 group/row cursor-pointer ${isSelected ? `max-md:bg-glass-active` : ``}`}
        onClick={() => {
          if (mobileSelectionActive) {
            onToggleSelect(issue.id, false)
            return
          }
          onOpen(issue)
        }}
        data-testid={`issue-row-${issue.identifier}`}
      >
        {bulkEnabled && (
          <div
            // self-stretch + the padding bleed grow the toggle
            // hitbox to the full row height and the row's left
            // padding — a click slightly beside the checkbox
            // must select, never open the issue (FEED-12).
            className="hidden md:flex items-center self-stretch -ml-6 pl-6"
            // Suppress the browser's shift-click text selection
            // so range-select doesn't highlight row text.
            onMouseDown={(e) => {
              if (e.shiftKey) e.preventDefault()
            }}
            onClick={(e) => {
              e.stopPropagation()
              onToggleSelect(issue.id, e.shiftKey)
            }}
          >
            <Checkbox
              checked={isSelected}
              aria-label={`Select ${issue.identifier}`}
              className={`transition-opacity ${anySelected ? `opacity-100` : `opacity-0 group-hover/row:opacity-100`}`}
            />
          </div>
        )}
        <div
          className="flex items-center justify-center"
          onClick={(e) => e.stopPropagation()}
        >
          <PriorityDropdown
            issueId={issue.id}
            priority={issue.priority}
            disabled={!canMutateRow}
          />
        </div>
        <span className="hidden md:inline text-xs text-muted-foreground font-mono truncate">
          {issue.identifier}
        </span>
        <div
          className="flex items-center justify-center"
          onClick={(e) => e.stopPropagation()}
        >
          <StatusDropdown
            issueId={issue.id}
            status={issue.status}
            statusId={issue.statusId}
            disabled={!canMutateRow}
          />
        </div>
        <span className="flex items-center gap-1.5 text-sm truncate ml-2 min-w-0">
          <span className="truncate">{issue.title}</span>
        </span>
        <div className="hidden md:flex items-center gap-1.5 ml-4 shrink-0">
          {issueLabels.map((label) => (
            <span
              key={label.id}
              className="flex items-center gap-1 border border-border/50 rounded-full px-1.5 py-px text-xs text-muted-foreground"
            >
              <div
                className="h-1.5 w-1.5 rounded-full shrink-0"
                style={{ backgroundColor: label.color }}
              />
              {label.name}
            </span>
          ))}
        </div>
        {!isSolo && (
          <div
            className="hidden md:flex items-center justify-center"
            onClick={(e) => e.stopPropagation()}
          >
            <AssigneeDropdown
              issueId={issue.id}
              assigneeId={issue.assigneeId}
              users={users}
              userMap={userMap}
              disabled={!canMutateRow}
            />
          </div>
        )}
        {/* Display-only: due dates are edited in the issue
            detail, never inline from the list (EXP-247). The
            tone (red overdue / orange today) is what explains
            the overdue-first ordering — REV2-48. */}
        <div className="flex items-center justify-end">
          {issue.dueDate && (
            <span
              className={`flex items-center gap-1 px-1 ${dueDateToneClass(issue.dueDate, today)}`}
            >
              <CalendarDays className="size-3 shrink-0" />
              <span className="text-xs whitespace-nowrap">
                {formatDate(issue.dueDate)}
              </span>
            </span>
          )}
        </div>
        {renderRowAction && (
          <div
            className="flex items-center justify-end"
            onClick={(e) => e.stopPropagation()}
          >
            {renderRowAction(issue)}
          </div>
        )}
      </div>
    </IssueRowContextMenu>
  )
})

export function IssueList({
  groups,
  issueLabelMap,
  labels,
  users,
  userMap,
  onNewIssue,
  onIssueClick,
  canCreate = true,
  canMutateIssue,
  canModerate = true,
  isLoading = false,
  hasAnyIssues = false,
  hasActiveFilters = false,
  onClearFilters,
  emptyStateExtra,
  renderRowAction,
  bulkTeamId,
  selectedIds = EMPTY_SELECTION,
  onSelectedIdsChange: setSelectedIds = noopSetSelectedIds,
}: IssueListProps) {
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set())
  // Extra rows revealed per group id beyond GROUP_ROW_CAP via "Show more"
  // (REV-46). Only ever grows; a stale entry after a filter change is just a
  // higher cap for that group.
  const [extraRows, setExtraRows] = useState<Map<string, number>>(new Map())
  // The shift-range anchor is never rendered — a ref keeps toggleSelect
  // referentially stable so it doesn't re-render every memoized row.
  const anchorIdRef = useRef<string | null>(null)
  const listRef = useRef<HTMLDivElement>(null)
  // Local-date boundary for the due-date tone — the same one the overdue-first
  // comparator sorts on (lib/board-view.ts). Ticks across midnight so a
  // long-lived tab keeps tone and ordering in agreement (REV2-48).
  const today = useToday()
  const visibleGroups = groups.filter((g) => g.issues.length > 0)
  const bulkEnabled = Boolean(bulkTeamId) && canModerate
  const isMobile = useIsMobile()
  // Mobile has no checkbox column: selection starts from the row context
  // menu's Select item, and while any selection exists a row TAP toggles it
  // instead of navigating (deselecting the last row exits — the native
  // EXP-405 contract). Desktop click-to-open is untouched.
  const mobileSelectionActive = isMobile && bulkEnabled && selectedIds.size > 0

  // Team boards feed the context menu's move-to-board submenu
  // (EXP-57). Trashed boards never reach the client (the boards shape
  // filters them server-side).
  const { data: boardRows } = useLiveQuery(
    (query) =>
      bulkTeamId
        ? query
            .from({ boards: boardCollection })
            .where(({ boards }) => eq(boards.teamId, bulkTeamId))
        : undefined,
    [bulkTeamId]
  )
  const teamBoards = useMemo(
    () =>
      bulkTeamId
        ? [...((boardRows ?? []) as Board[])].sort((left, right) =>
            left.name.localeCompare(right.name)
          )
        : undefined,
    [boardRows, bulkTeamId]
  )

  const renderLimit = (groupId: string) =>
    GROUP_ROW_CAP + (extraRows.get(groupId) ?? 0)

  // The range/select-all universe: RENDERED rows in render order — windowed
  // by the group cap, minus collapsed groups. Rows hidden behind "Show more"
  // stay out deliberately: a shift-range or Cmd/Ctrl+A must never sweep up
  // rows the user hasn't revealed.
  const visibleFlatIssues = useMemo(
    () =>
      groups
        .filter(
          (group) =>
            group.issues.length > 0 && !collapsedGroups.has(group.status.id)
        )
        .flatMap((group) =>
          group.issues.slice(0, renderLimit(group.status.id))
        ),
    [groups, collapsedGroups, extraRows]
  )
  const visibleFlatIssuesRef = useRef(visibleFlatIssues)
  visibleFlatIssuesRef.current = visibleFlatIssues

  // Prune selected ids whose rows left the data set (filter change, delete
  // elsewhere, sync). Collapsing a group hides rows but keeps them selected.
  useEffect(() => {
    const present = new Set(
      groups.flatMap((group) => group.issues.map((issue) => issue.id))
    )
    setSelectedIds((prev) => {
      const next = new Set([...prev].filter((id) => present.has(id)))
      return next.size === prev.size ? prev : next
    })
  }, [groups])

  // An emptied selection (bulk-bar Clear, Esc, external reset) also drops
  // the shift-range anchor — the next shift-click must not extend a range
  // from a pre-clear row.
  useEffect(() => {
    if (selectedIds.size === 0) anchorIdRef.current = null
  }, [selectedIds])

  // Stable (reads the universe + anchor through refs) so memoized rows don't
  // re-render when the selection changes.
  const toggleSelect = useCallback(
    (issueId: string, shiftKey: boolean) => {
      const ids = visibleFlatIssuesRef.current.map((issue) => issue.id)
      const anchorId = anchorIdRef.current
      const anchorIndex = anchorId ? ids.indexOf(anchorId) : -1
      const targetIndex = ids.indexOf(issueId)
      if (shiftKey && anchorIndex !== -1 && targetIndex !== -1) {
        // Shift-click extends: ADD the contiguous visible slice between the
        // anchor and the target (anchor stays put for further extensions).
        const [from, to] =
          anchorIndex < targetIndex
            ? [anchorIndex, targetIndex]
            : [targetIndex, anchorIndex]
        const range = ids.slice(from, to + 1)
        setSelectedIds((prev) => {
          const next = new Set(prev)
          for (const id of range) next.add(id)
          return next
        })
        return
      }
      setSelectedIds((prev) => {
        const next = new Set(prev)
        if (next.has(issueId)) {
          next.delete(issueId)
        } else {
          next.add(issueId)
        }
        return next
      })
      anchorIdRef.current = issueId
    },
    [setSelectedIds]
  )

  // Latest-ref wrappers: the callers hold the selection state, so their
  // inline callbacks get a new identity on every selection change — routing
  // the calls through refs keeps the row props stable.
  const onIssueClickRef = useRef(onIssueClick)
  onIssueClickRef.current = onIssueClick
  const openIssue = useCallback(
    (issue: Issue) => onIssueClickRef.current(issue),
    []
  )
  const renderRowActionRef = useRef(renderRowAction)
  renderRowActionRef.current = renderRowAction
  const hasRowAction = Boolean(renderRowAction)
  const stableRenderRowAction = useMemo(
    () =>
      hasRowAction
        ? (issue: Issue) => renderRowActionRef.current?.(issue)
        : undefined,
    [hasRowAction]
  )

  // Cmd/Ctrl+A selects everything visible under the current filters; Escape
  // clears. Both keys are overlay-scoped: an Escape that dismisses a Radix
  // menu/dialog/popover must NOT also wipe the selection (Linear closes only
  // the menu), and select-all only fires with focus on the body or inside
  // the list — never while an overlay is up or a field elsewhere has focus.
  // Radix flushes its close via React batching AFTER this event finishes, so
  // querying open overlays during the bubble phase still sees them.
  useEffect(() => {
    if (!bulkEnabled) return
    const overlayOpen = () =>
      document.querySelector(
        `[data-state="open"][role="menu"], [data-state="open"][role="listbox"], [data-state="open"][role="dialog"]`
      ) !== null
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === `a`) {
        if (overlayOpen()) return
        const active = document.activeElement
        if (
          active instanceof HTMLElement &&
          (active instanceof HTMLInputElement ||
            active instanceof HTMLTextAreaElement ||
            active instanceof HTMLSelectElement ||
            active.isContentEditable)
        ) {
          return
        }
        if (
          active !== document.body &&
          active !== null &&
          !listRef.current?.contains(active)
        ) {
          return
        }
        event.preventDefault()
        setSelectedIds(new Set(visibleFlatIssues.map((issue) => issue.id)))
        return
      }
      if (event.key === `Escape`) {
        if (overlayOpen()) return
        setSelectedIds((prev) => (prev.size > 0 ? new Set<string>() : prev))
      }
    }
    window.addEventListener(`keydown`, handleKeyDown)
    return () => window.removeEventListener(`keydown`, handleKeyDown)
  }, [bulkEnabled, visibleFlatIssues])

  // Solo team (exactly one human member): the assignee cell is hidden
  // entirely (EXP-247 — nothing to reassign). `users` is the bot-excluded
  // member list; length 0 means still loading (never a genuine empty), so a
  // multi-member team never briefly reads as solo.
  const isSolo = users.length === 1

  // The row grid grows a leading checkbox column (md+ when bulk select is
  // on), drops the md assignee column on solo teams, and grows a trailing
  // action column when the caller renders one. Every combination is a full
  // literal — Tailwind only sees complete class strings.
  const rowGridClass = bulkEnabled
    ? renderRowAction
      ? isSolo
        ? `grid-cols-[2rem_2rem_1fr_auto_2rem] md:grid-cols-[1.25rem_1.5rem_4.5rem_1.5rem_1fr_auto_4.5rem_2rem]`
        : `grid-cols-[2rem_2rem_1fr_auto_2rem] md:grid-cols-[1.25rem_1.5rem_4.5rem_1.5rem_1fr_auto_1.75rem_4.5rem_2rem]`
      : isSolo
        ? `grid-cols-[2rem_2rem_1fr_auto] md:grid-cols-[1.25rem_1.5rem_4.5rem_1.5rem_1fr_auto_4.5rem]`
        : `grid-cols-[2rem_2rem_1fr_auto] md:grid-cols-[1.25rem_1.5rem_4.5rem_1.5rem_1fr_auto_1.75rem_4.5rem]`
    : renderRowAction
      ? isSolo
        ? `grid-cols-[2rem_2rem_1fr_auto_2rem] md:grid-cols-[1.5rem_4.5rem_1.5rem_1fr_auto_4.5rem_2rem]`
        : `grid-cols-[2rem_2rem_1fr_auto_2rem] md:grid-cols-[1.5rem_4.5rem_1.5rem_1fr_auto_1.75rem_4.5rem_2rem]`
      : isSolo
        ? `grid-cols-[2rem_2rem_1fr_auto] md:grid-cols-[1.5rem_4.5rem_1.5rem_1fr_auto_4.5rem]`
        : `grid-cols-[2rem_2rem_1fr_auto] md:grid-cols-[1.5rem_4.5rem_1.5rem_1fr_auto_1.75rem_4.5rem]`

  const toggleGroup = (groupKey: string) => {
    setCollapsedGroups((prev) => {
      const next = new Set(prev)
      if (next.has(groupKey)) {
        next.delete(groupKey)
      } else {
        next.add(groupKey)
      }
      return next
    })
  }

  if (visibleGroups.length === 0) {
    if (isLoading) {
      return <IssueListSkeleton />
    }

    if (hasAnyIssues && hasActiveFilters) {
      return (
        <EmptyState
          icon={SearchX}
          title="No issues match your filters"
          description="Try removing some filters to see more issues."
        >
          {onClearFilters && (
            <Button size="sm" variant="outline" onClick={onClearFilters}>
              Clear filters
            </Button>
          )}
        </EmptyState>
      )
    }

    return (
      <div>
        <EmptyState
          icon={ListTodo}
          title="No issues yet"
          description="Create an issue to start tracking work."
        >
          {canCreate && (
            <Button size="sm" onClick={() => onNewIssue()}>
              <Plus className="mr-1.5 size-4" />
              New issue
            </Button>
          )}
        </EmptyState>
        {emptyStateExtra}
      </div>
    )
  }

  return (
    <div ref={listRef}>
      {visibleGroups.map((group) => {
        const option = group.status
        const Icon = ICON_COMPONENTS[option.icon]
        const isOpen = !collapsedGroups.has(option.id)
        const wash = groupHeaderWash(option)
        const limit = renderLimit(option.id)
        const renderedIssues =
          group.issues.length > limit ? group.issues.slice(0, limit) : group.issues
        const hiddenCount = group.issues.length - renderedIssues.length
        return (
          <CollapsiblePrimitive.Root
            key={option.id}
            open={isOpen}
            onOpenChange={() => toggleGroup(option.id)}
            data-testid={`issue-group-${option.id}`}
            // Stable across custom-status renames/ids: the builtin anchor key
            // for builtin groups, the row id otherwise. E2E selects on this.
            data-status-key={option.builtinKey ?? option.id}
          >
            {/* Group header */}
            {/* backdrop-blur is load-bearing: the tint is translucent and
                rows scroll under the sticky header. */}
            <div
              className={`group sticky top-0 z-10 flex items-center justify-between pl-3 pr-3 md:pr-6 py-1.5 border-b border-border/40 backdrop-blur-md ${wash.className}`}
              style={wash.style}
            >
              <div className="flex items-center gap-1.5">
                <CollapsiblePrimitive.Trigger asChild>
                  <Button
                    variant="ghost"
                    className="h-8 w-8 md:h-5 md:w-5 p-0 text-muted-foreground"
                  >
                    <ChevronRight
                      className={`size-3 transition-transform duration-fast ease-standard motion-reduce:transition-none ${isOpen ? `rotate-90` : ``}`}
                    />
                  </Button>
                </CollapsiblePrimitive.Trigger>
                <Icon
                  className={`h-3.5 w-3.5 ${statusColorClass(option)}`}
                  style={statusColorStyle(option)}
                />
                <span className="text-sm font-medium">{option.name}</span>
                <span className="text-xs text-muted-foreground">
                  {group.issues.length}
                </span>
              </div>
              {canCreate && (
                <Button
                  variant="ghost"
                  size="icon-xs"
                  className="hidden md:inline-flex text-muted-foreground opacity-0 group-hover:opacity-100 hover:opacity-100"
                  onClick={(e) => {
                    e.stopPropagation()
                    // A new issue can never be born a duplicate (no canonical
                    // issue to pair with) — the duplicate group's "+" seeds
                    // nothing and the dialog falls back to Backlog.
                    onNewIssue(
                      option.category === `duplicate` ? undefined : option
                    )
                  }}
                >
                  <Plus className="size-3" />
                </Button>
              )}
            </div>

            {/* Issue rows */}
            <CollapsiblePrimitive.Content className="overflow-hidden data-[state=closed]:animate-collapsible-up data-[state=open]:animate-collapsible-down">
              {renderedIssues.map((issue) => {
                const rowCanMutate = canMutateIssue
                  ? canMutateIssue(issue)
                  : true
                return (
                  <IssueRow
                    key={issue.id}
                    issue={issue}
                    issueLabels={issueLabelMap.get(issue.id) ?? NO_LABELS}
                    labels={labels}
                    users={users}
                    userMap={userMap}
                    teamBoards={teamBoards}
                    rowGridClass={rowGridClass}
                    today={today}
                    isSolo={isSolo}
                    bulkEnabled={bulkEnabled}
                    mobileSelectionActive={mobileSelectionActive}
                    isSelected={selectedIds.has(issue.id)}
                    anySelected={selectedIds.size > 0}
                    canMutateRow={rowCanMutate && canModerate}
                    onOpen={openIssue}
                    onToggleSelect={toggleSelect}
                    renderRowAction={stableRenderRowAction}
                  />
                )
              })}
              {hiddenCount > 0 && (
                <Button
                  variant="ghost"
                  size="sm"
                  className="w-full rounded-none border-b border-border/30 text-xs text-muted-foreground"
                  onClick={() =>
                    setExtraRows((prev) =>
                      new Map(prev).set(
                        option.id,
                        (prev.get(option.id) ?? 0) + GROUP_ROW_CHUNK
                      )
                    )
                  }
                >
                  Show {Math.min(GROUP_ROW_CHUNK, hiddenCount)} more (
                  {hiddenCount} hidden)
                </Button>
              )}
            </CollapsiblePrimitive.Content>
          </CollapsiblePrimitive.Root>
        )
      })}
    </div>
  )
}
