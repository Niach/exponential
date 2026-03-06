import type { Issue, IssueLabel, Label, User } from "@/db/schema"
import type { IssueFilters } from "@/lib/filters"
import { matchesFilters } from "@/lib/filters"
import { issueStatusOrder, type IssueStatus } from "@/lib/domain"

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
  const groups = issueStatusOrder.map((status) => ({
    status,
    issues: issues.filter((issue) => issue.status === status),
  }))

  if (statuses.length > 0) {
    return groups.filter((group) => statuses.includes(group.status))
  }

  return groups.filter((group) => group.issues.length > 0)
}

export function getEditingIssue(issues: Issue[], editingIssueId: string | null) {
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
