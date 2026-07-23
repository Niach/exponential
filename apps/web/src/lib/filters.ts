// The IssueFilters shape and matchesFilters() are mirrored across three
// clients. If you change the filter shape or matching semantics here, also
// update apps/ios/Exponential/Domain/IssueFilters.swift and
// apps/android/app/src/main/java/com/exponential/app/domain/IssueFilters.kt
// to keep the three clients in sync (no shared package yet).
import type { Issue } from "@/db/schema"
import type { IssuePriority, IssueStatus } from "@/lib/domain"
import { issuePriorityOptions, issueStatusOptions } from "@/lib/domain"

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

// --- URL search-param form (web-only, not mirrored on native) ---------------
// Filters live in the URL as clean comma-joined values (?status=todo,in_progress
// &priority=high&labels=<id>,<id>) so a filtered board is shareable and survives
// a refresh. The issue-detail route accepts the SAME optional params so the
// prev/next switcher can follow the board's filtered ordering; helpers are
// shared here so the two routes can't drift.

export interface IssueFilterSearch {
  status?: string
  priority?: string
  labels?: string
}

const STATUS_VALUES = issueStatusOptions.map((o) => o.value)
const PRIORITY_VALUES = issuePriorityOptions.map((o) => o.value)

// Coerce a raw search value (array or comma string) to a validated, comma-joined
// string, or undefined when empty — so cleared filters drop out of the URL.
function validatedCsv(
  raw: unknown,
  allowed?: readonly string[]
): string | undefined {
  let arr: string[]
  if (Array.isArray(raw)) {
    arr = raw.filter((v): v is string => typeof v === `string`)
  } else if (typeof raw === `string` && raw.length > 0) {
    arr = raw.split(`,`)
  } else {
    return undefined
  }
  const cleaned = allowed
    ? arr.filter((v) => allowed.includes(v))
    : arr.filter((v) => v.length > 0)
  return cleaned.length ? cleaned.join(`,`) : undefined
}

// validateSearch body for any route carrying board filters; drops anything
// unrecognised.
export function parseIssueFilterSearch(
  search: Record<string, unknown>
): IssueFilterSearch {
  return {
    status: validatedCsv(search.status, STATUS_VALUES),
    priority: validatedCsv(search.priority, PRIORITY_VALUES),
    labels: validatedCsv(search.labels),
  }
}

export function issueFiltersFromSearch(search: IssueFilterSearch): IssueFilters {
  return {
    statuses: search.status ? (search.status.split(`,`) as IssueStatus[]) : [],
    priorities: search.priority
      ? (search.priority.split(`,`) as IssuePriority[])
      : [],
    labelIds: search.labels ? search.labels.split(`,`) : [],
  }
}

// Always emits all three keys (undefined when empty) so spreading over `prev`
// search state clears removed filters from the URL.
export function issueFilterSearchFromFilters(
  filters: IssueFilters
): IssueFilterSearch {
  return {
    status: filters.statuses.length ? filters.statuses.join(`,`) : undefined,
    priority: filters.priorities.length
      ? filters.priorities.join(`,`)
      : undefined,
    labels: filters.labelIds.length ? filters.labelIds.join(`,`) : undefined,
  }
}
