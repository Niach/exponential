import { IssueFilterPopover } from "@/components/issue-filter-popover"
import { ActiveFilterPills } from "@/components/active-filter-pills"
import type { IssueFilters } from "@/lib/filters"
import type { Label } from "@/db/schema"

// EXP-449: title-less control row — the page name lives in the sidebar/topbar
// and the New-issue button moved into the sidebar header, so this bar is just
// the right-aligned actions + filter trigger over the pills.
interface IssueFilterBarProps {
  filters: IssueFilters
  onFiltersChange: (filters: IssueFilters) => void
  labels: Label[]
  // Extra header actions rendered before the filter button.
  actions?: React.ReactNode
}

export function IssueFilterBar({
  filters,
  onFiltersChange,
  labels,
  actions,
}: IssueFilterBarProps) {
  return (
    <div className="px-4 md:px-6">
      {/* Fixed height (= the old py-3 + h-8 rendering) so hosting the bulk
          action bar in this row never reflows the list below (FEED-12). */}
      <div className="flex h-14 items-center justify-between">
        <div className="flex items-center gap-1 ml-auto">
          {actions}
          <IssueFilterPopover
            filters={filters}
            onFiltersChange={onFiltersChange}
            labels={labels}
          />
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
