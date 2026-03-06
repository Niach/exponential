import { TRPCError } from "@trpc/server"
import { and, eq, inArray } from "drizzle-orm"
import { attachments, issues, labels, projects, workspaceMembers } from "@/db/schema"
import type { WorkspaceMember } from "@/db/schema"
import type { WorkspaceRole } from "@/lib/domain"

type WorkspaceMemberRecord = Pick<WorkspaceMember, `role` | `userId` | `workspaceId`>

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
    .where(inArray(projects.workspaceId, workspaceIds))

  return rows.map((row) => row.id)
}

export async function getUserLabelIds(userId: string): Promise<string[]> {
  const workspaceIds = await getUserWorkspaceIds(userId)

  if (workspaceIds.length === 0) {
    return []
  }

  const db = await getDb()
  const rows = await db
    .select({ id: labels.id })
    .from(labels)
    .where(inArray(labels.workspaceId, workspaceIds))

  return rows.map((row) => row.id)
}

export async function getUserIdsInWorkspaces(userId: string): Promise<string[]> {
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

export async function getWorkspaceMember(
  userId: string,
  workspaceId: string
) {
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

export async function assertWorkspaceOwner(userId: string, workspaceId: string) {
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
  await assertWorkspaceMember(userId, issueContext.workspaceId)

  return {
    issue: issueContext,
    label,
  }
}
