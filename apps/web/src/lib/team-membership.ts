// Barrel re-export. Implementation lives in lib/auth/.
// Kept so existing imports of @/lib/team-membership continue to work.

export {
  assertTeamAccess,
  assertAssigneeInTeam,
  assertMatchingTeamIds,
  getReadableTeamIds,
  getReadableUserIdsInTeams,
  getUserTeamIds,
  getSoleHumanMemberId,
  getTeamMember,
  assertTeamMember,
  assertTeamOwner,
  getBoardTeamId,
  assertBoardMember,
  getIssueTeamContext,
  getAttachmentTeamContext,
  getTeamById,
} from "@/lib/auth/membership"

// Canonical authorization predicates (capability/action-driven). These replace
// the old per-action assertCan* helpers.
export {
  resolveTeamAccess,
  assertIssueAccess,
  assertIssueLabelTeamMatch,
} from "@/lib/auth/access"
export type { TeamCapability, IssueAction } from "@/lib/auth/access"

export {
  sqlStringLiteral,
  buildWhereClause,
  buildTextInClause,
  buildTeamScopedChildWhere,
  andClauses,
  orClauses,
} from "@/lib/auth/shape-where"
