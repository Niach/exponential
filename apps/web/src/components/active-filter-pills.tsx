import { Button } from "@/components/ui/button"
import { CircleQuestionMark, X } from "lucide-react"
import { StatusIcon } from "@/components/issue-properties/status-dropdown"
import {
  PriorityIcon,
  getPriorityConfig,
} from "@/components/issue-properties/priority-dropdown"
import type { IssueFilters } from "@/lib/filters"
import { hasActiveFilters } from "@/lib/filters"
import { emptyFilters } from "@/lib/filters"
import { issuePriorityOptions } from "@/lib/domain"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import type { StatusRowOption } from "@/lib/team-statuses"
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
  const { options: statusOptions } = useTeamStatusesContext()

  if (!hasActiveFilters(filters)) return null

  const labelMap = new Map(labels.map((l) => [l.id, l]))

  // Pills read in the shared option order — the team's `teamStatuses` sequence
  // for statuses (REV2-85: the same order the board groups use), the contract
  // displayOrder for priorities — not the order values happened to be ticked.
  // A token that resolves to no row (a status deleted while the URL was open)
  // still renders a removable "Unknown status" pill so the filter is escapable.
  // A URL can carry BOTH forms of the same status (legacy enum + row uuid) —
  // fold every equivalent token into ONE pill whose ✕ strips them all.
  const statusPills: { tokens: string[]; option: StatusRowOption | null }[] = []
  for (const option of statusOptions) {
    const tokens = filters.statusTokens.filter(
      (candidate) => candidate === option.id || candidate === option.builtinKey
    )
    if (tokens.length > 0) statusPills.push({ tokens, option })
  }
  const matchedTokens = new Set(statusPills.flatMap((pill) => pill.tokens))
  for (const token of filters.statusTokens) {
    if (!matchedTokens.has(token))
      statusPills.push({ tokens: [token], option: null })
  }

  const priorities = issuePriorityOptions
    .map((option) => option.value)
    .filter((value) => filters.priorities.includes(value))

  const removeStatusTokens = (tokens: string[]) =>
    onFiltersChange({
      ...filters,
      statusTokens: filters.statusTokens.filter((s) => !tokens.includes(s)),
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
      {statusPills.map(({ tokens, option }) => (
        <Button
          key={`s-${tokens[0]}`}
          variant="outline"
          size="xs"
          onClick={() => removeStatusTokens(tokens)}
        >
          {option ? (
            <StatusIcon option={option} className="!h-3 !w-3" />
          ) : (
            <CircleQuestionMark className="!h-3 !w-3 text-muted-foreground" />
          )}
          {option ? option.name : `Unknown status`}
          <X className="size-2.5 ml-0.5" />
        </Button>
      ))}
      {priorities.map((priority) => {
        const config = getPriorityConfig(priority)
        return (
          <Button
            key={`p-${priority}`}
            variant="outline"
            size="xs"
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
