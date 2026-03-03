import type { Issue, Label } from "@/db/schema"
import { StatusDropdown, getStatusConfig } from "@/components/status-dropdown"
import { PriorityDropdown } from "@/components/priority-dropdown"
import { Button } from "@/components/ui/button"
import { Plus, CalendarDays } from "lucide-react"

function formatDate(date: Date | string): string {
  const d = typeof date === `string` ? new Date(date) : date
  return d.toLocaleDateString(`en-US`, { month: `short`, day: `numeric` })
}

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

export function IssueList({ groups, issueLabelMap, onNewIssue, onIssueClick }: IssueListProps) {
  const visibleGroups = groups.filter((g) => g.issues.length > 0)

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
        return (
          <div key={group.status}>
            {/* Group header */}
            <div className="group sticky top-0 z-10 flex items-center justify-between bg-background px-6 py-1.5 border-b border-border/50">
              <div className="flex items-center gap-2">
                <Icon className={`h-3.5 w-3.5 ${config.color}`} />
                <span className="text-sm font-medium">{config.label}</span>
                <span className="text-xs text-muted-foreground">{group.issues.length}</span>
              </div>
              <Button
                variant="ghost"
                size="icon-xs"
                className="text-muted-foreground opacity-0 group-hover:opacity-100 hover:opacity-100"
                onClick={() => onNewIssue(group.status)}
              >
                <Plus className="size-3" />
              </Button>
            </div>

            {/* Issue rows */}
            {group.issues.map((issue) => {
              const issueLabels = issueLabelMap.get(issue.id) ?? []
              return (
                <div
                  key={issue.id}
                  className="grid grid-cols-[24px_72px_24px_1fr_auto] items-center h-[34px] px-6 hover:bg-accent/30 border-b border-border/30 group/row cursor-pointer"
                  onClick={() => onIssueClick(issue)}
                >
                  <div className="flex items-center justify-center" onClick={(e) => e.stopPropagation()}>
                    <PriorityDropdown issueId={issue.id} priority={issue.priority} />
                  </div>
                  <span className="text-xs text-muted-foreground font-mono truncate">
                    {issue.identifier}
                  </span>
                  <div className="flex items-center justify-center" onClick={(e) => e.stopPropagation()}>
                    <StatusDropdown issueId={issue.id} status={issue.status} />
                  </div>
                  <span className="text-sm truncate ml-2">{issue.title}</span>
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
          </div>
        )
      })}
    </div>
  )
}
