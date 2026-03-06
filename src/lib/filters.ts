import type { Issue } from "@/db/schema"
import type { IssuePriority, IssueStatus } from "@/lib/domain"

export interface IssueFilters {
  statuses: IssueStatus[]
  priorities: IssuePriority[]
  labelIds: string[]
}

export const emptyFilters: IssueFilters = {
  statuses: [],
  priorities: [],
  labelIds: [],
}

export type TabPreset = `all` | `active` | `backlog`

export const tabPresetStatuses: Record<TabPreset, IssueStatus[]> = {
  all: [],
  active: [`in_progress`, `todo`],
  backlog: [`backlog`],
}

export function deriveActiveTab(statuses: IssueStatus[]): TabPreset {
  if (statuses.length === 0) return `all`
  const sorted = [...statuses].sort()
  const activeSorted = [...tabPresetStatuses.active].sort()
  const backlogSorted = [...tabPresetStatuses.backlog].sort()
  if (
    sorted.length === activeSorted.length &&
    sorted.every((s, i) => s === activeSorted[i])
  )
    return `active`
  if (
    sorted.length === backlogSorted.length &&
    sorted.every((s, i) => s === backlogSorted[i])
  )
    return `backlog`
  return `all`
}

export function matchesFilters(
  issue: Issue,
  issueLabelIds: string[],
  filters: IssueFilters
): boolean {
  if (filters.statuses.length > 0 && !filters.statuses.includes(issue.status))
    return false
  if (
    filters.priorities.length > 0 &&
    !filters.priorities.includes(issue.priority)
  )
    return false
  if (
    filters.labelIds.length > 0 &&
    !filters.labelIds.some((id) => issueLabelIds.includes(id))
  )
    return false
  return true
}

export function activeFilterCount(filters: IssueFilters): number {
  return (
    filters.statuses.length +
    filters.priorities.length +
    filters.labelIds.length
  )
}

export function hasActiveFilters(filters: IssueFilters): boolean {
  return activeFilterCount(filters) > 0
}
