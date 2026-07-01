// Canonical authorization predicates. This is the single authority for "can
// user X do Y" decisions in the tRPC layer (and the MCP tool layer); the older
// scattered `assertCan*` helpers collapsed into the two capability-driven
// predicates below plus the moderation clamp. Low-level data lookups and the
// member-role assertion still live in `./membership`.
//
// Behaviour is intentionally identical to the helpers it replaces — see the
// per-branch comments for the rules.

import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { contract } from "@exp/domain-contract"
import { issues, labels } from "@/db/schema"
import type { WorkspaceRole } from "@/lib/domain"
import { isUserAdmin } from "@/lib/admin"
import {
  assertMatchingWorkspaceIds,
  assertWorkspaceAccess,
  getIssueWorkspaceContext,
  getWorkspaceById,
  getWorkspaceMember,
  isWorkspaceModerator,
} from "./membership"

async function getDb() {
  const { db } = await import(`@/db/connection`)
  return db
}

// ---------------------------------------------------------------------------
// Workspace-scoped capabilities
// ---------------------------------------------------------------------------

// - `read` / `comment`: any member, or any authed user in a public workspace.
//   (Comments are intentionally as open as read access.)
// - `create_issue`: a public workspace with publicWritePolicy=`everyone` lets
//   any authed user file; otherwise the caller must be a member.
// - `mutate_resources`: workspace-level resources (projects, labels, members,
//   invites). On the public workspace only instance admins may write; on a
//   private workspace the caller must be a member with the required role.
export type WorkspaceCapability =
  | `read`
  | `comment`
  | `create_issue`
  | `mutate_resources`

export async function resolveWorkspaceAccess(
  userId: string,
  workspaceId: string,
  capability: WorkspaceCapability = `read`,
  opts?: { roles?: WorkspaceRole[] }
) {
  const workspace = await getWorkspaceById(workspaceId)
  if (!workspace) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Workspace not found` })
  }
  const member = await getWorkspaceMember(userId, workspaceId)

  switch (capability) {
    case `read`:
    case `comment`: {
      if (member) return { kind: `member` as const, workspace, member }
      if (workspace.isPublic) return { kind: `public` as const, workspace }
      throw new TRPCError({
        code: `FORBIDDEN`,
        message: `Not a member of this workspace`,
      })
    }
    case `create_issue`: {
      if (workspace.isPublic && workspace.publicWritePolicy === `everyone`) {
        if (member) return { kind: `member` as const, workspace, member }
        return { kind: `public` as const, workspace }
      }
      assertWorkspaceAccess(member)
      return { kind: `member` as const, workspace, member }
    }
    case `mutate_resources`: {
      if (workspace.isPublic) {
        if (await isUserAdmin(userId)) {
          return { kind: `admin` as const, workspace }
        }
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only admins can modify the public workspace`,
        })
      }
      assertWorkspaceAccess(member, opts?.roles)
      return { kind: `member` as const, workspace, member }
    }
  }
}

// ---------------------------------------------------------------------------
// Issue-scoped actions
// ---------------------------------------------------------------------------

// - `read`: anyone who can read the workspace.
// - `write` / `delete`: member, OR (in a public workspace) the issue creator or
//   an instance admin. publicWritePolicy gates create, not mutation.
export type IssueAction = `read` | `write` | `delete`

export async function assertIssueAccess(
  userId: string,
  issueId: string,
  action: IssueAction
) {
  const issueContext = await getIssueWorkspaceContext(issueId)

  switch (action) {
    case `read`: {
      await resolveWorkspaceAccess(userId, issueContext.workspaceId, `read`)
      return issueContext
    }
    case `write`:
    case `delete`: {
      const workspace = await getWorkspaceById(issueContext.workspaceId)
      if (!workspace) {
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Workspace not found`,
        })
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
  }
}

// Toggling a label both verifies the label and issue share a workspace and that
// the caller may mutate the issue. Returns both contexts for the caller's write.
export async function assertIssueLabelWorkspaceMatch(
  userId: string,
  issueId: string,
  labelId: string
) {
  const db = await getDb()
  const [label] = await db
    .select({ id: labels.id, workspaceId: labels.workspaceId })
    .from(labels)
    .where(eq(labels.id, labelId))
    .limit(1)

  const issueContext = await getIssueWorkspaceContext(issueId)
  assertMatchingWorkspaceIds(issueContext.workspaceId, label?.workspaceId)
  await assertIssueAccess(userId, issueId, `write`)

  return { issue: issueContext, label }
}

// ---------------------------------------------------------------------------
// Public-workspace moderation clamp
// ---------------------------------------------------------------------------

// Fields a non-moderator may NOT set on issues in a PUBLIC workspace. Title,
// description and labels stay open; everything listed here is moderation-gated
// and is clamped (on create) or stripped (on update) server-side so a stale or
// tampered client cannot bypass the UI restrictions. Single source of truth:
// packages/domain-contract/contract.json (also generated into the native
// WorkspacePermissions so they can't hand-drift from the server).
export const MODERATION_RESTRICTED_FIELDS = contract.moderationRestrictedFields

// True when the user is a non-moderator acting in a public workspace, so the
// moderation-gated fields must be clamped (create) or stripped (update).
export async function isModerationRestricted(
  userId: string,
  workspaceId: string
): Promise<boolean> {
  const workspace = await getWorkspaceById(workspaceId)
  if (!workspace?.isPublic) return false
  return !(await isWorkspaceModerator(userId, workspaceId))
}

// Remove the moderation-gated fields from an update payload in place.
export function applyModerationRestrictions(updates: Record<string, unknown>) {
  for (const field of MODERATION_RESTRICTED_FIELDS) {
    delete updates[field]
  }
}
