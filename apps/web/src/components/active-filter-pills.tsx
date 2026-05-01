import { Button } from "@/components/ui/button"
import { X } from "lucide-react"
import { StatusIcon, getStatusConfig } from "@/components/status-dropdown"
import { PriorityIcon, getPriorityConfig } from "@/components/priority-dropdown"
import type { IssueFilters } from "@/lib/filters"
import { hasActiveFilters } from "@/lib/filters"
import { emptyFilters } from "@/lib/filters"
import type { Label } from "@/db/schema"

interface ActiveFilterPillsProps {
  filters: IssueFilters
  onFiltersChange: (filters: IssueFilters) => void
  labels: Label[]
}

export function ActiveFilterPills({
  filters,
  onFiltersChange,
  labels,
}: ActiveFilterPillsProps) {
  if (!hasActiveFilters(filters)) return null

  const labelMap = new Map(labels.map((l) => [l.id, l]))

  const removeStatus = (value: string) =>
    onFiltersChange({
      ...filters,
      statuses: filters.statuses.filter((s) => s !== value),
    })

  const removePriority = (value: string) =>
    onFiltersChange({
      ...filters,
      priorities: filters.priorities.filter((p) => p !== value),
    })

  const removeLabel = (id: string) =>
    onFiltersChange({
      ...filters,
      labelIds: filters.labelIds.filter((l) => l !== id),
    })

  return (
    <div className="flex items-center gap-1.5 px-6 py-1.5 flex-wrap">
      {filters.statuses.map((status) => {
        const config = getStatusConfig(status)
        return (
          <Button
            key={`s-${status}`}
            variant="outline"
            size="xs"
            className="h-6 gap-1 rounded-full text-xs"
            onClick={() => removeStatus(status)}
          >
            <StatusIcon status={status} className="!h-3 !w-3" />
            {config.label}
            <X className="size-2.5 ml-0.5" />
          </Button>
        )
      })}
      {filters.priorities.map((priority) => {
        const config = getPriorityConfig(priority)
        return (
          <Button
            key={`p-${priority}`}
            variant="outline"
            size="xs"
            className="h-6 gap-1 rounded-full text-xs"
            onClick={() => removePriority(priority)}
          >
            <PriorityIcon priority={priority} className="!h-3 !w-3" />
            {config.label}
            <X className="size-2.5 ml-0.5" />
          </Button>
        )
      })}
      {filters.labelIds.map((id) => {
        const label = labelMap.get(id)
        if (!label) return null
        return (
          <Button
            key={`l-${id}`}
            variant="outline"
            size="xs"
            className="h-6 gap-1 rounded-full text-xs"
            onClick={() => removeLabel(id)}
          >
            <div
              className="h-2 w-2 rounded-full shrink-0"
              style={{ backgroundColor: label.color }}
            />
            {label.name}
            <X className="size-2.5 ml-0.5" />
          </Button>
        )
      })}
      <Button
        variant="ghost"
        size="xs"
        className="h-6 text-xs text-muted-foreground"
        onClick={() => onFiltersChange(emptyFilters)}
      >
        Clear all
      </Button>
    </div>
  )
}
