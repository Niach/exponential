import { Button } from "@/components/ui/button"
import { Plus } from "lucide-react"
import { IssueFilterPopover } from "@/components/issue-filter-popover"
import { ActiveFilterPills } from "@/components/active-filter-pills"
import type { IssueFilters } from "@/lib/filters"
import { deriveActiveTab, tabPresetStatuses } from "@/lib/filters"
import type { TabPreset } from "@/lib/filters"
import type { Label } from "@/db/schema"

const tabs: { id: TabPreset; label: string }[] = [
  { id: `all`, label: `All Issues` },
  { id: `active`, label: `Active` },
  { id: `backlog`, label: `Backlog` },
]

interface IssueFilterBarProps {
  filters: IssueFilters
  onFiltersChange: (filters: IssueFilters) => void
  labels: Label[]
  onNewIssue: () => void
}

export function IssueFilterBar({
  filters,
  onFiltersChange,
  labels,
  onNewIssue,
}: IssueFilterBarProps) {
  const activeTab = deriveActiveTab(filters.statuses)

  const handleTabClick = (tabId: TabPreset) => {
    onFiltersChange({ ...filters, statuses: tabPresetStatuses[tabId] })
  }

  return (
    <div className="px-3 md:px-6">
      <div className="flex items-center justify-between py-3">
        <h1 className="hidden md:block text-base font-medium">Issues</h1>
        <div className="flex items-center gap-1 ml-auto">
          <IssueFilterPopover
            filters={filters}
            onFiltersChange={onFiltersChange}
            labels={labels}
          />
          <Button
            size="xs"
            className="bg-indigo-600 hover:bg-indigo-700 text-white ml-1"
            onClick={onNewIssue}
          >
            <Plus className="size-3" />
            <span className="hidden md:inline">New Issue</span>
          </Button>
        </div>
      </div>
      <div className="flex items-center gap-1 -mx-3 px-3 md:mx-0 md:px-0 overflow-x-auto">
        {tabs.map((tab) => (
          <Button
            key={tab.id}
            variant="ghost"
            size="sm"
            onClick={() => handleTabClick(tab.id)}
            className={`shrink-0 rounded-full h-7 px-3 text-xs ${
              activeTab === tab.id
                ? `bg-accent text-foreground font-medium`
                : `text-muted-foreground hover:text-foreground`
            }`}
          >
            {tab.label}
          </Button>
        ))}
      </div>
      <ActiveFilterPills
        filters={filters}
        onFiltersChange={onFiltersChange}
        labels={labels}
      />
    </div>
  )
}
