import type { Issue, IssueLabel, Label } from "@/db/schema"
import type { IssueFilters } from "@/lib/filters"
import { matchesFilters } from "@/lib/filters"
import {
  formatDateForMutation,
  issueStatusOrder,
  type IssuePriority,
  type IssueStatus,
} from "@/lib/domain"

const priorityRank: Record<IssuePriority, number> = {
  urgent: 0,
  high: 1,
  medium: 2,
  low: 3,
  none: 4,
}

// The minimal shape the canonical comparator needs — `Issue` satisfies it, and
// so do the public feedback board's tRPC rows (whose timestamps arrive as
// strings rather than Dates).
export interface SortableIssue {
  priority: IssuePriority
  dueDate: string | null
  number: number
  completedAt: Date | string | null
  updatedAt: Date | string
}

// Electric rows carry real Dates, but tRPC-serialized rows (public board) and
// optimistic upserts carry strings — Electric's `YYYY-MM-DD hh:mm:ss…+00`
// vs ISO `…T…Z`. Normalize the space to `T` and pad a bare `±hh` offset to
// `±hh:00` (JS Date rejects hour-only offsets) so mixed formats compare as
// real instants (EXP-38 timestamp gotcha). Exported for the release
// comparator (`lib/releases.ts`), which sorts the same mixed formats.
export function timestampMs(value: Date | string): number {
  if (value instanceof Date) return value.getTime()
  const iso = value.replace(` `, `T`).replace(/([+-]\d{2})$/, `$1:00`)
  return new Date(iso).getTime()
}

// EXP-38 canonical in-group comparator — the cross-platform contract, mirrored
// byte-identically on iOS, Android, and desktop (group ORDER itself stays
// `issueStatusOrder`):
// - backlog/todo/in_progress: overdue first (dueDate < today), then priority
//   urgent(0) < high < medium < low < none(4), then dueDate asc with nulls
//   LAST, then issue `number` asc NUMERICALLY (never the identifier string —
//   "EXP-10" sorts before "EXP-9" lexicographically).
// - done: (completedAt ?? updatedAt) DESC — latest completed first.
// - cancelled/duplicate: updatedAt DESC.
export function compareIssuesForGroup(status: IssueStatus, today: string) {
  return (a: SortableIssue, b: SortableIssue): number => {
    if (status === `done`) {
      return (
        timestampMs(b.completedAt ?? b.updatedAt) -
        timestampMs(a.completedAt ?? a.updatedAt)
      )
    }

    if (status === `cancelled` || status === `duplicate`) {
      return timestampMs(b.updatedAt) - timestampMs(a.updatedAt)
    }

    const aOverdue = a.dueDate !== null && a.dueDate < today
    const bOverdue = b.dueDate !== null && b.dueDate < today
    if (aOverdue !== bOverdue) {
      return aOverdue ? -1 : 1
    }

    const priorityDiff = priorityRank[a.priority] - priorityRank[b.priority]
    if (priorityDiff !== 0) {
      return priorityDiff
    }

    if (a.dueDate !== null && b.dueDate !== null && a.dueDate !== b.dueDate) {
      // Safe as a string compare — `dueDate` is a plain DATE column.
      return a.dueDate < b.dueDate ? -1 : 1
    }
    if (a.dueDate !== null && b.dueDate === null) return -1
    if (a.dueDate === null && b.dueDate !== null) return 1

    return a.number - b.number
  }
}

export interface IssueGroup {
  issues: Issue[]
  status: IssueStatus
}

export function buildIssueLabelMap(issueLabels: IssueLabel[], labels: Label[]) {
  const labelMap = new Map(labels.map((label) => [label.id, label]))
  const issueLabelMap = new Map<string, Label[]>()

  for (const issueLabel of issueLabels) {
    const label = labelMap.get(issueLabel.labelId)

    if (!label) {
      continue
    }

    const currentLabels = issueLabelMap.get(issueLabel.issueId) ?? []
    currentLabels.push(label)
    issueLabelMap.set(issueLabel.issueId, currentLabels)
  }

  return issueLabelMap
}

export function buildIssueLabelIdsMap(issueLabels: IssueLabel[]) {
  const issueLabelIdsMap = new Map<string, string[]>()

  for (const issueLabel of issueLabels) {
    const currentLabelIds = issueLabelIdsMap.get(issueLabel.issueId) ?? []
    currentLabelIds.push(issueLabel.labelId)
    issueLabelIdsMap.set(issueLabel.issueId, currentLabelIds)
  }

  return issueLabelIdsMap
}

export function buildFilteredIssues(
  issues: Issue[],
  issueLabelIdsMap: Map<string, string[]>,
  filters: IssueFilters
) {
  return issues.filter((issue) =>
    matchesFilters(issue, issueLabelIdsMap.get(issue.id) ?? [], filters)
  )
}

export function buildVisibleIssueGroups(
  issues: Issue[],
  statuses: IssueFilters[`statuses`]
) {
  const today = formatDateForMutation(new Date()) ?? ``

  const groups = issueStatusOrder.map((status) => ({
    status,
    issues: issues
      .filter((issue) => issue.status === status)
      .sort(compareIssuesForGroup(status, today)),
  }))

  if (statuses.length > 0) {
    return groups.filter((group) => statuses.includes(group.status))
  }

  return groups.filter((group) => group.issues.length > 0)
}

export interface IssueListPosition {
  // 1-based position of the issue in the flattened board sequence.
  index: number
  total: number
  prev: Issue | null
  next: Issue | null
}

// Flattens the board's visible groups (already in `issueStatusOrder` group
// order, each pre-sorted by the EXP-38 comparator) into the single
// user-visible sequence and locates one issue in it — powers the detail
// header's "N / total" prev/next switcher. Returns null when the issue isn't
// in the current view (e.g. its status is filtered out), in which case the
// switcher hides.
export function findIssuePosition(
  groups: IssueGroup[],
  issueId: string
): IssueListPosition | null {
  const sequence = groups.flatMap((group) => group.issues)
  const index = sequence.findIndex((issue) => issue.id === issueId)
  if (index === -1) return null
  return {
    index: index + 1,
    total: sequence.length,
    prev: index > 0 ? sequence[index - 1] : null,
    next: index < sequence.length - 1 ? sequence[index + 1] : null,
  }
}
