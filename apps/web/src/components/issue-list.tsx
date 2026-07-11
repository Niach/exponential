import { useEffect, useMemo, useRef, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { Issue, Label, Release, User } from "@/db/schema"
import { releaseCollection } from "@/lib/collections"
import { compareReleases } from "@/lib/releases"
import { StatusDropdown, getStatusConfig } from "@/components/issue-properties/status-dropdown"
import { PriorityDropdown } from "@/components/issue-properties/priority-dropdown"
import { AssigneeDropdown } from "@/components/issue-properties/assignee-dropdown"
import { DueDateDropdown } from "@/components/issue-properties/due-date-dropdown"
import { IssueRowContextMenu } from "@/components/issue-row-menu/context-menu"
import { BulkActionBar } from "@/components/bulk-action-bar"
import { EmptyState } from "@/components/empty-state"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Skeleton } from "@/components/ui/skeleton"
import { Collapsible as CollapsiblePrimitive } from "radix-ui"
import { Plus, ChevronRight, ListTodo, Repeat, SearchX } from "lucide-react"
import type { IssueStatus } from "@/lib/domain"
import type { IssueGroup } from "@/lib/project-board"

const statusHeaderBg: Record<IssueStatus, string> = {
  backlog: `rgba(113, 113, 122, 0.08)`,
  todo: `rgba(212, 212, 216, 0.08)`,
  in_progress: `rgba(234, 179, 8, 0.10)`,
  done: `rgba(34, 197, 94, 0.10)`,
  cancelled: `rgba(113, 113, 122, 0.08)`,
  duplicate: `rgba(113, 113, 122, 0.08)`,
}

interface IssueListProps {
  groups: IssueGroup[]
  issueLabelMap: Map<string, Label[]>
  labels: Label[]
  users: User[]
  userMap: Map<string, User>
  onNewIssue: (status?: IssueStatus) => void
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
  // Distinguish "the project has no issues" from "filters hide everything".
  hasAnyIssues?: boolean
  hasActiveFilters?: boolean
  onClearFilters?: () => void
  // Optional trailing per-row action cell (e.g. the release detail's
  // remove-from-release X). Rendered in its own click-isolated grid column.
  renderRowAction?: (issue: Issue) => React.ReactNode
  // Enables bulk selection + the floating action bar (hover checkboxes on
  // md+, shift-click ranges, Cmd/Ctrl+A, Esc) and the context menu's
  // add-to-release submenu. The workspace scope feeds the release queries.
  // Undefined = bulk select off. Selection also requires canModerate.
  bulkWorkspaceId?: string
}

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
  renderRowAction,
  bulkWorkspaceId,
}: IssueListProps) {
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set())
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  const [anchorId, setAnchorId] = useState<string | null>(null)
  const listRef = useRef<HTMLDivElement>(null)
  const visibleGroups = groups.filter((g) => g.issues.length > 0)
  const bulkEnabled = Boolean(bulkWorkspaceId) && canModerate

  // Workspace releases feed the context menu's add-to-release submenu (kept
  // out of the per-row menu component so its tests stay collection-free).
  const { data: releaseRows } = useLiveQuery(
    (query) =>
      bulkWorkspaceId
        ? query
            .from({ releases: releaseCollection })
            .where(({ releases }) => eq(releases.workspaceId, bulkWorkspaceId))
        : undefined,
    [bulkWorkspaceId]
  )
  const workspaceReleases = useMemo(
    () =>
      bulkWorkspaceId
        ? [...((releaseRows ?? []) as Release[])].sort(compareReleases)
        : undefined,
    [releaseRows, bulkWorkspaceId]
  )

  // The range/select-all universe: filtered rows in render order, minus
  // collapsed groups.
  const visibleFlatIssues = useMemo(
    () =>
      groups
        .filter(
          (group) =>
            group.issues.length > 0 && !collapsedGroups.has(group.status)
        )
        .flatMap((group) => group.issues),
    [groups, collapsedGroups]
  )

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

  const clearSelection = () => {
    setSelectedIds(new Set())
    setAnchorId(null)
  }

  const toggleSelect = (issueId: string, shiftKey: boolean) => {
    const ids = visibleFlatIssues.map((issue) => issue.id)
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
    setAnchorId(issueId)
  }

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

  const selectedIssues = useMemo(
    () =>
      groups
        .flatMap((group) => group.issues)
        .filter((issue) => selectedIds.has(issue.id)),
    [groups, selectedIds]
  )

  // The row grid grows a leading checkbox column (md+ when bulk select is on)
  // and a trailing action column when the caller renders one.
  const rowGridClass = bulkEnabled
    ? renderRowAction
      ? `grid-cols-[2rem_2rem_1fr_auto_2rem] md:grid-cols-[1.25rem_1.5rem_4.5rem_1.5rem_1fr_auto_1.75rem_4.5rem_2rem]`
      : `grid-cols-[2rem_2rem_1fr_auto] md:grid-cols-[1.25rem_1.5rem_4.5rem_1.5rem_1fr_auto_1.75rem_4.5rem]`
    : renderRowAction
      ? `grid-cols-[2rem_2rem_1fr_auto_2rem] md:grid-cols-[1.5rem_4.5rem_1.5rem_1fr_auto_1.75rem_4.5rem_2rem]`
      : `grid-cols-[2rem_2rem_1fr_auto] md:grid-cols-[1.5rem_4.5rem_1.5rem_1fr_auto_1.75rem_4.5rem]`

  // Solo workspace (exactly one human member): render the assignee cell as a
  // static avatar, not an interactive dropdown. `users` is the bot-excluded
  // member list; length 0 means still loading (never a genuine empty), so a
  // multi-member workspace never briefly reads as solo.
  const isSolo = users.length === 1

  const toggleGroup = (status: IssueStatus) => {
    setCollapsedGroups((prev) => {
      const next = new Set(prev)
      if (next.has(status)) {
        next.delete(status)
      } else {
        next.add(status)
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
    )
  }

  return (
    <div ref={listRef}>
      {visibleGroups.map((group) => {
        const config = getStatusConfig(group.status)
        const Icon = config.icon
        const isOpen = !collapsedGroups.has(group.status)
        const headerBg =
          statusHeaderBg[group.status] ?? `rgba(113, 113, 122, 0.08)`
        return (
          <CollapsiblePrimitive.Root
            key={group.status}
            open={isOpen}
            onOpenChange={() => toggleGroup(group.status)}
            data-testid={`issue-group-${group.status}`}
          >
            {/* Group header */}
            <div
              className="group sticky top-0 z-10 flex items-center justify-between pl-3 pr-3 md:pr-6 py-1.5 border-b border-border/50"
              style={{ backgroundColor: headerBg }}
            >
              <div className="flex items-center gap-1.5">
                <CollapsiblePrimitive.Trigger asChild>
                  <Button
                    variant="ghost"
                    className="h-8 w-8 md:h-5 md:w-5 p-0 text-muted-foreground"
                  >
                    <ChevronRight
                      className={`size-3 transition-transform duration-200 ${isOpen ? `rotate-90` : ``}`}
                    />
                  </Button>
                </CollapsiblePrimitive.Trigger>
                <Icon className={`h-3.5 w-3.5 ${config.color}`} />
                <span className="text-sm font-medium">{config.label}</span>
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
                    onNewIssue(group.status)
                  }}
                >
                  <Plus className="size-3" />
                </Button>
              )}
            </div>

            {/* Issue rows */}
            <CollapsiblePrimitive.Content className="overflow-hidden data-[state=closed]:animate-collapsible-up data-[state=open]:animate-collapsible-down">
              {group.issues.map((issue) => {
                const issueLabels = issueLabelMap.get(issue.id) ?? []
                const rowCanMutate = canMutateIssue
                  ? canMutateIssue(issue)
                  : true
                const moderatorRowCanMutate = rowCanMutate && canModerate
                return (
                  <IssueRowContextMenu
                    key={issue.id}
                    issue={issue}
                    issueLabels={issueLabels}
                    labels={labels}
                    users={users}
                    userMap={userMap}
                    releases={workspaceReleases}
                    onOpenIssue={() => onIssueClick(issue)}
                  >
                    <div
                      className={`grid ${rowGridClass} items-center h-12 md:h-10 px-3 md:px-6 hover:bg-accent/30 border-b border-border/30 group/row cursor-pointer`}
                      onClick={() => onIssueClick(issue)}
                      data-testid={`issue-row-${issue.identifier}`}
                    >
                      {bulkEnabled && (
                        <div
                          className="hidden md:flex items-center"
                          // Suppress the browser's shift-click text selection
                          // so range-select doesn't highlight row text.
                          onMouseDown={(e) => {
                            if (e.shiftKey) e.preventDefault()
                          }}
                          onClick={(e) => {
                            e.stopPropagation()
                            toggleSelect(issue.id, e.shiftKey)
                          }}
                        >
                          <Checkbox
                            checked={selectedIds.has(issue.id)}
                            aria-label={`Select ${issue.identifier}`}
                            className={`transition-opacity ${selectedIds.size > 0 ? `opacity-100` : `opacity-0 group-hover/row:opacity-100`}`}
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
                          disabled={!moderatorRowCanMutate}
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
                          disabled={!moderatorRowCanMutate}
                        />
                      </div>
                      <span className="flex items-center gap-1.5 text-sm truncate ml-2 min-w-0">
                        {issue.recurrenceInterval !== null && (
                          <Repeat
                            className="size-3 shrink-0 text-muted-foreground"
                            aria-label="Recurring"
                          />
                        )}
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
                      <div
                        className="hidden md:flex items-center justify-center"
                        onClick={(e) => e.stopPropagation()}
                      >
                        <AssigneeDropdown
                          issueId={issue.id}
                          assigneeId={issue.assigneeId}
                          users={users}
                          userMap={userMap}
                          disabled={!moderatorRowCanMutate}
                          readOnly={isSolo}
                        />
                      </div>
                      <div
                        className="flex items-center justify-end"
                        onClick={(e) => e.stopPropagation()}
                      >
                        <DueDateDropdown
                          issueId={issue.id}
                          dueDate={issue.dueDate}
                          disabled={!moderatorRowCanMutate}
                        />
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
              })}
            </CollapsiblePrimitive.Content>
          </CollapsiblePrimitive.Root>
        )
      })}

      {bulkEnabled && bulkWorkspaceId && selectedIssues.length > 0 && (
        <BulkActionBar
          workspaceId={bulkWorkspaceId}
          issues={selectedIssues}
          issueLabelMap={issueLabelMap}
          labels={labels}
          users={users}
          onClear={clearSelection}
        />
      )}
    </div>
  )
}
