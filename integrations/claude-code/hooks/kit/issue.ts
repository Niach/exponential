// GENERATED from packages/plugin-kit/src/model/core/issue.ts (bun run plugin-kit:generate). Do not edit.
import { ISSUE_PRIORITIES, ISSUE_STATUSES, type IssuePriority, type IssueRow, type IssueStatus } from "./wire"

// The issue's words and marks outside the app: the labels every client shows,
// and for a surface without the icon font (a terminal) one glyph per status.

export const STATUS_LABEL: Record<IssueStatus, string> = {
  backlog: `Backlog`,
  in_progress: `In Progress`,
  in_review: `In Review`,
  done: `Done`,
  cancelled: `Cancelled`,
  duplicate: `Duplicate`,
}

export const PRIORITY_LABEL: Record<IssuePriority, string> = {
  urgent: `Urgent`,
  high: `High`,
  medium: `Medium`,
  low: `Low`,
  none: `No priority`,
}

/** The status mark where no icon can be drawn: the icon's own shape
 *  (an open ring, a half, a full disc, a struck ring). */
export const STATUS_GLYPH: Record<IssueStatus, string> = {
  backlog: `○`,
  in_progress: `◐`,
  in_review: `◑`,
  done: `●`,
  cancelled: `⊘`,
  duplicate: `⊘`,
}

/** The semantic colour of a status (design-tokens `semantic`; the web's
 *  `BUILTIN_STATUS_COLOR_CLASS`). */
export type Tone = `neutral` | `yellow` | `green` | `red` | `orange` | `blue`

export const STATUS_TONE: Record<IssueStatus, Tone> = {
  backlog: `neutral`,
  in_progress: `yellow`,
  in_review: `green`,
  done: `blue`,
  cancelled: `neutral`,
  duplicate: `neutral`,
}

export function normalizeStatus(value: unknown): IssueStatus {
  return (ISSUE_STATUSES as readonly unknown[]).includes(value) ? (value as IssueStatus) : `backlog`
}

export function normalizePriority(value: unknown): IssuePriority {
  return (ISSUE_PRIORITIES as readonly unknown[]).includes(value) ? (value as IssuePriority) : `none`
}

export interface IssueGroup<Row extends IssueRow = IssueRow> {
  status: IssueStatus
  issues: Row[]
}

/** Rows grouped by their anchor status in the contract's display order;
 *  empty groups are dropped, row order inside a group is the tool's. The
 *  tool rows carry the ANCHOR, so a team's custom status lists under the
 *  builtin it anchors to. */
export function groupIssuesByStatus<Row extends IssueRow>(rows: readonly Row[]): IssueGroup<Row>[] {
  const groups = new Map<IssueStatus, Row[]>()
  for (const row of rows) {
    const status = normalizeStatus(row.status)
    const group = groups.get(status) ?? []
    group.push({ ...row, status, priority: normalizePriority(row.priority) })
    groups.set(status, group)
  }
  return ISSUE_STATUSES.filter((status) => groups.has(status)).map((status) => ({ status, issues: groups.get(status) ?? [] }))
}

const IDENTIFIER = /(?:^|[^A-Za-z0-9])([A-Za-z][A-Za-z0-9]{0,3}-\d{1,7})(?![A-Za-z0-9])/

/** The issue a branch names: `exp/EXP-12`, `feature/exp-12-fix` → `EXP-12` (a board prefix is at most four characters).
 *  A session branch (`exp/session-b0291ed2`) names no issue: its tail is not
 *  a number. */
export function branchIssueIdentifier(branch: string | null | undefined): string | null {
  const match = IDENTIFIER.exec(branch ?? ``)
  return match ? match[1]!.toUpperCase() : null
}

/** An identifier a person typed (`exp-12`, `#EXP-12`), or null. */
export function parseIssueIdentifier(text: string | null | undefined): string | null {
  const match = /^#?([A-Za-z][A-Za-z0-9]{0,3}-\d{1,7})$/.exec((text ?? ``).trim())
  return match ? match[1]!.toUpperCase() : null
}

/** A title cut for one line of `columns` cells. */
export function clip(text: string, columns: number): string {
  const flat = text.replace(/\s+/g, ` `).trim()
  if (columns <= 0) return ``
  return flat.length > columns ? `${flat.slice(0, Math.max(0, columns - 1))}…` : flat
}
