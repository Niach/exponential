// The IssueFilters shape and matchesFilters() are mirrored across three
// clients. If you change the filter shape or matching semantics here, also
// update apps/ios/Exponential/Domain/IssueFilters.swift and
// apps/android/app/src/main/java/com/exponential/app/domain/IssueFilters.kt
// to keep the three clients in sync (no shared package yet).
import type { Issue } from "@/db/schema"
import type { IssuePriority } from "@/lib/domain"
import { issuePriorityOptions, issueStatusOptions } from "@/lib/domain"

export interface IssueFilters {
  // EXP-314 dual-token status filter: a token is either an `issue_statuses`
  // row uuid (the modern form the picker writes) or a legacy anchor-enum value
  // (`backlog`, `done`, …) carried by an older shared/bookmarked URL or an older
  // native client. Matching accepts both; the picker normalizes to row ids on
  // the next toggle.
  statusTokens: string[]
  priorities: IssuePriority[]
  labelIds: string[]
}

export const emptyFilters: IssueFilters = {
  statusTokens: [],
  priorities: [],
  labelIds: [],
}

export function matchesFilters(
  issue: Issue,
  issueLabelIds: string[],
  filters: IssueFilters,
  // The issue's RESOLVED status (from useTeamStatuses().resolve) — matching
  // on it keeps filters agreeing with the rendered groups even for rows whose
  // status_id is NULL/stale (pre-backfill, deleted-status races), exactly
  // like the three native mirrors. Callers without team rows omit it and get
  // the raw dual-column match.
  resolvedStatus?: { id: string; builtinKey: string | null }
): boolean {
  if (
    filters.statusTokens.length > 0 &&
    !filters.statusTokens.some((token) =>
      resolvedStatus
        ? token === resolvedStatus.id || token === resolvedStatus.builtinKey
        : // A uuid token only ever equals `statusId`; an enum token only ever
          // equals `status` — one dual comparison covers both without team
          // data.
          token === issue.statusId || token === issue.status
    )
  )
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
    filters.statusTokens.length +
    filters.priorities.length +
    filters.labelIds.length
  )
}

export function hasActiveFilters(filters: IssueFilters): boolean {
  return activeFilterCount(filters) > 0
}

// --- URL search-param form (web-only, not mirrored on native) ---------------
// Filters live in the URL as clean comma-joined values (?status=done,in_progress
// &priority=high&labels=<id>,<id>) so a filtered board is shareable and survives
// a refresh. The issue-detail route accepts the SAME optional params so the
// prev/next switcher can follow the board's filtered ordering; helpers are
// shared here so the two routes can't drift.
//
// EXP-314 keeps the `status` KEY and widens its allowlist to row uuids — old
// links (?status=done) keep working verbatim.

export interface IssueFilterSearch {
  status?: string
  priority?: string
  labels?: string
}

const STATUS_VALUES: readonly string[] = issueStatusOptions.map((o) => o.value)
const PRIORITY_VALUES: readonly string[] = issuePriorityOptions.map(
  (o) => o.value
)
const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

export function isValidStatusToken(value: string): boolean {
  return UUID_RE.test(value) || STATUS_VALUES.includes(value)
}

// Coerce a raw search value (array or comma string) to a validated, comma-joined
// string, or undefined when empty — so cleared filters drop out of the URL.
function validatedCsv(
  raw: unknown,
  allowed?: (value: string) => boolean
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
    ? arr.filter((v) => allowed(v))
    : arr.filter((v) => v.length > 0)
  return cleaned.length ? cleaned.join(`,`) : undefined
}

const isKnownPriority = (value: string) => PRIORITY_VALUES.includes(value)

// validateSearch body for any route carrying board filters; drops anything
// unrecognised.
export function parseIssueFilterSearch(
  search: Record<string, unknown>
): IssueFilterSearch {
  return {
    status: validatedCsv(search.status, isValidStatusToken),
    priority: validatedCsv(search.priority, isKnownPriority),
    labels: validatedCsv(search.labels),
  }
}

export function issueFiltersFromSearch(search: IssueFilterSearch): IssueFilters {
  return {
    statusTokens: search.status ? search.status.split(`,`) : [],
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
    status: filters.statusTokens.length
      ? filters.statusTokens.join(`,`)
      : undefined,
    priority: filters.priorities.length
      ? filters.priorities.join(`,`)
      : undefined,
    labels: filters.labelIds.length ? filters.labelIds.join(`,`) : undefined,
  }
}
