import { and, desc, eq, gt, inArray, isNotNull, isNull, ne, or, sql } from "drizzle-orm"
import { ASSISTANT } from "@exp/domain-contract/agent-settings"
import type { MessageCard } from "@exp/db-schema/domain"
import { agents, messages, sessions } from "@/db/schema"
import { pendingInput } from "@/lib/sessions/messages"
import type { Tx } from "@/lib/sessions/tx"

// #EXP-1439 (agent-settings.json `assistant.chat`, the spec): the Assistant's
// conversation stays quiet. Between two of the member's messages it holds
// at most ONE live status card (every surface it shows edits that card in
// place) and a text only in a turn the member started; a decision is an
// ask (with a push). Routine bus lines (a PR opened, a session claimed)
// never land in it: its next check-in reads them.

/** The bus lines an Assistant conversation never receives. */
export const ASSISTANT_SILENT_EVENTS: ReadonlySet<string> = new Set(ASSISTANT.chat.silentEvents)

/** `session` is a member's Assistant conversation: a ROOT whose agent is
 *  an Assistant. */
export async function isAssistantConversation(
  tx: Pick<Tx, `select`>,
  session: { id: string; rootId: string; agentId: string | null }
): Promise<boolean> {
  if (!session.agentId || session.id !== session.rootId) return false
  const [row] = await tx
    .select({ role: agents.role })
    .from(agents)
    .where(eq(agents.id, session.agentId))
    .limit(1)
  return row?.role === `assistant`
}

/** Of `agentIds`, the Assistants. */
export async function assistantAgentIds(tx: Pick<Tx, `select`>, agentIds: readonly string[]): Promise<Set<string>> {
  const ids = [...new Set(agentIds)]
  if (ids.length === 0) return new Set()
  const rows = await tx
    .select({ id: agents.id })
    .from(agents)
    .where(and(inArray(agents.id, ids), eq(agents.role, `assistant`)))
  return new Set(rows.map((row) => row.id))
}

/** A row the MEMBER wrote: a person's message (not a trigger tick, which
 *  is person-authored with an `event` card) or their answer to one of the
 *  conversation's own cards. */
function memberInput(sessionId: string) {
  return or(
    and(
      eq(messages.authorKind, `user`),
      isNull(messages.authorSessionId),
      or(isNull(messages.card), sql`${messages.card}->>'kind' <> 'event'`)
    ),
    and(
      eq(messages.authorSessionId, sessionId),
      isNotNull(messages.answeredAt),
      sql`${messages.card}->>'kind' IN ('question', 'plan')`
    )
  )!
}

/** The current turn was started by the member: among the inputs the device
 *  delivered LAST (one `consumed_at`), one is theirs. A turn woken by a
 *  tick or a child's line only is not. */
export async function turnHasMemberInput(tx: Pick<Tx, `select`>, sessionId: string): Promise<boolean> {
  const [delivered] = await tx
    .select({ at: messages.consumedAt })
    .from(messages)
    .where(and(eq(messages.sessionId, sessionId), pendingInput(), isNotNull(messages.consumedAt)))
    .orderBy(desc(messages.consumedAt))
    .limit(1)
  if (!delivered?.at) return false
  const [mine] = await tx
    .select({ id: messages.id })
    .from(messages)
    .where(and(eq(messages.sessionId, sessionId), eq(messages.consumedAt, delivered.at), memberInput(sessionId)))
    .limit(1)
  return !!mine
}

/** THE status card a new surface edits: the conversation's newest own
 *  surface card since the member last wrote (null = post a new one). */
export async function statusCardTarget(
  tx: Pick<Tx, `select`>,
  sessionId: string
): Promise<{ id: string; card: MessageCard } | null> {
  const [last] = await tx
    .select({ at: messages.createdAt })
    .from(messages)
    .where(and(eq(messages.sessionId, sessionId), memberInput(sessionId)))
    .orderBy(desc(messages.createdAt))
    .limit(1)
  const [row] = await tx
    .select({ id: messages.id, card: messages.card })
    .from(messages)
    .where(
      and(
        eq(messages.sessionId, sessionId),
        eq(messages.authorSessionId, sessionId),
        sql`${messages.card}->>'kind' = 'surface'`,
        ...(last ? [gt(messages.createdAt, last.at)] : [])
      )
    )
    .orderBy(desc(messages.createdAt))
    .limit(1)
    .for(`update`)
  return row?.card ? { id: row.id, card: row.card } : null
}

/** A scheduled check-in has something to look at: any of the member's
 *  OTHER sessions in the team (their threads, their agents' conversations
 *  and every child) moved since `since`. */
export async function memberChatsChanged(
  tx: Pick<Tx, `select`>,
  agent: { id: string; teamId: string; ownerId: string },
  since: Date
): Promise<boolean> {
  const [moved] = await tx
    .select({ id: sessions.id })
    .from(sessions)
    .where(
      and(
        eq(sessions.teamId, agent.teamId),
        eq(sessions.userId, agent.ownerId),
        // Its own conversation's turns are not news; its children are.
        or(isNull(sessions.agentId), ne(sessions.agentId, agent.id), ne(sessions.id, sessions.rootId)),
        gt(sessions.lastActivityAt, since)
      )
    )
    .limit(1)
  return !!moved
}
