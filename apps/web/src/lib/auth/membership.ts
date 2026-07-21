import { TRPCError } from "@trpc/server"
import { and, eq, inArray, isNull } from "drizzle-orm"
import {
  attachments,
  issues,
  boards,
  teamMembers,
  teams,
} from "@/db/schema"
import type { TeamMember } from "@/db/schema"
import type { TeamRole } from "@/lib/domain"

export type TeamMemberRecord = Pick<
  TeamMember,
  `role` | `userId` | `teamId`
>

async function getDb() {
  const { db } = await import(`@/db/connection`)
  return db
}

export function assertTeamAccess(
  member: TeamMemberRecord | undefined,
  requiredRoles?: TeamRole[]
): asserts member is TeamMemberRecord {
  if (!member) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Not a member of this team`,
    })
  }

  if (requiredRoles && !requiredRoles.includes(member.role)) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Insufficient role. Required: ${requiredRoles.join(`, `)}`,
    })
  }
}

// Resolves the set of team ids readable by a caller — used by shape
// proxies. Authed users see only teams they have joined; anonymous
// callers see nothing (EXP-180 removed the public feedback boards — every
// shape is member-only, and buildWhereClause([]) yields the impossible-match
// sentinel).
export async function getReadableTeamIds(
  userId: string | null
): Promise<string[]> {
  if (userId) return getUserTeamIds(userId)
  return []
}

export async function getReadableBoardIds(
  userId: string | null
): Promise<string[]> {
  if (userId) return getUserBoardIds(userId)
  return []
}

// Resolves the user ids whose full `users` rows the caller may sync via the
// users shape (and read via users.listByTeamIds). The users table carries
// EMAILS and NAMES, so this is deliberately tighter than team
// readability: a caller sees co-members of teams they have joined, plus
// themself. Every membership is an explicit invite (the self-service public
// join is gone), so no per-team exclusion is needed. Anonymous callers
// get nothing.
export async function getReadableUserIdsInTeams(
  userId: string | null
): Promise<string[]> {
  if (!userId) return []
  const db = await getDb()
  const membershipRows = await db
    .select({ teamId: teamMembers.teamId })
    .from(teamMembers)
    .where(eq(teamMembers.userId, userId))
  const joinedTeamIds = membershipRows.map((row) => row.teamId)
  if (joinedTeamIds.length === 0) return [userId]
  const rows = await db
    .select({ userId: teamMembers.userId })
    .from(teamMembers)
    .where(inArray(teamMembers.teamId, joinedTeamIds))
  return [...new Set([userId, ...rows.map((row) => row.userId)])]
}

export async function getUserTeamIds(userId: string): Promise<string[]> {
  const db = await getDb()
  const rows = await db
    .select({ teamId: teamMembers.teamId })
    .from(teamMembers)
    .where(eq(teamMembers.userId, userId))

  return rows.map((row) => row.teamId)
}

export async function getUserBoardIds(userId: string): Promise<string[]> {
  const teamIds = await getUserTeamIds(userId)

  if (teamIds.length === 0) {
    return []
  }

  const db = await getDb()
  const rows = await db
    .select({ id: boards.id })
    .from(boards)
    // Trashed boards drop out of the authed issues shape scope.
    .where(
      and(
        inArray(boards.teamId, teamIds),
        isNull(boards.deletedAt)
      )
    )

  return rows.map((row) => row.id)
}

export async function getTeamMember(userId: string, teamId: string) {
  const db = await getDb()
  const [member] = await db
    .select({
      userId: teamMembers.userId,
      teamId: teamMembers.teamId,
      role: teamMembers.role,
    })
    .from(teamMembers)
    .where(
      and(
        eq(teamMembers.teamId, teamId),
        eq(teamMembers.userId, userId)
      )
    )
    .limit(1)

  return member
}

export async function assertTeamMember(
  userId: string,
  teamId: string,
  requiredRoles?: TeamRole[]
) {
  const member = await getTeamMember(userId, teamId)
  assertTeamAccess(member, requiredRoles)
  return member
}

export async function assertTeamOwner(
  userId: string,
  teamId: string
) {
  return assertTeamMember(userId, teamId, [`owner`])
}

// Subject-side membership validation for issue assignment. Unlike
// assertTeamMember (ACTOR authorization -> FORBIDDEN), an invalid
// assignee is bad INPUT -> BAD_REQUEST. Without this check any member could
// push-notify and auto-subscribe arbitrary users of the instance by assigning
// them issues in teams they don't belong to (cross-tenant notification
// injection). The message is identical for "user does not exist" and "user is
// not a member" so the endpoint cannot be used to enumerate account ids.
export async function assertAssigneeInTeam(
  assigneeId: string,
  teamId: string
): Promise<void> {
  const member = await getTeamMember(assigneeId, teamId)
  if (!member) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Assignee must be a member of this team`,
    })
  }
}

// EXP-50: solo-team default assignment. Returns the user id of the
// team's only human (non-agent) member, or null when the team has
// zero or two-plus members.
export async function getSoleHumanMemberId(
  teamId: string
): Promise<string | null> {
  const db = await getDb()
  const rows = await db
    .select({ userId: teamMembers.userId })
    .from(teamMembers)
    .where(eq(teamMembers.teamId, teamId))
    .limit(2)
  return rows.length === 1 ? rows[0].userId : null
}

export async function getBoardTeamId(boardId: string) {
  const db = await getDb()
  const [board] = await db
    .select({
      id: boards.id,
      teamId: boards.teamId,
    })
    .from(boards)
    // Trashed boards 404 for every member mutation (issues.create,
    // boards.update/setRepository via assertBoardMember, widgets retarget,
    // MCP boards_get). The restore path uses a direct select, not this helper.
    .where(and(eq(boards.id, boardId), isNull(boards.deletedAt)))
    .limit(1)

  if (!board) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Board not found`,
    })
  }

  return board
}

export async function assertBoardMember(
  userId: string,
  boardId: string,
  requiredRoles?: TeamRole[]
) {
  const board = await getBoardTeamId(boardId)
  await assertTeamMember(userId, board.teamId, requiredRoles)
  return board
}

export async function getIssueTeamContext(issueId: string) {
  const db = await getDb()
  const [issueContext] = await db
    .select({
      issueId: issues.id,
      boardId: issues.boardId,
      teamId: boards.teamId,
    })
    .from(issues)
    .innerJoin(boards, eq(issues.boardId, boards.id))
    // Trashed board ⇒ its issues 404 for all issue/comment/label/subscribe
    // reads + mutations (restored automatically on restore).
    .where(and(eq(issues.id, issueId), isNull(boards.deletedAt)))
    .limit(1)

  if (!issueContext) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Issue not found`,
    })
  }

  return issueContext
}

export async function getAttachmentTeamContext(attachmentId: string) {
  const db = await getDb()
  const [attachmentContext] = await db
    .select({
      attachmentId: attachments.id,
      issueId: attachments.issueId,
      commentId: attachments.commentId,
      storageKey: attachments.storageKey,
      teamId: boards.teamId,
      boardId: issues.boardId,
      contentType: attachments.contentType,
      filename: attachments.filename,
      sizeBytes: attachments.sizeBytes,
    })
    .from(attachments)
    .innerJoin(issues, eq(attachments.issueId, issues.id))
    .innerJoin(boards, eq(issues.boardId, boards.id))
    // Trashed board ⇒ block attachment byte reads during the trash window
    // (restored automatically on restore).
    .where(and(eq(attachments.id, attachmentId), isNull(boards.deletedAt)))
    .limit(1)

  if (!attachmentContext) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Attachment not found`,
    })
  }

  return attachmentContext
}

export async function getTeamById(teamId: string) {
  const db = await getDb()
  const [team] = await db
    .select({
      id: teams.id,
    })
    .from(teams)
    .where(eq(teams.id, teamId))
    .limit(1)
  return team
}

export function assertMatchingTeamIds(
  issueTeamId: string | undefined,
  labelTeamId: string | undefined
) {
  if (!issueTeamId || !labelTeamId) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Missing issue or label team`,
    })
  }

  if (issueTeamId !== labelTeamId) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Issue and label must belong to the same team`,
    })
  }
}
