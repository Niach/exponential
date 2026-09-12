import { issues } from "@/db/schema"

// EXP-707: the ONE camelCase mirror of the issues shape's server-pinned
// column allowlist (routes/api/shapes/issues.ts ISSUE_COLUMNS) — used by
// tRPC issues.get and the MCP issues_get/issues_list projections so every
// read surface ships the same pinned set and a future server-only issue
// column never leaks. The REV2-5 scoping columns (team_id, board_deleted_at,
// board_archived_at) are excluded everywhere. Parity with the shape list is
// locked by lib/mcp/api-conventions.test.ts.
export const issueWireColumns = {
  id: issues.id,
  boardId: issues.boardId,
  number: issues.number,
  identifier: issues.identifier,
  title: issues.title,
  description: issues.description,
  status: issues.status,
  statusId: issues.statusId,
  priority: issues.priority,
  assigneeId: issues.assigneeId,
  creatorId: issues.creatorId,
  source: issues.source,
  dueDate: issues.dueDate,
  sortOrder: issues.sortOrder,
  completedAt: issues.completedAt,
  duplicateOfId: issues.duplicateOfId,
  prUrl: issues.prUrl,
  prNumber: issues.prNumber,
  prState: issues.prState,
  branch: issues.branch,
  prMergedAt: issues.prMergedAt,
  createdAt: issues.createdAt,
  updatedAt: issues.updatedAt,
}

// EXP-847: the LIST projection. `exponential_issues_list` hands whole issue
// rows to an agent, and 50 full descriptions are the bulk of a listing's
// tokens — so a list row carries a HEAD of the description and the agent
// fetches the rest with `exponential_issues_get` (which keeps the full text).
// The columns themselves stay `issueWireColumns` (api-conventions.test.ts
// locks that set); the cut is a mapping over the selected rows.
export const ISSUE_LIST_DESCRIPTION_MAX = 200

/** The description cut to `ISSUE_LIST_DESCRIPTION_MAX` characters, with an
 * ellipsis appended when anything was dropped. Null/short text is returned
 * untouched. */
export function truncateIssueDescription(
  description: string | null | undefined
): string | null {
  if (typeof description !== `string`) return description ?? null
  return description.length > ISSUE_LIST_DESCRIPTION_MAX
    ? `${description.slice(0, ISSUE_LIST_DESCRIPTION_MAX)}…`
    : description
}

/** `truncateIssueDescription` over a list projection's rows. Rows that carry
 * no `description` key at all pass through unchanged. */
export function withTruncatedDescriptions<
  T extends { description?: string | null },
>(rows: readonly T[]): T[] {
  return rows.map((row) =>
    `description` in row
      ? { ...row, description: truncateIssueDescription(row.description) }
      : row
  )
}
