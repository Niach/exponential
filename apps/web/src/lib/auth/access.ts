// Canonical authorization predicates. This is the single authority for "can
// user X do Y" decisions in the tRPC layer (and the MCP tool layer); the older
// scattered `assertCan*` helpers collapsed into the two capability-driven
// predicates below. Low-level data lookups and the member-role assertion still
// live in `./membership`.
//
// v7: workspace-level publicness is gone. Membership (always an explicit
// invite) is the only capability gate — public feedback boards are read-only
// for non-members (anonymous reads happen in the shape proxies; writes arrive
// only via the embedded widget's server-side service). The old
// public-workspace moderation clamp is deleted with the self-joined-member
// class it existed for.

import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { labels } from "@/db/schema"
import type { WorkspaceRole } from "@/lib/domain"
import {
  assertMatchingWorkspaceIds,
  assertWorkspaceAccess,
  getIssueWorkspaceContext,
  getWorkspaceById,
  getWorkspaceMember,
} from "./membership"

async function getDb() {
  const { db } = await import(`@/db/connection`)
  return db
}

// ---------------------------------------------------------------------------
// Workspace-scoped capabilities
// ---------------------------------------------------------------------------

// - `read` / `comment` / `create_issue`: any member.
// - `mutate_resources`: workspace-level resources (projects, labels, members,
//   invites) — a member with the required role.
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
    case `comment`:
    case `create_issue`: {
      assertWorkspaceAccess(member)
      return { kind: `member` as const, workspace, member }
    }
    case `mutate_resources`: {
      assertWorkspaceAccess(member, opts?.roles)
      return { kind: `member` as const, workspace, member }
    }
  }
}

// ---------------------------------------------------------------------------
// Issue-scoped actions
// ---------------------------------------------------------------------------

// - `read` / `write` / `delete`: any member of the issue's workspace.
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
      const member = await getWorkspaceMember(userId, issueContext.workspaceId)
      assertWorkspaceAccess(member)
      return issueContext
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
