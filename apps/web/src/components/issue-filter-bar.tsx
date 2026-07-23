import { Button } from "@/components/ui/button"
import { Plus } from "lucide-react"
import { IssueFilterPopover } from "@/components/issue-filter-popover"
import { ActiveFilterPills } from "@/components/active-filter-pills"
import type { IssueFilters } from "@/lib/filters"
import type { Label } from "@/db/schema"

interface IssueFilterBarProps {
  filters: IssueFilters
  onFiltersChange: (filters: IssueFilters) => void
  labels: Label[]
  onNewIssue: () => void
  canCreate?: boolean
  title?: string
  // Extra header actions rendered before the filter button.
  actions?: React.ReactNode
}

export function IssueFilterBar({
  filters,
  onFiltersChange,
  labels,
  onNewIssue,
  canCreate = true,
  title = `Issues`,
  actions,
}: IssueFilterBarProps) {
  return (
    <div className="px-4 md:px-6">
      <div className="flex items-center justify-between py-3">
        {title !== `` && (
          <h1 className="hidden md:block text-base font-medium">{title}</h1>
        )}
        <div className="flex items-center gap-1 ml-auto">
          {actions}
          <IssueFilterPopover
            filters={filters}
            onFiltersChange={onFiltersChange}
            labels={labels}
          />
          {canCreate && (
            <Button
              size="xs"
              className="hidden md:inline-flex bg-indigo-600 hover:bg-indigo-700 text-white ml-1"
              onClick={onNewIssue}
            >
              <Plus className="size-3" />
              New Issue
            </Button>
          )}
        </div>
      </div>
      <ActiveFilterPills
        filters={filters}
        onFiltersChange={onFiltersChange}
        labels={labels}
      />
    </div>
  )
}
