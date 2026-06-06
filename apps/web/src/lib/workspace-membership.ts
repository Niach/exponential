// Barrel re-export. Implementation lives in lib/auth/.
// Kept so existing imports of @/lib/workspace-membership continue to work.

export {
  assertWorkspaceAccess,
  assertMatchingWorkspaceIds,
  getPublicWorkspaceIds,
  getReadableWorkspaceIds,
  getReadableProjectIds,
  getReadableUserIdsInWorkspaces,
  invalidatePublicWorkspaceCache,
  getUserWorkspaceIds,
  getUserProjectIds,
  getUserIdsInWorkspaces,
  getWorkspaceMember,
  assertWorkspaceMember,
  assertWorkspaceOwner,
  getProjectWorkspaceId,
  assertProjectMember,
  getIssueWorkspaceContext,
  getAttachmentWorkspaceContext,
  getWorkspaceById,
  assertNotPublicWorkspace,
  isWorkspaceModerator,
} from "@/lib/auth/membership"

// Canonical authorization predicates (capability/action-driven). These replace
// the old per-action assertCan* helpers.
export {
  resolveWorkspaceAccess,
  assertIssueAccess,
  assertIssueLabelWorkspaceMatch,
  MODERATION_RESTRICTED_FIELDS,
  isModerationRestricted,
  applyModerationRestrictions,
} from "@/lib/auth/access"
export type { WorkspaceCapability, IssueAction } from "@/lib/auth/access"

export {
  sqlStringLiteral,
  buildWhereClause,
} from "@/lib/auth/shape-where"
