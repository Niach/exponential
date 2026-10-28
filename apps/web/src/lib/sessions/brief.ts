import { TRPCError } from "@trpc/server"
import { and, asc, eq, sql } from "drizzle-orm"
import { composeBrief, type BriefInput } from "@exp/domain-contract/session-brief"
import { db } from "@/db/connection"
import { agents, issues, messages, sessionIssues, sessions, teams, userVoiceSettings, type Session } from "@/db/schema"
import { deriveSessionTitle } from "@/lib/sessions/title"
import type { Db } from "@/lib/sessions/tx"

export { deriveSessionTitle }

// EXP-1291/NEW-1/NEW-2: the server-composed FIRST prompt of a new session
// (`sessions.brief`, fixture-locked by packages/domain-contract
// session-brief.json) plus the system-append layers the device stacks under
// its runbook: the team prompt and, in an agent's conversation tree, the
// agent's server-only instructions (`agents.instructions`, resolved through
// the root's `agent_id`). A session's Issues = ITS OWN rows (root or child
// alike); a root works them itself. A prompt run carries its body inside
// the first message (no Action section any more).

async function firstPersonMessage(sessionId: string): Promise<string | null> {
  const [row] = await db
    .select({ body: messages.body })
    .from(messages)
    .where(and(eq(messages.sessionId, sessionId), eq(messages.authorKind, `user`)))
    .orderBy(asc(messages.createdAt))
    .limit(1)
  return row?.body ?? null
}

async function sessionIssueRows(session: Pick<Session, `id`>) {
  return db
    .select({ identifier: issues.identifier, title: issues.title })
    .from(sessionIssues)
    .innerJoin(issues, eq(issues.id, sessionIssues.issueId))
    .where(eq(sessionIssues.sessionId, session.id))
    .orderBy(asc(sessionIssues.sortOrder), asc(sessionIssues.createdAt))
}

export async function composeSessionBrief(sessionId: string): Promise<{
  brief: string
  teamPrompt: string | null
  agentInstructions: string | null
  parentBranch: string | null
  /** #EXP-1383: the root's agent (null = not an agent conversation tree):
   *  the device tells agent-tree sessions apart by it. */
  rootAgentId: string | null
  /** #NEW-79 (voice-policy.json): this session IS its member's conversation
   *  with the agent picked in Settings → Voice, so the device launches it
   *  under the launcher-enforced tool policy. */
  voicePolicy: boolean
}> {
  const [session] = await db.select().from(sessions).where(eq(sessions.id, sessionId)).limit(1)
  if (!session) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
  const [root] =
    session.rootId === session.id
      ? [session]
      : await db.select().from(sessions).where(eq(sessions.id, session.rootId)).limit(1)
  if (!root) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })

  const input: BriefInput = { text: await firstPersonMessage(session.id) }

  input.issues = await sessionIssueRows(session)

  let parentBranch: string | null = null
  if (session.parentId) {
    const [parent] = await db
      .select()
      .from(sessions)
      .where(eq(sessions.id, session.parentId))
      .limit(1)
    if (parent) {
      parentBranch = parent.branch ?? null
      input.parent = {
        title: await deriveSessionTitle(parent),
        depth: session.depth,
        branch: parentBranch,
      }
    }
  }

  const [team] = await db
    .select({ agentPrompt: teams.agentPrompt })
    .from(teams)
    .where(eq(teams.id, session.teamId))
    .limit(1)
  const [agent] = root.agentId
    ? await db
        .select({ instructions: agents.instructions })
        .from(agents)
        .where(eq(agents.id, root.agentId))
        .limit(1)
    : []

  return {
    brief: composeBrief(input),
    voicePolicy: await isVoiceConversation(session),
    teamPrompt: team?.agentPrompt?.trim() || null,
    agentInstructions: agent?.instructions?.trim() || null,
    parentBranch,
    // #EXP-1383: a conversation's agent, or the agent that started the root.
    rootAgentId: root.agentId ?? root.startedByAgentId ?? null,
  }
}

/** #NEW-79: a ROOT agent conversation whose agent is its member's voice
 *  agent (`user_voice_settings.agent_id`). Children, threads and other
 *  agents' conversations keep today's posture. */
export async function isVoiceConversation(
  session: Pick<Session, `id` | `rootId` | `agentId` | `userId`>,
  database: Db = db
): Promise<boolean> {
  if (session.rootId !== session.id || !session.agentId) return false
  const [voice] = await database
    .select({ agentId: userVoiceSettings.agentId })
    .from(userVoiceSettings)
    .where(eq(userVoiceSettings.userId, session.userId))
    .limit(1)
  return voice?.agentId === session.agentId
}

/** #EXP-1383 (chats.json `agentHint`): the device fetched this session's
 *  brief now, so the agent instructions it launches on are the current ones.
 *  Clients compare it (on an agent chat's root) with
 *  `agents.instructions_updated_at`. `updated_at` stays: a launch is no edit
 *  (the chat list sorts by it, and old desktops/CLIs fetch the brief on every
 *  launch and resume), so the write runs under the transaction-local
 *  `exponential.preserve_timestamps` GUC the `update_updated_at` trigger
 *  honours (0001_triggers.sql, EXP-630). */
export async function stampAgentInstructionsAt(sessionId: string, at: Date = new Date()): Promise<void> {
  await db.transaction(async (tx) => {
    await tx.execute(sql`SELECT set_config('exponential.preserve_timestamps', 'on', true)`)
    await tx.update(sessions).set({ agentInstructionsAt: at }).where(eq(sessions.id, sessionId))
  })
}
