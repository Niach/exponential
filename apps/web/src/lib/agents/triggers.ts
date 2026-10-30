import { and, desc, eq, gt, ne, sql } from "drizzle-orm"
import { memberChatsChanged } from "@/lib/agents/assistant-chat"
import type { AgentTrigger, AgentTriggerState } from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"
import { db } from "@/db/connection"
import { agents, conversations, messages, sessions, type Agent, type Event, type Session } from "@/db/schema"
import { conversationInTx } from "@/lib/agents/conversation"
import { latestCronMatch, parseCron } from "@/lib/agents/cron"
import { askHidden } from "@/lib/sessions/bus"
import { insertMessage } from "@/lib/sessions/messages"
import type { Tx } from "@/lib/sessions/tx"
import { getSteerRelayConfig, relayPostNudge } from "@/lib/steer"

// NEW-2: the SERVER fires an agent's triggers (the device armed nothing
// since migration 0182). A firing = a message into the agent OWNER's
// conversation session, authored by the owner (so every device delivers it
// as pending input) and carrying the `trigger_fired` event card. Schedules
// are evaluated every watchdog tick: the window since the last tick (capped
// at `agentTrigger.catchUpHours`) is searched for the latest matching
// minute, so a server that was down over a due time fires ONCE when it is
// back (#FEED-124: caught up, never lost) and a tick never fires twice. A
// schedule never stacks: while its last firing is still unread the tick is
// skipped (S-3: the due schedules of a tick fold into ONE edited tick card;
// a tick with nothing moved in the agent's trees is skipped). Event
// triggers fire as the `events` row lands (`lib/events.ts`),
// never for the agent whose own session emitted it. #EXP-1409: an event
// about a private session (`EventPrivateDetail`) posts its full payload
// only into a conversation whose readers may see that session (the
// #EXP-1399 `askHidden` rule); every other firing quotes the row's own
// ids-only payload.

const HOUR = 60 * 60 * 1000
const MIN = 60 * 1000

export const TRIGGER_COPY = {
  schedule: (cron: string, tz: string | null, caughtUp: boolean) =>
    `Schedule \`${cron}\`${tz ? ` (${tz})` : ``} fired${caughtUp ? ` (caught up: this tick was missed earlier)` : ``}.`,
  event: (event: Pick<Event, `name` | `payload` | `emittedBy`>) =>
    `Event \`${event.name}\` (from ${event.emittedBy === `session` ? `a session` : event.emittedBy === `user` ? `a person` : `the server`}):\n\n\`\`\`json\n${JSON.stringify(event.payload, null, 2)}\n\`\`\``,
} as const

export function enabledTriggers(agent: Pick<Agent, `triggers`>): AgentTrigger[] {
  return (agent.triggers ?? []).filter(
    (trigger) => trigger && typeof trigger === `object` && trigger.enabled !== false && typeof trigger.id === `string`
  )
}

/** The full payload of an event about a private session (`origin`), for
 *  the firings whose readers may see it (`EmitEventArgs.privateDetail`). */
export interface EventPrivateDetail {
  origin: Pick<Session, `shared` | `userId`>
  payload: Record<string, unknown>
}

/** Posts a firing into the owner's conversation (pending input). */
async function postFiring(
  tx: Tx,
  agent: Agent,
  trigger: AgentTrigger,
  body: string | ((conversation: Session) => string),
  event: Event | null
): Promise<{ ownerId: string; deviceId: string } | null> {
  return postAgentInputInTx(tx, agent, agent.ownerId, body, {
    triggerId: trigger.id,
    eventId: event?.id ?? null,
    name: event?.name ?? null,
  })
}

/** A `trigger_fired` card + its line as pending input in `userId`'s
 *  conversation with the agent (created when missing), authored by that
 *  member so every device delivers it. Returns the host to nudge
 *  (`nudgeAgentHost`). #EXP-1383: an assignment's nudge rides it too
 *  (no trigger, the issue as its entity). */
export async function postAgentInputInTx(
  tx: Tx,
  agent: Agent,
  userId: string,
  body: string | ((conversation: Session) => string),
  card: { triggerId: string | null; eventId: string | null; name: string | null; issueId?: string | null }
): Promise<{ ownerId: string; deviceId: string } | null> {
  const { session, created } = await conversationInTx(tx, agent, userId)
  await insertMessage(tx, session, {
    authorKind: `user`,
    authorId: userId,
    body: typeof body === `function` ? body(session) : body,
    card: {
      kind: `event`,
      event: `trigger_fired`,
      agentId: agent.id,
      triggerId: card.triggerId,
      eventId: card.eventId,
      name: card.name,
      ...(card.issueId ? { issueId: card.issueId } : {}),
    },
  })
  return created ?? { ownerId: agent.ownerId, deviceId: agent.deviceId }
}

function nudge(host: { ownerId: string; deviceId: string } | null) {
  const config = getSteerRelayConfig()
  if (!config || !host) return
  void relayPostNudge(config, host.ownerId, host.deviceId)
}

/** Wakes the device that runs a firing (best-effort relay poke). */
export function nudgeAgentHost(host: { ownerId: string; deviceId: string } | null) {
  nudge(host)
}

/** S-3: the owner's schedule TICK card the next firing folds into: the
 *  unread one, else the newest one when no person message came after it
 *  (then it is re-armed: unread again). Null = post a new card. */
async function foldableTick(tx: Tx, agent: Agent): Promise<{ id: string; pending: boolean } | null> {
  const [row] = await tx
    .select({ id: messages.id, consumedAt: messages.consumedAt, conversationId: messages.conversationId, createdAt: messages.createdAt })
    .from(messages)
    .innerJoin(conversations, eq(conversations.id, messages.conversationId))
    .innerJoin(sessions, eq(sessions.id, conversations.sessionId))
    .where(
      and(
        eq(sessions.agentId, agent.id),
        eq(sessions.userId, agent.ownerId),
        sql`${messages.card}->>'event' = 'trigger_fired' AND ${messages.card}->>'eventId' IS NULL AND ${messages.card}->>'triggerId' IS NOT NULL`
      )
    )
    .orderBy(desc(messages.createdAt))
    .limit(1)
  if (!row) return null
  if (row.consumedAt === null) return { id: row.id, pending: true }
  const [later] = await tx
    .select({ id: messages.id })
    .from(messages)
    .where(
      and(
        eq(messages.conversationId, row.conversationId),
        eq(messages.authorKind, `user`),
        ne(messages.id, row.id),
        gt(messages.createdAt, row.createdAt)
      )
    )
    .limit(1)
  return later ? null : { id: row.id, pending: false }
}

/** S-3: whether anything moved in the agent's watched trees (the threads
 *  it started, the sub-sessions of its conversations) since `since`. An
 *  agent with no tree at all always counts as changed (nothing to judge
 *  by: its schedule is its own work). */
async function treesChanged(tx: Tx, agent: Agent, since: Date): Promise<boolean> {
  const trees = sql`(
    ${sessions.rootId} IN (SELECT id FROM sessions r WHERE r.started_by_agent_id = ${agent.id})
    OR (${sessions.rootId} IN (SELECT id FROM sessions c WHERE c.agent_id = ${agent.id}) AND ${sessions.id} <> ${sessions.rootId})
  )`
  const [any] = await tx
    .select({ id: sessions.id })
    .from(sessions)
    .where(and(eq(sessions.teamId, agent.teamId), trees))
    .limit(1)
  if (!any) return true
  const [moved] = await tx
    .select({ id: sessions.id })
    .from(sessions)
    .where(and(eq(sessions.teamId, agent.teamId), trees, gt(sessions.lastActivityAt, since)))
    .limit(1)
  return !!moved
}

/** One tick of every agent's schedules. Returns the firings posted. S-3:
 *  every schedule due this tick folds into ONE tick card per agent
 *  conversation, edited in place while it is unread (or the newest thing no
 *  person message answered: then it is re-armed), and a tick is skipped
 *  when nothing moved in the agent's watched trees since its last firing.
 *  Never stacks: an unread tick only takes the newer text. */
export async function fireScheduledTriggers(now: Date = new Date()): Promise<number> {
  const catchUpSince = new Date(now.getTime() - contract.agentTrigger.catchUpHours * HOUR)
  const hosts: Array<{ ownerId: string; deviceId: string } | null> = []
  const fired = await db.transaction(async (tx) => {
    const rows = await tx
      .select()
      .from(agents)
      .where(sql`${agents.triggers} @> '[{"kind": "schedule", "enabled": true}]'::jsonb`)
      .for(`update`, { skipLocked: true })
    let count = 0
    for (const agent of rows) {
      const state: AgentTriggerState = { ...(agent.triggerState ?? {}) }
      let changed = false
      const due: Array<{ trigger: AgentTrigger; cron: string; tz: string | null; at: Date; caughtUp: boolean }> = []
      let lastFired: Date | null = null
      for (const trigger of enabledTriggers(agent)) {
        if (trigger.kind !== `schedule`) continue
        const fields = parseCron(trigger.cron)
        if (!fields) continue
        const entry = { ...(state[trigger.id] ?? {}) }
        const firedAt = entry.firedAt ? new Date(entry.firedAt) : null
        if (firedAt && !Number.isNaN(firedAt.getTime()) && (!lastFired || firedAt > lastFired)) lastFired = firedAt
        const checkedAt = entry.checkedAt ? new Date(entry.checkedAt) : null
        entry.checkedAt = now.toISOString()
        state[trigger.id] = entry
        changed = true
        // A trigger seen for the first time is ARMED: it fires from the
        // next due minute on, never retroactively.
        if (!checkedAt || Number.isNaN(checkedAt.getTime())) continue
        const after = checkedAt > catchUpSince ? checkedAt : catchUpSince
        const at = latestCronMatch(fields, after, now, trigger.tz ?? null)
        if (!at) continue
        due.push({ trigger, cron: trigger.cron, tz: trigger.tz ?? null, at, caughtUp: now.getTime() - at.getTime() > 2 * MIN })
      }
      // #EXP-1439: an Assistant watches ALL its member's chats in the team.
      const moved = (since: Date) =>
        agent.role === `assistant` ? memberChatsChanged(tx, agent, since) : treesChanged(tx, agent, since)
      if (due.length > 0 && (!lastFired || (await moved(lastFired)))) {
        const body = due.map((d) => TRIGGER_COPY.schedule(d.cron, d.tz, d.caughtUp)).join(`\n`)
        try {
          const tick = await foldableTick(tx, agent)
          if (tick) {
            await tx
              .update(messages)
              .set({ body, updatedAt: now, ...(tick.pending ? {} : { consumedAt: null }) })
              .where(eq(messages.id, tick.id))
            if (!tick.pending) hosts.push({ ownerId: agent.ownerId, deviceId: agent.deviceId })
          } else {
            hosts.push(await postFiring(tx, agent, due[0]!.trigger, body, null))
          }
          for (const d of due) state[d.trigger.id] = { ...state[d.trigger.id], firedAt: d.at.toISOString() }
          count += due.length
        } catch (error) {
          // The agent's device is gone for its owner: nothing can run the
          // firing; the tick moves on (the owner fixes the device).
          console.warn(`[agent-triggers] schedules of agent ${agent.id} not fired:`, error)
        }
      }
      // Entries of triggers that no longer exist are dropped.
      const live = new Set(enabledTriggers(agent).map((trigger) => trigger.id))
      for (const key of Object.keys(state)) {
        if (!live.has(key)) {
          delete state[key]
          changed = true
        }
      }
      if (changed) {
        await tx.update(agents).set({ triggerState: state }).where(eq(agents.id, agent.id))
      }
    }
    return count
  })
  for (const host of hosts) nudge(host)
  return fired
}

/** An `events` row landed: every enabled event trigger on its name in the
 *  team fires, except the agent whose own session emitted it. */
export async function fireEventTriggersInTx(
  tx: Tx,
  event: Event,
  privateDetail?: EventPrivateDetail
): Promise<number> {
  const rows = await tx
    .select()
    .from(agents)
    .where(
      and(
        eq(agents.teamId, event.teamId),
        sql`${agents.triggers} @> ${JSON.stringify([{ kind: `event`, enabled: true, name: event.name }])}::jsonb`
      )
    )
  if (rows.length === 0) return 0
  const emitterAgentId = event.sessionId
    ? ((
        await tx
          .select({ agentId: sql<string | null>`coalesce(${sessions.agentId}, ${sessions.startedByAgentId})` })
          .from(sessions)
          .where(sql`${sessions.id} = (SELECT root_id FROM sessions WHERE id = ${event.sessionId})`)
          .limit(1)
      )[0]?.agentId ?? null)
    : null
  const body = (conversation: Session) =>
    TRIGGER_COPY.event(
      privateDetail && !askHidden(conversation, privateDetail.origin)
        ? { ...event, payload: privateDetail.payload }
        : event
    )
  let count = 0
  const hosts: Array<{ ownerId: string; deviceId: string } | null> = []
  for (const agent of rows) {
    if (agent.id === emitterAgentId) continue
    for (const trigger of enabledTriggers(agent)) {
      if (trigger.kind !== `event` || trigger.name !== event.name) continue
      try {
        hosts.push(await postFiring(tx, agent, trigger, body, event))
        count += 1
      } catch (error) {
        console.warn(`[agent-triggers] event trigger ${trigger.id} of agent ${agent.id} not fired:`, error)
      }
    }
  }
  for (const host of hosts) nudge(host)
  return count
}
