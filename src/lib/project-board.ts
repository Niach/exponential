import type { Issue, IssueLabel, Label, User } from "@/db/schema"
import type { IssueFilters } from "@/lib/filters"
import { matchesFilters } from "@/lib/filters"
import {
  formatDateForMutation,
  issueStatusOrder,
  type IssueStatus,
} from "@/lib/domain"

function isIssueOverdue(issue: Issue, today: string) {
  return (
    issue.dueDate !== null &&
    issue.dueDate < today &&
    issue.status !== `done` &&
    issue.status !== `cancelled`
  )
}

function compareIssuesForGroup(today: string) {
  return (a: Issue, b: Issue) => {
    const aOverdue = isIssueOverdue(a, today)
    const bOverdue = isIssueOverdue(b, today)

    if (aOverdue !== bOverdue) {
      return aOverdue ? -1 : 1
    }

    if (a.dueDate !== null && b.dueDate !== null) {
      return a.dueDate.localeCompare(b.dueDate)
    }

    if (a.dueDate !== null) return -1
    if (b.dueDate !== null) return 1

    return 0
  }
}

export interface IssueGroup {
  issues: Issue[]
  status: IssueStatus
}

export function buildUserMap(users: User[]) {
  return new Map(users.map((user) => [user.id, user]))
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
  const compare = compareIssuesForGroup(today)

  const groups = issueStatusOrder.map((status) => ({
    status,
    issues: issues.filter((issue) => issue.status === status).sort(compare),
  }))

  if (statuses.length > 0) {
    return groups.filter((group) => statuses.includes(group.status))
  }

  return groups.filter((group) => group.issues.length > 0)
}

export function getEditingIssue(
  issues: Issue[],
  editingIssueId: string | null
) {
  if (!editingIssueId) {
    return null
  }

  return issues.find((issue) => issue.id === editingIssueId) ?? null
}

export function getEditingIssueLabelIds(
  issueLabels: IssueLabel[],
  editingIssueId: string | null
) {
  if (!editingIssueId) {
    return []
  }

  return issueLabels
    .filter((issueLabel) => issueLabel.issueId === editingIssueId)
    .map((issueLabel) => issueLabel.labelId)
}
