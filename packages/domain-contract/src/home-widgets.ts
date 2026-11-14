// #EXP-1830: the home screen widgets (Android Glance, iOS WidgetKit). Pure and
// locked by `fixtures/home-widgets.json`, replayed by iOS ExpCore
// `HomeWidgets.swift` and Android `domain/HomeWidgets.kt`. The fixture's
// `$comment` holds the rules; this is the reference.

import contract from "../contract.json" with { type: "json" }
import fixture from "../fixtures/home-widgets.json" with { type: "json" }

export const HOME_WIDGETS = fixture

export interface WidgetStatus {
  id: string
  category: string
  color: string
  sortOrder: number
}

export interface WidgetIssue {
  id: string
  identifier: string
  title: string
  statusId: string
  boardId: string
  teamId: string
  priority: string
  sortOrder: number
  assigneeId: string | null
  updatedAt: string
}

export interface WidgetIssueRow {
  id: string
  identifier: string
  title: string
  statusCategory: string
  statusColor: string
}

export interface WidgetList<Row> {
  count: number
  rows: Row[]
}

function openIssues(issues: WidgetIssue[], statuses: WidgetStatus[]) {
  const byId = new Map(statuses.map((status) => [status.id, status]))
  return issues.flatMap((issue) => {
    const status = byId.get(issue.statusId)
    return status && fixture.openCategories.includes(status.category) ? [{ issue, status }] : []
  })
}

const issueRow = ({ issue, status }: { issue: WidgetIssue; status: WidgetStatus }): WidgetIssueRow => ({
  id: issue.id,
  identifier: issue.identifier,
  title: issue.title,
  statusCategory: status.category,
  statusColor: status.color,
})

/** One board's open issues in its reading order. */
export function boardWidget(input: {
  boardId: string
  statuses: WidgetStatus[]
  issues: WidgetIssue[]
  max: number
}): WidgetList<WidgetIssueRow> {
  const rows = openIssues(input.issues.filter((issue) => issue.boardId === input.boardId), input.statuses).sort(
    (a, b) =>
      fixture.categoryOrder.indexOf(a.status.category) - fixture.categoryOrder.indexOf(b.status.category) ||
      a.status.sortOrder - b.status.sortOrder ||
      a.issue.sortOrder - b.issue.sortOrder ||
      a.issue.identifier.localeCompare(b.issue.identifier),
  )
  return { count: rows.length, rows: rows.slice(0, input.max).map(issueRow) }
}

/** The viewer's open issues in the picked teams (none = every team). */
export function myIssuesWidget(input: {
  me: string
  teamIds: string[]
  statuses: WidgetStatus[]
  issues: WidgetIssue[]
  max: number
}): WidgetList<WidgetIssueRow> {
  const priority = contract.issuePriority.displayOrder
  const mine = input.issues.filter(
    (issue) => issue.assigneeId === input.me && (input.teamIds.length === 0 || input.teamIds.includes(issue.teamId)),
  )
  const rows = openIssues(mine, input.statuses).sort(
    (a, b) =>
      priority.indexOf(a.issue.priority) - priority.indexOf(b.issue.priority) ||
      b.issue.updatedAt.localeCompare(a.issue.updatedAt) ||
      a.issue.identifier.localeCompare(b.issue.identifier),
  )
  return { count: rows.length, rows: rows.slice(0, input.max).map(issueRow) }
}

export interface WidgetNeedsYouInput {
  messageId: string
  sessionId: string
  rootId: string
  kind: string
  opens: string
  title: string
  age: string
}

export interface WidgetNeedsYouRow {
  messageId: string
  sessionId: string
  rootId: string
  opens: string
  icon: string
  title: string
  age: string
}

const KIND_ICON: Record<string, string> = {
  plan: fixture.icons.plan,
  question: fixture.icons.question,
  pr: fixture.icons.pr,
}

/** THE Needs you rows (needs-you.json order), an encrypted chat's text withheld. */
export function needsYouWidget(input: {
  rows: WidgetNeedsYouInput[]
  sealedRootIds: string[]
  max: number
}): WidgetList<WidgetNeedsYouRow> {
  const sealed = new Set(input.sealedRootIds)
  return {
    count: input.rows.length,
    rows: input.rows.slice(0, input.max).map((row) => ({
      messageId: row.messageId,
      sessionId: row.sessionId,
      rootId: row.rootId,
      opens: row.opens,
      icon: KIND_ICON[row.kind] ?? fixture.icons.question,
      title: sealed.has(row.rootId) ? fixture.copy.sealedRow : row.title,
      age: row.age,
    })),
  }
}
