import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray, isNull, sql } from "drizzle-orm"
import { composeBrief, type BriefInput } from "@exp/domain-contract/session-brief"
import { db } from "@/db/connection"
import { agents, attachments, issues, messages, sessionIssues, sessions, teams, userVoiceSettings, type Session } from "@/db/schema"
import { ASSISTANT } from "@exp/domain-contract/agent-settings"
import { voiceAgentIsPrivate } from "@/lib/sessions/meeting-access"
import { deriveSessionTitle } from "@/lib/sessions/title"
import type { Db } from "@/lib/sessions/tx"
import { inSession, pendingInput } from "@/lib/sessions/messages"
import { isSealedBody } from "@/lib/sessions/sealed"
import { attachmentLine } from "@/lib/sessions/attachment-lines"

export { deriveSessionTitle }

// EXP-1291/NEW-1/NEW-2: the server-composed FIRST prompt of a new session
// (`sessions.brief`, fixture-locked by packages/domain-contract
// session-brief.json) plus the system-append layers the device stacks under
// its runbook: the team prompt and, in an agent's conversation tree, the
// agent's server-only instructions (`agents.instructions`, resolved through
// the root's `agent_id`). A session's Issues = ITS OWN rows (root or child
// alike); a root works them itself. A prompt run carries its body inside
// the first message (no Action section any more).

/** The session's first input: a person's message, or (#EXP-1452) the
 *  message of the agent session that started it. */
async function firstPersonMessage(sessionId: string): Promise<{ body: string | null; attachmentIds: string[] }> {
  const [row] = await db
    .select({ body: messages.body, attachmentIds: messages.attachmentIds })
    .from(messages)
    .where(and(inSession(sessionId), isNull(messages.card), pendingInput()))
    .orderBy(asc(messages.createdAt))
    .limit(1)
  return { body: row?.body ?? null, attachmentIds: row?.attachmentIds ?? [] }
}

/** FEED-180: the attachment lines of a sealed first message, in its order
 *  (a plaintext body already carries them, `claimAttachments`). */
async function sealedAttachmentLines(ids: string[]): Promise<string[]> {
  if (ids.length === 0) return []
  const rows = await db
    .select({ id: attachments.id, filename: attachments.filename, contentType: attachments.contentType })
    .from(attachments)
    .where(inArray(attachments.id, ids))
  const byId = new Map(rows.map((row) => [row.id, row]))
  return ids.flatMap((id) => {
    const row = byId.get(id)
    return row ? [attachmentLine(row)] : []
  })
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
  /** This session IS its member's conversation with the agent picked in
   *  Settings → Voice. It runs with permissions bypassed like every other
   *  session (the voice tool policy is retired); the device reads it only
   *  as `screenMemory`'s precondition. */
  voicePolicy: boolean
  /** EXP-1458: the first person message when it is SEALED (its body
   *  envelope, untouched): `brief` was then composed with no text and the
   *  device opens this itself and composes the brief with it. Null =
   *  plaintext (or no message): `brief` is complete. */
  sealedText: string | null
  /** FEED-180: `sealedText`'s attachments as the lines a plaintext body
   *  gets appended (fixtures/session-brief.json); the device appends them
   *  to the opened text. Empty for a plaintext message. */
  sealedAttachments: string[]
  /** NEW-58 (screen-memory.json TOOL): `voicePolicy` AND the conversation is
   *  private to its member (the agent on their own device shared with no
   *  team, the conversation not `shared`): only then may the device attach
   *  `get_screenshots`. */
  screenMemory: boolean
}> {
  const [session] = await db.select().from(sessions).where(eq(sessions.id, sessionId)).limit(1)
  if (!session) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
  const [root] =
    session.rootId === session.id
      ? [session]
      : await db.select().from(sessions).where(eq(sessions.id, session.rootId)).limit(1)
  if (!root) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })

  // EXP-1458: the server never reads a sealed first message.
  const first = await firstPersonMessage(session.id)
  const sealedText = isSealedBody(first.body) ? first.body : null
  const sealedAttachments = sealedText ? await sealedAttachmentLines(first.attachmentIds) : []
  const input: BriefInput = { text: sealedText ? null : first.body }

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
        .select({ instructions: agents.instructions, role: agents.role })
        .from(agents)
        .where(eq(agents.id, root.agentId))
        .limit(1)
    : []

  const voicePolicy = await isVoiceConversation(session)
  return {
    brief: composeBrief(input),
    sealedText,
    sealedAttachments,
    voicePolicy,
    screenMemory: voicePolicy && !session.shared && (await voiceAgentIsPrivate(db, session.userId, session.agentId!)),
    teamPrompt: team?.agentPrompt?.trim() || null,
    // #EXP-1439: an Assistant's turns run on the built-in prompt ONLY
    // (agent-settings.json `assistant.prompt`); a team agent on its own text.
    agentInstructions:
      agent?.role === `assistant` ? ASSISTANT.prompt.text : agent?.instructions?.trim() || null,
    parentBranch,
    // #EXP-1383: a conversation's agent, or the agent that started the root.
    rootAgentId: root.agentId ?? root.startedByAgentId ?? null,
  }
}

/** A ROOT agent conversation whose agent is its member's voice agent
 *  (`user_voice_settings.agent_id`): screen memory's precondition, and only
 *  its member answers its cards. Children, threads and other agents'
 *  conversations never count. */
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
