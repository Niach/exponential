import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { issues, labels, projects } from "@/db/schema"
import type { WorkspaceRole } from "@/lib/domain"
import { isUserAdmin } from "@/lib/admin"
import {
  assertMatchingWorkspaceIds,
  assertWorkspaceMember,
  getIssueWorkspaceContext,
  getProjectWorkspaceId,
  getWorkspaceById,
  getWorkspaceMember,
  resolveWorkspaceAccess,
} from "./membership"

async function getDb() {
  const { db } = await import(`@/db/connection`)
  return db
}

// A user can approve an agent plan if they are a workspace owner OR the
// issue's creator. Regular members can comment to refine but cannot approve.
export async function assertCanApprovePlan(userId: string, issueId: string) {
  const db = await getDb()
  const [row] = await db
    .select({
      issueId: issues.id,
      projectId: issues.projectId,
      workspaceId: projects.workspaceId,
      creatorId: issues.creatorId,
    })
    .from(issues)
    .innerJoin(projects, eq(issues.projectId, projects.id))
    .where(eq(issues.id, issueId))
    .limit(1)
  if (!row) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Issue not found` })
  }
  if (row.creatorId === userId) return row
  const member = await getWorkspaceMember(userId, row.workspaceId)
  if (member && member.role === `owner`) return row
  throw new TRPCError({
    code: `FORBIDDEN`,
    message: `Only the issue creator or a workspace owner can approve the plan`,
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
