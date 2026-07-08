// Barrel re-export. Implementation lives in lib/auth/.
// Kept so existing imports of @/lib/workspace-membership continue to work.

export {
  assertWorkspaceAccess,
  assertMatchingWorkspaceIds,
  getPublicProjectScope,
  getPublicLabelIds,
  invalidatePublicProjectCache,
  getReadableWorkspaceIds,
  getReadableProjectIds,
  getReadableUserIdsInWorkspaces,
  getUserWorkspaceIds,
  getUserProjectIds,
  getWorkspaceMember,
  assertWorkspaceMember,
  assertWorkspaceOwner,
  getProjectWorkspaceId,
  assertProjectMember,
  getIssueWorkspaceContext,
  getAttachmentWorkspaceContext,
  getWorkspaceById,
} from "@/lib/auth/membership"
export type { PublicProjectScope } from "@/lib/auth/membership"

// Canonical authorization predicates (capability/action-driven). These replace
// the old per-action assertCan* helpers.
export {
  resolveWorkspaceAccess,
  assertIssueAccess,
  assertIssueLabelWorkspaceMatch,
} from "@/lib/auth/access"
export type { WorkspaceCapability, IssueAction } from "@/lib/auth/access"

export {
  sqlStringLiteral,
  buildWhereClause,
  buildTextInClause,
  andClauses,
  orClauses,
} from "@/lib/auth/shape-where"
