import { TRPCError } from "@trpc/server"
import { and, eq, inArray } from "drizzle-orm"
import {
  attachments,
  issues,
  labels,
  projects,
  workspaceMembers,
  workspaces,
} from "@/db/schema"
import type { WorkspaceMember } from "@/db/schema"
import type { WorkspaceRole } from "@/lib/domain"
import { isUserAdmin } from "@/lib/admin"

type WorkspaceMemberRecord = Pick<
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

let publicWorkspaceIdsCache: string[] | undefined = undefined

export async function getPublicWorkspaceIds(): Promise<string[]> {
  if (publicWorkspaceIdsCache !== undefined) {
    return publicWorkspaceIdsCache
  }
  const db = await getDb()
  const rows = await db
    .select({ id: workspaces.id })
    .from(workspaces)
    .where(eq(workspaces.isPublic, true))
  publicWorkspaceIdsCache = rows.map((row) => row.id)
  return publicWorkspaceIdsCache
}

// Resolves the set of workspace ids readable by a caller — used by shape
// proxies. Authed users see their own memberships plus all public workspaces;
// anonymous callers see only public workspaces.
export async function getReadableWorkspaceIds(
  userId: string | null
): Promise<string[]> {
  if (userId) return getUserWorkspaceIds(userId)
  return getPublicWorkspaceIds()
}

export async function getReadableProjectIds(
  userId: string | null
): Promise<string[]> {
  if (userId) return getUserProjectIds(userId)
  return getPublicProjectIds()
}

export async function getReadableUserIdsInWorkspaces(
  userId: string | null
): Promise<string[]> {
  if (userId) return getUserIdsInWorkspaces(userId)
  // Anonymous callers see member identities only for public workspaces, so
  // assignee/creator chips render.
  const publicIds = await getPublicWorkspaceIds()
  if (publicIds.length === 0) return []
  const db = await getDb()
  const rows = await db
    .select({ userId: workspaceMembers.userId })
    .from(workspaceMembers)
    .where(inArray(workspaceMembers.workspaceId, publicIds))
  return [...new Set(rows.map((row) => row.userId))]
}

export async function getPublicProjectIds(): Promise<string[]> {
  const publicIds = await getPublicWorkspaceIds()
  if (publicIds.length === 0) return []
  const db = await getDb()
  const rows = await db
    .select({ id: projects.id })
    .from(projects)
    .where(inArray(projects.workspaceId, publicIds))
  return rows.map((row) => row.id)
}

export function invalidatePublicWorkspaceCache() {
  publicWorkspaceIdsCache = undefined
}

export async function getUserWorkspaceIds(userId: string): Promise<string[]> {
  const db = await getDb()
  const rows = await db
    .select({ workspaceId: workspaceMembers.workspaceId })
    .from(workspaceMembers)
    .where(eq(workspaceMembers.userId, userId))

  const ids = rows.map((row) => row.workspaceId)
  const publicIds = await getPublicWorkspaceIds()
  for (const publicId of publicIds) {
    if (!ids.includes(publicId)) ids.push(publicId)
  }
  return ids
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
    .where(inArray(projects.workspaceId, workspaceIds))

  return rows.map((row) => row.id)
}

export async function getUserIdsInWorkspaces(
  userId: string
): Promise<string[]> {
  const workspaceIds = await getUserWorkspaceIds(userId)

  if (workspaceIds.length === 0) {
    return []
  }

  const db = await getDb()
  const rows = await db
    .select({ userId: workspaceMembers.userId })
    .from(workspaceMembers)
    .where(inArray(workspaceMembers.workspaceId, workspaceIds))

  return [...new Set(rows.map((row) => row.userId))]
}

export function buildWhereClause(column: string, ids: string[]): string {
  if (ids.length === 0) {
    return `"${column}" = '00000000-0000-0000-0000-000000000000'`
  }

  const escapedIds = ids.map((id) => `'${id}'`).join(`,`)
  return `"${column}" IN (${escapedIds})`
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
    .where(eq(projects.id, projectId))
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
    .where(eq(issues.id, issueId))
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
      storageKey: attachments.storageKey,
      workspaceId: projects.workspaceId,
      contentType: attachments.contentType,
      filename: attachments.filename,
      sizeBytes: attachments.sizeBytes,
    })
    .from(attachments)
    .innerJoin(issues, eq(attachments.issueId, issues.id))
    .innerJoin(projects, eq(issues.projectId, projects.id))
    .where(eq(attachments.id, attachmentId))
    .limit(1)

  if (!attachmentContext) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Attachment not found`,
    })
  }

  return attachmentContext
}

async function getWorkspaceById(workspaceId: string) {
  const db = await getDb()
  const [workspace] = await db
    .select({
      id: workspaces.id,
      isPublic: workspaces.isPublic,
      publicWritePolicy: workspaces.publicWritePolicy,
    })
    .from(workspaces)
    .where(eq(workspaces.id, workspaceId))
    .limit(1)
  return workspace
}

// Resolves whether the user can read/use a workspace. Returns:
// - { kind: 'member', workspace, member } when the user is a member
// - { kind: 'public', workspace } when the workspace is public and user is authed
// - throws FORBIDDEN/NOT_FOUND otherwise
export async function resolveWorkspaceAccess(
  userId: string,
  workspaceId: string
) {
  const workspace = await getWorkspaceById(workspaceId)
  if (!workspace) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Workspace not found` })
  }
  const member = await getWorkspaceMember(userId, workspaceId)
  if (member) {
    return { kind: `member` as const, workspace, member }
  }
  if (workspace.isPublic) {
    return { kind: `public` as const, workspace }
  }
  throw new TRPCError({
    code: `FORBIDDEN`,
    message: `Not a member of this workspace`,
  })
}

// Allowed if:
// - Private workspace → user must be a workspace member.
// - Public workspace with publicWritePolicy=members → user must be a workspace member.
// - Public workspace with publicWritePolicy=everyone → any authed user.
export async function assertCanCreateIssueInProject(
  userId: string,
  projectId: string
) {
  const project = await getProjectWorkspaceId(projectId)
  const workspace = await getWorkspaceById(project.workspaceId)
  if (!workspace) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Workspace not found` })
  }
  if (workspace.isPublic && workspace.publicWritePolicy === `everyone`) {
    await resolveWorkspaceAccess(userId, project.workspaceId)
    return project
  }
  await assertWorkspaceMember(userId, project.workspaceId)
  return project
}

// Update is always tightly scoped — even in public workspaces, only the issue
// creator, a workspace member, or an instance admin may mutate. This is true
// regardless of publicWritePolicy: the policy gates create, not update.
export async function assertCanMutateIssue(userId: string, issueId: string) {
  const issueContext = await getIssueWorkspaceContext(issueId)
  const workspace = await getWorkspaceById(issueContext.workspaceId)
  if (!workspace) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Workspace not found` })
  }

  const member = await getWorkspaceMember(userId, issueContext.workspaceId)
  if (member) return issueContext

  if (workspace.isPublic) {
    const db = await getDb()
    const [issue] = await db
      .select({ creatorId: issues.creatorId })
      .from(issues)
      .where(eq(issues.id, issueId))
      .limit(1)
    if (!issue) {
      throw new TRPCError({ code: `NOT_FOUND`, message: `Issue not found` })
    }
    if (issue.creatorId === userId) return issueContext
    if (await isUserAdmin(userId)) return issueContext
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Only the issue creator or a workspace member can modify this issue`,
    })
  }

  throw new TRPCError({
    code: `FORBIDDEN`,
    message: `Not a member of this workspace`,
  })
}

// Comments are intentionally open: any authed user who can read a workspace
// can comment on its issues. Members can always comment; in public workspaces,
// any authed user can comment regardless of publicWritePolicy.
export async function assertCanCommentInWorkspace(
  userId: string,
  workspaceId: string
) {
  return resolveWorkspaceAccess(userId, workspaceId)
}

// For mutations on workspace-level resources (projects, labels, members, invites).
// In a public workspace, only admins may mutate; in private workspaces, the
// requested role is enforced via assertWorkspaceMember.
export async function assertCanMutateWorkspaceResources(
  userId: string,
  workspaceId: string,
  requiredRoles?: WorkspaceRole[]
) {
  const workspace = await getWorkspaceById(workspaceId)
  if (!workspace) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Workspace not found` })
  }
  if (workspace.isPublic) {
    if (await isUserAdmin(userId)) return
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Only admins can modify the public workspace`,
    })
  }
  await assertWorkspaceMember(userId, workspaceId, requiredRoles)
}

export async function assertIssueLabelWorkspaceMatch(
  userId: string,
  issueId: string,
  labelId: string
) {
  const db = await getDb()
  const [label] = await db
    .select({
      id: labels.id,
      workspaceId: labels.workspaceId,
    })
    .from(labels)
    .where(eq(labels.id, labelId))
    .limit(1)

  const issueContext = await getIssueWorkspaceContext(issueId)
  assertMatchingWorkspaceIds(issueContext.workspaceId, label?.workspaceId)
  // Labels modify the issue, so route through the same gate as issue mutation.
  await assertCanMutateIssue(userId, issueId)

  return {
    issue: issueContext,
    label,
  }
}
