import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { alias } from "drizzle-orm/pg-core"
import agentSettingsFixture from "@exp/domain-contract/fixtures/agent-settings.json"
import { agents, issues, sessions } from "@/db/schema"
import { emitIssueEventInTx } from "@/lib/events"
import { nudgeAgentHost, postAgentInputInTx } from "@/lib/agents/triggers"
import type { IssueEventOrigin } from "@/lib/integrations/event-origin"
import type { Tx } from "@/lib/sessions/tx"

// #EXP-1383 (agent-settings.json `assignee`): an issue ASSIGNED to someone
// emits the builtin team event `issue.assigned` {issueId, identifier, title,
// assigneeId, assigneeAgentId}; assigned to a team AGENT it also posts a
// NUDGE into the assigner's conversation with that agent (created when
// missing; the trigger-firing card with `copy.nudge` as its line and the
// issue as its entity) and pokes its device. Same transaction as the write.
// An agent assigning ITSELF never nudges itself; an agent session assigns
// as its member (the session's user).

export const ASSIGNEE_NUDGE_COPY = agentSettingsFixture.assignee.copy.nudge

export function assigneeNudgeLine(issue: { identifier: string; title: string }): string {
  return ASSIGNEE_NUDGE_COPY.replace(`{identifier}`, issue.identifier).replace(`{title}`, issue.title)
}

/** The agent `agentId` when it belongs to `teamId` and `actorUserId` may
 *  hand it work (BAD_REQUEST else): S-3, another member's Assistant is
 *  theirs alone (agent-settings.json `assistant`), never an assignee. */
export async function assertAgentInTeam(
  database: Pick<Tx, `select`>,
  agentId: string,
  teamId: string,
  actorUserId: string
): Promise<void> {
  const [row] = await database
    .select({ teamId: agents.teamId, role: agents.role, ownerId: agents.ownerId })
    .from(agents)
    .where(eq(agents.id, agentId))
    .limit(1)
  if (!row || row.teamId !== teamId) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `The assignee agent must belong to the issue's team`,
    })
  }
  if (row.role === `assistant` && row.ownerId !== actorUserId) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Another member's Assistant can't be assigned`,
    })
  }
}

/** The session behind an origin: its member and its root's agent. Only a
 *  verified origin counts (`agentId` null = the caller's session is none or
 *  not an agent's, so the person assigns as themselves). */
async function originActor(
  tx: Tx,
  origin: IssueEventOrigin | undefined
): Promise<{ userId: string; agentId: string | null } | null> {
  if (!origin?.sessionId || origin.agentId === null) return null
  const root = alias(sessions, `assign_root`)
  const [row] = await tx
    .select({ userId: sessions.userId, agentId: root.agentId, startedByAgentId: root.startedByAgentId })
    .from(sessions)
    .leftJoin(root, eq(root.id, sessions.rootId))
    .where(eq(sessions.id, origin.sessionId))
    .limit(1)
  if (!row) return null
  const agentId = origin.agentId ?? row.agentId ?? row.startedByAgentId ?? null
  return agentId ? { userId: row.userId, agentId } : null
}

export async function onIssueAssignedInTx(
  tx: Tx,
  args: {
    issueId: string
    teamId: string
    actorUserId: string | null
    origin?: IssueEventOrigin
    assigneeId: string | null
    assigneeAgentId: string | null
  }
): Promise<void> {
  if (!args.assigneeId && !args.assigneeAgentId) return
  const event = await emitIssueEventInTx(tx, {
    issueId: args.issueId,
    teamId: args.teamId,
    name: `issue.assigned`,
    payload: { assigneeId: args.assigneeId, assigneeAgentId: args.assigneeAgentId },
    emittedBy: args.origin?.sessionId
      ? { sessionId: args.origin.sessionId }
      : args.actorUserId
        ? { userId: args.actorUserId }
        : { server: true },
  })
  if (!args.assigneeAgentId) return
  const acting = await originActor(tx, args.origin)
  if (acting?.agentId === args.assigneeAgentId) return
  const assignerId = acting?.userId ?? args.actorUserId
  if (!assignerId) return
  const [agent] = await tx.select().from(agents).where(eq(agents.id, args.assigneeAgentId)).limit(1)
  if (!agent || agent.teamId !== args.teamId) return
  // S-3: never a turn for one member on another member's Assistant.
  if (agent.role === `assistant` && agent.ownerId !== assignerId) return
  const [issue] = await tx
    .select({ identifier: issues.identifier, title: issues.title })
    .from(issues)
    .where(eq(issues.id, args.issueId))
    .limit(1)
  if (!issue) return
  try {
    // A savepoint: a refused nudge never poisons the assignment's write.
    const host = await tx.transaction((sp) =>
      postAgentInputInTx(sp, agent, assignerId, assigneeNudgeLine(issue), {
        triggerId: null,
        eventId: event?.id ?? null,
        name: `issue.assigned`,
        issueId: args.issueId,
      })
    )
    nudgeAgentHost(host)
  } catch (error) {
    // The agent's device is not reachable for this member (unshared, gone):
    // the assignment stands, the agent learns of it from its own reads.
    console.warn(`[agent-assign] nudge of agent ${agent.id} for issue ${args.issueId} not posted:`, error)
  }
}
