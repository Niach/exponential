import type { IssueRow } from "./model"

// EXP-1183 — the issue list's pure half: the `exponential_issues_list` row
// fields beyond `IssueRow` (lib/issue-columns.ts `issueWireColumns`), the
// due-date tone (`lib/issue-due-date.ts`, REV2-48) and the assignee lookup
// plan the view runs through the host.

export interface IssueListRow extends IssueRow {
  boardId?: string | null
  assigneeId?: string | null
}

/** One `exponential_members_list` row. */
export interface MemberRow {
  id: string
  name?: string | null
  email?: string | null
  image?: string | null
}

export type DueDateTone = `overdue` | `today` | `upcoming`

export const DUE_DATE_TONE_CLASS: Record<DueDateTone, string> = {
  overdue: `text-red-500`,
  today: `text-orange-500`,
  upcoming: `text-muted-foreground`,
}

/** The device-LOCAL `YYYY-MM-DD` the tone compares against. */
export function localDay(date = new Date()): string {
  const pad = (value: number) => String(value).padStart(2, `0`)
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

/** REV2-48: due TODAY wins over overdue, then overdue, else muted. */
export function dueDateTone(dueDate: string, today = localDay()): DueDateTone {
  const day = dueDate.slice(0, 10)
  if (day === today) return `today`
  return day < today ? `overdue` : `upcoming`
}

/** `Oct 5` — the list's `formatDate`, read as a LOCAL date (no UTC shift). */
export function formatDueDate(dueDate: string): string {
  const [year, month, day] = dueDate.slice(0, 10).split(`-`).map(Number)
  if (!year || !month || !day) return dueDate
  return new Date(year, month - 1, day).toLocaleDateString(`en-US`, {
    month: `short`,
    day: `numeric`,
  })
}

/** The boards whose team members can name the list's assignees — only
 *  boards holding an assigned row, at most `cap` (each costs a tool call). */
export function assigneeBoardIds(rows: readonly IssueListRow[], cap = 4): string[] {
  const out: string[] = []
  for (const row of rows) {
    if (!row.assigneeId || !row.boardId || out.includes(row.boardId)) continue
    out.push(row.boardId)
    if (out.length >= cap) break
  }
  return out
}

/** Which optional columns the list draws — a column nobody fills is empty
 *  space (EXP-1023: the web collapses its due cell the same way). */
export function issueListColumns(rows: readonly IssueListRow[]): {
  pr: boolean
  assignee: boolean
  due: boolean
} {
  return {
    pr: rows.some((row) => row.prNumber != null),
    assignee: rows.some((row) => Boolean(row.assigneeId)),
    due: rows.some((row) => Boolean(row.dueDate)),
  }
}
