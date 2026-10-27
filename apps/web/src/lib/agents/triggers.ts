import { and, eq, isNull, sql } from "drizzle-orm"
import type { AgentTrigger, AgentTriggerState } from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"
import { db } from "@/db/connection"
import { agents, messages, sessions, type Agent, type Event, type Session } from "@/db/schema"
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
// skipped. Event triggers fire as the `events` row lands (`lib/events.ts`),
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

/** The firing of `trigger` still unread on the owner's conversation. */
async function firingPending(tx: Tx, agent: Agent, triggerId: string): Promise<boolean> {
  const [row] = await tx
    .select({ id: messages.id })
    .from(messages)
    .innerJoin(sessions, eq(sessions.id, messages.sessionId))
    .where(
      and(
        eq(sessions.agentId, agent.id),
        eq(sessions.userId, agent.ownerId),
        isNull(messages.consumedAt),
        sql`${messages.card}->>'event' = 'trigger_fired' AND ${messages.card}->>'triggerId' = ${triggerId}`
      )
    )
    .limit(1)
  return !!row
}

/** One tick of every agent's schedules. Returns the firings posted. */
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
      for (const trigger of enabledTriggers(agent)) {
        if (trigger.kind !== `schedule`) continue
        const fields = parseCron(trigger.cron)
        if (!fields) continue
        const entry = { ...(state[trigger.id] ?? {}) }
        const checkedAt = entry.checkedAt ? new Date(entry.checkedAt) : null
        entry.checkedAt = now.toISOString()
        state[trigger.id] = entry
        changed = true
        // A trigger seen for the first time is ARMED: it fires from the
        // next due minute on, never retroactively.
        if (!checkedAt || Number.isNaN(checkedAt.getTime())) continue
        const after = checkedAt > catchUpSince ? checkedAt : catchUpSince
        const due = latestCronMatch(fields, after, now, trigger.tz ?? null)
        if (!due) continue
        if (await firingPending(tx, agent, trigger.id)) continue
        const caughtUp = now.getTime() - due.getTime() > 2 * MIN
        try {
          hosts.push(
            await postFiring(tx, agent, trigger, TRIGGER_COPY.schedule(trigger.cron, trigger.tz ?? null, caughtUp), null)
          )
          entry.firedAt = due.toISOString()
          count += 1
        } catch (error) {
          // The agent's device is gone for its owner: nothing can run the
          // firing; the tick moves on (the owner fixes the device).
          console.warn(`[agent-triggers] schedule ${trigger.id} of agent ${agent.id} not fired:`, error)
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
