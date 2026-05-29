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
  resolveWorkspaceAccess,
  isWorkspaceModerator,
} from "@/lib/auth/membership"

export {
  assertCanApprovePlan,
  assertCanCreateIssueInProject,
  assertCanMutateIssue,
  assertCanCommentInWorkspace,
  assertCanMutateWorkspaceResources,
  assertIssueLabelWorkspaceMatch,
} from "@/lib/auth/policies"

export {
  sqlStringLiteral,
  buildWhereClause,
} from "@/lib/auth/shape-where"
