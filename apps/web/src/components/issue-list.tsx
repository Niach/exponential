import { useState } from "react"
import type { Issue, Label, User } from "@/db/schema"
import { StatusDropdown, getStatusConfig } from "@/components/issue-properties/status-dropdown"
import { PriorityDropdown } from "@/components/issue-properties/priority-dropdown"
import { AssigneeDropdown } from "@/components/issue-properties/assignee-dropdown"
import { DueDateDropdown } from "@/components/issue-properties/due-date-dropdown"
import { IssueRowContextMenu } from "@/components/issue-row-menu/context-menu"
import { EmptyState } from "@/components/empty-state"
import { Button } from "@/components/ui/button"
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
}: IssueListProps) {
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set())
  const visibleGroups = groups.filter((g) => g.issues.length > 0)

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
        description="Create an issue to track work — then assign it to a coding agent to have it open a pull request."
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
    <div>
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
                    onOpenIssue={() => onIssueClick(issue)}
                  >
                    <div
                      className="grid grid-cols-[2rem_2rem_1fr_auto] md:grid-cols-[1.5rem_4.5rem_1.5rem_1fr_auto_1.75rem_4.5rem] items-center h-12 md:h-10 px-3 md:px-6 hover:bg-accent/30 border-b border-border/30 group/row cursor-pointer"
                      onClick={() => onIssueClick(issue)}
                      data-testid={`issue-row-${issue.identifier}`}
                    >
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
                    </div>
                  </IssueRowContextMenu>
                )
              })}
            </CollapsiblePrimitive.Content>
          </CollapsiblePrimitive.Root>
        )
      })}
    </div>
  )
}
