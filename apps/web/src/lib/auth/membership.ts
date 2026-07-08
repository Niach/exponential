import { TRPCError } from "@trpc/server"
import { and, eq, inArray, isNull } from "drizzle-orm"
import {
  attachments,
  issueLabels,
  issues,
  projects,
  workspaceMembers,
  workspaces,
} from "@/db/schema"
import type { WorkspaceMember } from "@/db/schema"
import type { WorkspaceRole } from "@/lib/domain"

export type WorkspaceMemberRecord = Pick<
  WorkspaceMember,
  `role` | `userId` | `workspaceId`
>

async function getDb() {
  const { db } = await import(`@/db/connection`)
  return db
}

export function assertWorkspaceAccess(
  member: WorkspaceMemberRecord | undefined,
  requiredRoles?: WorkspaceRole[]
): asserts member is WorkspaceMemberRecord {
  if (!member) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Not a member of this workspace`,
    })
  }

  if (requiredRoles && !requiredRoles.includes(member.role)) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Insufficient role. Required: ${requiredRoles.join(`, `)}`,
    })
  }
}

// The instance-wide public surface: every unarchived feedback-board project
// (projects.type = 'feedback') plus per-toggle sub-lists. Anonymous shape
// requests resolve against this scope; authed members never do (their where
// clauses stay membership-derived and byte-identical). Cached per process —
// invalidated by projects.create/update/delete and bootstrap.
export type PublicProjectScope = {
  // All public (feedback-type, unarchived) project ids.
  projectIds: string[]
  // Distinct workspaces hosting at least one public project (the host row's
  // name/slug must sync for board routing/header).
  workspaceIds: string[]
  // Public projects with publicShowComments = true.
  commentProjectIds: string[]
  // Public projects with publicShowActivity = true.
  activityProjectIds: string[]
  // Public projects with publicShowCoding != 'off' (badge or live).
  codingProjectIds: string[]
  // Public projects with publicShowCoding = 'live' (public activity stream).
  liveProjectIds: string[]
}

let publicProjectScopeCache: PublicProjectScope | undefined = undefined

export async function getPublicProjectScope(): Promise<PublicProjectScope> {
  if (publicProjectScopeCache !== undefined) {
    return publicProjectScopeCache
  }
  const db = await getDb()
  const rows = await db
    .select({
      id: projects.id,
      workspaceId: projects.workspaceId,
      publicShowComments: projects.publicShowComments,
      publicShowActivity: projects.publicShowActivity,
      publicShowCoding: projects.publicShowCoding,
    })
    .from(projects)
    .where(
      and(
        eq(projects.type, `feedback`),
        isNull(projects.archivedAt),
        // Trashed feedback boards leave the public surface immediately (heals
        // every anonymous shape scope + the sitemap).
        isNull(projects.deletedAt)
      )
    )
  publicProjectScopeCache = {
    projectIds: rows.map((row) => row.id),
    workspaceIds: [...new Set(rows.map((row) => row.workspaceId))],
    commentProjectIds: rows
      .filter((row) => row.publicShowComments)
      .map((row) => row.id),
    activityProjectIds: rows
      .filter((row) => row.publicShowActivity)
      .map((row) => row.id),
    codingProjectIds: rows
      .filter((row) => row.publicShowCoding !== `off`)
      .map((row) => row.id),
    liveProjectIds: rows
      .filter((row) => row.publicShowCoding === `live`)
      .map((row) => row.id),
  }
  return publicProjectScopeCache
}

export function invalidatePublicProjectCache() {
  publicProjectScopeCache = undefined
}

// Label ids used on at least one public project's issues. Uncached: the
// anonymous labels shape is web-only, low-volume, and a stale list would hide
// freshly-applied labels; correctness beats the extra query. (The resulting
// where clause is the one data-driven anonymous clause — acceptable churn for
// the web collection layer, which recovers from must-refetch. Never reuse this
// pattern for authed shapes.)
export async function getPublicLabelIds(): Promise<string[]> {
  const scope = await getPublicProjectScope()
  if (scope.projectIds.length === 0) return []
  const db = await getDb()
  const rows = await db
    .selectDistinct({ labelId: issueLabels.labelId })
    .from(issueLabels)
    .where(inArray(issueLabels.projectId, scope.projectIds))
  return rows.map((row) => row.labelId)
}

// Resolves the set of workspace ids readable by a caller — used by shape
// proxies. Authed users see only workspaces they have joined; anonymous
// callers see the workspaces hosting a public feedback board (name/slug only
// in practice — every other shape scopes anonymous access per-project).
export async function getReadableWorkspaceIds(
  userId: string | null
): Promise<string[]> {
  if (userId) return getUserWorkspaceIds(userId)
  return (await getPublicProjectScope()).workspaceIds
}

export async function getReadableProjectIds(
  userId: string | null
): Promise<string[]> {
  if (userId) return getUserProjectIds(userId)
  return (await getPublicProjectScope()).projectIds
}

// Resolves the user ids whose full `users` rows the caller may sync via the
// users shape (and read via users.listByWorkspaceIds). The users table carries
// EMAILS and NAMES, so this is deliberately tighter than workspace
// readability: a caller sees co-members of workspaces they have joined, plus
// themself. Since v7 every membership is an explicit invite (the self-service
// public join is gone), so no per-workspace exclusion is needed. Anonymous
// callers get nothing — public-board viewers render a deterministic anonymous
// handle for every user row that never syncs.
export async function getReadableUserIdsInWorkspaces(
  userId: string | null
): Promise<string[]> {
  if (!userId) return []
  const db = await getDb()
  const membershipRows = await db
    .select({ workspaceId: workspaceMembers.workspaceId })
    .from(workspaceMembers)
    .where(eq(workspaceMembers.userId, userId))
  const joinedWorkspaceIds = membershipRows.map((row) => row.workspaceId)
  if (joinedWorkspaceIds.length === 0) return [userId]
  const rows = await db
    .select({ userId: workspaceMembers.userId })
    .from(workspaceMembers)
    .where(inArray(workspaceMembers.workspaceId, joinedWorkspaceIds))
  return [...new Set([userId, ...rows.map((row) => row.userId)])]
}

export async function getUserWorkspaceIds(userId: string): Promise<string[]> {
  const db = await getDb()
  const rows = await db
    .select({ workspaceId: workspaceMembers.workspaceId })
    .from(workspaceMembers)
    .where(eq(workspaceMembers.userId, userId))

  return rows.map((row) => row.workspaceId)
}

export async function getUserProjectIds(userId: string): Promise<string[]> {
  const workspaceIds = await getUserWorkspaceIds(userId)

  if (workspaceIds.length === 0) {
    return []
  }

  const db = await getDb()
  const rows = await db
    .select({ id: projects.id })
    .from(projects)
    // Trashed projects drop out of the authed issues shape scope.
    .where(
      and(
        inArray(projects.workspaceId, workspaceIds),
        isNull(projects.deletedAt)
      )
    )

  return rows.map((row) => row.id)
}

export async function getWorkspaceMember(userId: string, workspaceId: string) {
  const db = await getDb()
  const [member] = await db
    .select({
      userId: workspaceMembers.userId,
      workspaceId: workspaceMembers.workspaceId,
      role: workspaceMembers.role,
    })
    .from(workspaceMembers)
    .where(
      and(
        eq(workspaceMembers.workspaceId, workspaceId),
        eq(workspaceMembers.userId, userId)
      )
    )
    .limit(1)

  return member
}

export async function assertWorkspaceMember(
  userId: string,
  workspaceId: string,
  requiredRoles?: WorkspaceRole[]
) {
  const member = await getWorkspaceMember(userId, workspaceId)
  assertWorkspaceAccess(member, requiredRoles)
  return member
}

export async function assertWorkspaceOwner(
  userId: string,
  workspaceId: string
) {
  return assertWorkspaceMember(userId, workspaceId, [`owner`])
}

export async function getProjectWorkspaceId(projectId: string) {
  const db = await getDb()
  const [project] = await db
    .select({
      id: projects.id,
      workspaceId: projects.workspaceId,
    })
    .from(projects)
    // Trashed projects 404 for every member mutation (issues.create,
    // projects.update/setRepository via assertProjectMember, widgets retarget,
    // MCP projects_get). The restore path uses a direct select, not this helper.
    .where(and(eq(projects.id, projectId), isNull(projects.deletedAt)))
    .limit(1)

  if (!project) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Project not found`,
    })
  }

  return project
}

export async function assertProjectMember(
  userId: string,
  projectId: string,
  requiredRoles?: WorkspaceRole[]
) {
  const project = await getProjectWorkspaceId(projectId)
  await assertWorkspaceMember(userId, project.workspaceId, requiredRoles)
  return project
}

export async function getIssueWorkspaceContext(issueId: string) {
  const db = await getDb()
  const [issueContext] = await db
    .select({
      issueId: issues.id,
      projectId: issues.projectId,
      workspaceId: projects.workspaceId,
    })
    .from(issues)
    .innerJoin(projects, eq(issues.projectId, projects.id))
    // Trashed project ⇒ its issues 404 for all issue/comment/label/subscribe
    // reads + mutations (restored automatically on restore).
    .where(and(eq(issues.id, issueId), isNull(projects.deletedAt)))
    .limit(1)

  if (!issueContext) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Issue not found`,
    })
  }

  return issueContext
}

export async function getAttachmentWorkspaceContext(attachmentId: string) {
  const db = await getDb()
  const [attachmentContext] = await db
    .select({
      attachmentId: attachments.id,
      issueId: attachments.issueId,
      commentId: attachments.commentId,
      storageKey: attachments.storageKey,
      workspaceId: projects.workspaceId,
      projectId: issues.projectId,
      contentType: attachments.contentType,
      filename: attachments.filename,
      sizeBytes: attachments.sizeBytes,
      // Public feedback-board read path: anonymous byte reads are allowed for
      // unarchived feedback projects (comment attachments only where comments
      // are public).
      projectType: projects.type,
      projectPublicShowComments: projects.publicShowComments,
      projectArchivedAt: projects.archivedAt,
    })
    .from(attachments)
    .innerJoin(issues, eq(attachments.issueId, issues.id))
    .innerJoin(projects, eq(issues.projectId, projects.id))
    // Trashed project ⇒ block attachment byte reads during the trash window
    // (restored automatically on restore).
    .where(and(eq(attachments.id, attachmentId), isNull(projects.deletedAt)))
    .limit(1)

  if (!attachmentContext) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Attachment not found`,
    })
  }

  return attachmentContext
}

export async function getWorkspaceById(workspaceId: string) {
  const db = await getDb()
  const [workspace] = await db
    .select({
      id: workspaces.id,
    })
    .from(workspaces)
    .where(eq(workspaces.id, workspaceId))
    .limit(1)
  return workspace
}

export function assertMatchingWorkspaceIds(
  issueWorkspaceId: string | undefined,
  labelWorkspaceId: string | undefined
) {
  if (!issueWorkspaceId || !labelWorkspaceId) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Missing issue or label workspace`,
    })
  }

  if (issueWorkspaceId !== labelWorkspaceId) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Issue and label must belong to the same workspace`,
    })
  }
}
