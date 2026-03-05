import { useState } from "react"
import type { Issue, Label } from "@/db/schema"
import { StatusDropdown, getStatusConfig } from "@/components/status-dropdown"
import { PriorityDropdown } from "@/components/priority-dropdown"
import { Button } from "@/components/ui/button"
import { Collapsible as CollapsiblePrimitive } from "radix-ui"
import { Plus, CalendarDays, ChevronRight } from "lucide-react"
import { formatDate } from "@/lib/utils"

interface IssueGroup {
  status: string
  issues: Issue[]
}

interface IssueListProps {
  groups: IssueGroup[]
  issueLabelMap: Map<string, Label[]>
  onNewIssue: (status?: string) => void
  onIssueClick: (issue: Issue) => void
}

export function IssueList({
  groups,
  issueLabelMap,
  onNewIssue,
  onIssueClick,
}: IssueListProps) {
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(
    new Set()
  )
  const visibleGroups = groups.filter((g) => g.issues.length > 0)

  const toggleGroup = (status: string) => {
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
    return (
      <div className="flex flex-col items-center justify-center py-16 text-muted-foreground">
        <p className="text-sm">No issues yet</p>
        <p className="text-xs mt-1">Create your first issue to get started</p>
      </div>
    )
  }

  return (
    <div>
      {visibleGroups.map((group) => {
        const config = getStatusConfig(group.status)
        const Icon = config.icon
        const isOpen = !collapsedGroups.has(group.status)
        return (
          <CollapsiblePrimitive.Root
            key={group.status}
            open={isOpen}
            onOpenChange={() => toggleGroup(group.status)}
          >
            {/* Group header */}
            <div className="group sticky top-0 z-10 flex items-center justify-between bg-background px-6 py-1.5 border-b border-border/50">
              <div className="flex items-center gap-1.5">
                <CollapsiblePrimitive.Trigger asChild>
                  <Button
                    variant="ghost"
                    className="h-5 w-5 p-0 text-muted-foreground"
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
              <Button
                variant="ghost"
                size="icon-xs"
                className="text-muted-foreground opacity-0 group-hover:opacity-100 hover:opacity-100"
                onClick={(e) => {
                  e.stopPropagation()
                  onNewIssue(group.status)
                }}
              >
                <Plus className="size-3" />
              </Button>
            </div>

            {/* Issue rows */}
            <CollapsiblePrimitive.Content className="overflow-hidden data-[state=closed]:animate-collapsible-up data-[state=open]:animate-collapsible-down">
              {group.issues.map((issue) => {
                const issueLabels = issueLabelMap.get(issue.id) ?? []
                return (
                  <div
                    key={issue.id}
                    className="grid grid-cols-[24px_72px_24px_1fr_auto] items-center h-[34px] px-6 hover:bg-accent/30 border-b border-border/30 group/row cursor-pointer"
                    onClick={() => onIssueClick(issue)}
                  >
                    <div
                      className="flex items-center justify-center"
                      onClick={(e) => e.stopPropagation()}
                    >
                      <PriorityDropdown
                        issueId={issue.id}
                        priority={issue.priority}
                      />
                    </div>
                    <span className="text-xs text-muted-foreground font-mono truncate">
                      {issue.identifier}
                    </span>
                    <div
                      className="flex items-center justify-center"
                      onClick={(e) => e.stopPropagation()}
                    >
                      <StatusDropdown
                        issueId={issue.id}
                        status={issue.status}
                      />
                    </div>
                    <span className="text-sm truncate ml-2">
                      {issue.title}
                    </span>
                    <div className="flex items-center gap-1.5 ml-4 shrink-0">
                      {issueLabels.map((label) => (
                        <span
                          key={label.id}
                          className="flex items-center gap-1 border border-border/50 rounded-full px-1.5 py-px text-[11px] text-muted-foreground"
                        >
                          <div
                            className="h-1.5 w-1.5 rounded-full shrink-0"
                            style={{ backgroundColor: label.color }}
                          />
                          {label.name}
                        </span>
                      ))}
                      {issue.dueDate && (
                        <span className="flex items-center gap-1 text-xs text-muted-foreground ml-1">
                          <CalendarDays className="size-3" />
                          {formatDate(issue.dueDate)}
                        </span>
                      )}
                    </div>
                  </div>
                )
              })}
            </CollapsiblePrimitive.Content>
          </CollapsiblePrimitive.Root>
        )
      })}
    </div>
  )
}
