// Canonical authorization predicates. This is the single authority for "can
// user X do Y" decisions in the tRPC layer (and the MCP tool layer); the older
// scattered `assertCan*` helpers collapsed into the two capability-driven
// predicates below. Low-level data lookups and the member-role assertion still
// live in `./membership`.
//
// Membership (always an explicit invite) is the only capability gate —
// nothing is anonymously readable (EXP-180 removed public boards); anonymous
// writes arrive only via the embedded widget's server-side service.

import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { labels } from "@/db/schema"
import type { TeamRole } from "@/lib/domain"
import {
  assertMatchingTeamIds,
  assertTeamAccess,
  getIssueTeamContext,
  getTeamById,
  getTeamMember,
} from "./membership"

async function getDb() {
  const { db } = await import(`@/db/connection`)
  return db
}

// ---------------------------------------------------------------------------
// Team-scoped capabilities
// ---------------------------------------------------------------------------

// - `read` / `comment` / `create_issue`: any member.
// - `mutate_resources`: team-level resources (boards, labels, members,
//   invites) — a member with the required role.
export type TeamCapability =
  | `read`
  | `comment`
  | `create_issue`
  | `mutate_resources`

export async function resolveTeamAccess(
  userId: string,
  teamId: string,
  capability: TeamCapability = `read`,
  opts?: { roles?: TeamRole[] }
) {
  const team = await getTeamById(teamId)
  if (!team) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Team not found` })
  }
  const member = await getTeamMember(userId, teamId)

  switch (capability) {
    case `read`:
    case `comment`:
    case `create_issue`: {
      assertTeamAccess(member)
      return { kind: `member` as const, team, member }
    }
    case `mutate_resources`: {
      assertTeamAccess(member, opts?.roles)
      return { kind: `member` as const, team, member }
    }
  }
}

// ---------------------------------------------------------------------------
// Issue-scoped actions
// ---------------------------------------------------------------------------

// - `read` / `write` / `delete`: any member of the issue's team.
export type IssueAction = `read` | `write` | `delete`

export async function assertIssueAccess(
  userId: string,
  issueId: string,
  action: IssueAction
) {
  const issueContext = await getIssueTeamContext(issueId)
  switch (action) {
    case `read`: {
      await resolveTeamAccess(userId, issueContext.teamId, `read`)
      return issueContext
    }
    case `write`:
    case `delete`: {
      const member = await getTeamMember(userId, issueContext.teamId)
      assertTeamAccess(member)
      return issueContext
    }
  }
}

// Toggling a label both verifies the label and issue share a team and that
// the caller may mutate the issue. Returns both contexts for the caller's write.
export async function assertIssueLabelTeamMatch(
  userId: string,
  issueId: string,
  labelId: string
) {
  const db = await getDb()
  const [label] = await db
    .select({ id: labels.id, teamId: labels.teamId })
    .from(labels)
    .where(eq(labels.id, labelId))
    .limit(1)

  const issueContext = await getIssueTeamContext(issueId)
  assertMatchingTeamIds(issueContext.teamId, label?.teamId)
  await assertIssueAccess(userId, issueId, `write`)

  return { issue: issueContext, label }
}
