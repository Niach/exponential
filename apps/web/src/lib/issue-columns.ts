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
