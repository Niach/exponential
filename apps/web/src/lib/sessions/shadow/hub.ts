// #EXP-1510: the slice of Postgres the orchestration core reasons over, as
// its `HubState` (packages/eagent/src/model.ts). Loaded before and after
// every shadowed event: the involved sessions' whole trees, the live
// sessions watching any of them, their devices and agents, and only the
// messages a rule reads (open cards and asks, unconsumed input, each
// session's last own reply, the caller's exchange with a target tree).

import { and, eq, inArray, isNotNull, isNull, ne, or, sql } from "drizzle-orm"
import type { HubAgent, HubMessage, HubSession, HubState } from "@exp/eagent/model"
import { db } from "@/db/connection"
import { agents, conversations, devices, messages, sessions } from "@/db/schema"
import { isSealedBody, isSealedCard } from "@/lib/sessions/sealed"

export interface HubScope {
  /** Sessions the event names (their trees load whole). */
  sessionIds?: readonly string[]
  /** Every live session on these devices (a heartbeat). */
  deviceIds?: readonly string[]
  /** Every live session (the watchdog tick). */
  allLive?: boolean
  /** The calling session of a send: its exchange with the target's tree decides reuse (#FEED-172). */
  callerId?: string | null
  /** A send's target: the caller's exchange with ITS tree is loaded. */
  targetId?: string | null
}

const LIVE = [`queued`, `running`, `idle`] as const
const ms = (at: Date | null | undefined) => (at ? at.getTime() : null)
const stamp = (value: unknown): number | null => {
  if (typeof value !== `string`) return null
  const at = Date.parse(value)
  return Number.isFinite(at) ? at : null
}

type SessionRow = typeof sessions.$inferSelect

function hubSession(row: SessionRow): HubSession {
  const b = row.blocked as (Record<string, unknown> & { kind?: string | null }) | null
  return {
    id: row.id,
    parentId: row.parentId,
    rootId: row.rootId,
    depth: row.depth,
    teamId: row.teamId,
    userId: row.userId,
    shared: row.shared,
    agentId: row.agentId,
    startedByAgentId: row.startedByAgentId,
    deviceId: row.deviceId,
    status: row.status,
    busy: row.busy,
    background: row.background,
    needsInput: row.needsInput,
    blocked: b?.kind
      ? {
          kind: b.kind as NonNullable<HubSession[`blocked`]>[`kind`],
          since: stamp(b.since) ?? 0,
          window: (b.window as string | null | undefined) ?? null,
          resetsAt: (b.resetsAt as string | null | undefined) ?? null,
          ...(typeof b.resume === `boolean` ? { resume: b.resume, resumeSince: stamp(b.resumeSince) } : {}),
        }
      : null,
    wakeOn: [...(row.wakeOn ?? [])],
    createdAt: row.createdAt.getTime(),
    startedAt: ms(row.startedAt),
    endedAt: ms(row.endedAt),
    lastTurnAt: ms(row.lastTurnAt),
    lastActivityAt: ms(row.lastActivityAt),
    updatedAt: row.updatedAt.getTime(),
  }
}

const MESSAGE_COLUMNS = {
  id: messages.id,
  sessionId: conversations.sessionId,
  authorKind: messages.authorKind,
  authorSessionId: messages.authorSessionId,
  body: messages.body,
  card: messages.card,
  consumedAt: messages.consumedAt,
  answeredAt: messages.answeredAt,
  createdAt: messages.createdAt,
}
type MessageRow = { [K in keyof typeof MESSAGE_COLUMNS]: unknown }

function hubMessage(row: MessageRow): HubMessage {
  const body = (row.body as string) ?? ``
  const card = (row.card as HubMessage[`card`]) ?? null
  const sealed = isSealedBody(body) || (card !== null && isSealedCard(card))
  return {
    id: row.id as string,
    sessionId: row.sessionId as string,
    authorKind: row.authorKind as HubMessage[`authorKind`],
    authorSessionId: (row.authorSessionId as string | null) ?? null,
    // The hub never reads a sealed body (EXP-1458).
    body: sealed ? `` : body,
    card,
    sealed,
    pending: row.consumedAt === null,
    answered: row.answeredAt !== null,
    at: (row.createdAt as Date).getTime(),
  }
}

/** The rows of `scope` as the core's state (`seq` 0: core ids are `m<n>`, never a uuid). */
export async function loadHub(scope: HubScope): Promise<HubState> {
  const seeds = [...(scope.sessionIds ?? []), ...(scope.callerId ? [scope.callerId] : []), ...(scope.targetId ? [scope.targetId] : [])]
  const seedConditions = [
    seeds.length ? inArray(sessions.id, seeds) : null,
    scope.deviceIds?.length ? and(inArray(sessions.deviceId, [...scope.deviceIds]), inArray(sessions.status, [...LIVE])) : null,
    scope.allLive ? inArray(sessions.status, [...LIVE]) : null,
  ].filter((condition) => condition !== null)
  if (seedConditions.length === 0) return { seq: 0, sessions: {}, messages: [], devices: {}, agents: {} }
  const seedRoots = await db.selectDistinct({ rootId: sessions.rootId }).from(sessions).where(or(...seedConditions))
  const rows = new Map<string, SessionRow>()
  const addTrees = async (rootIds: string[]) => {
    const fresh = rootIds.filter((id) => ![...rows.values()].some((row) => row.rootId === id))
    if (fresh.length === 0) return
    for (const row of await db.select().from(sessions).where(inArray(sessions.rootId, fresh))) rows.set(row.id, row)
  }
  await addTrees(seedRoots.map((row) => row.rootId))
  // Live watchers of any loaded row (the bus reaches them), and their trees (caps, roots).
  if (rows.size) {
    const watched = [...rows.keys()]
    const watchers = await db
      .selectDistinct({ rootId: sessions.rootId })
      .from(sessions)
      .where(
        and(
          inArray(sessions.status, [...LIVE]),
          sql`${sessions.wakeOn} ?| ARRAY[${sql.join(
            watched.map((id) => sql`${id}`),
            sql`, `
          )}]::text[]`
        )
      )
    await addTrees(watchers.map((row) => row.rootId))
  }
  const state: HubState = { seq: 0, sessions: {}, messages: [], devices: {}, agents: {} }
  for (const row of rows.values()) state.sessions[row.id] = hubSession(row)
  const ids = [...rows.keys()]
  if (ids.length === 0) return state

  const deviceIds = [...new Set([...[...rows.values()].map((row) => row.deviceId), ...(scope.deviceIds ?? [])])]
  for (const row of await db
    .select({ deviceId: devices.deviceId, lastSeenAt: devices.lastSeenAt, liveSessionIds: devices.liveSessionIds })
    .from(devices)
    .where(inArray(devices.deviceId, deviceIds))) {
    const seen = row.lastSeenAt?.getTime() ?? 0
    const known = state.devices[row.deviceId]
    if (known && known.lastSeenAt >= seen) continue
    state.devices[row.deviceId] = { id: row.deviceId, lastSeenAt: seen, liveSessionIds: row.liveSessionIds ?? null }
  }

  const agentIds = [...new Set([...rows.values()].flatMap((row) => [row.agentId, row.startedByAgentId].filter((id): id is string => !!id)))]
  if (agentIds.length) {
    for (const agent of await db
      .select({ id: agents.id, ownerId: agents.ownerId, teamId: agents.teamId, role: agents.role })
      .from(agents)
      .where(inArray(agents.id, agentIds))) {
      const conversation = [...rows.values()].find((row) => row.agentId === agent.id && row.userId === agent.ownerId)
      state.agents[agent.id] = {
        id: agent.id,
        ownerId: agent.ownerId,
        teamId: agent.teamId,
        role: agent.role as HubAgent[`role`],
        conversationId: conversation?.id ?? ``,
        triggers: [],
        firedAt: {},
      }
    }
  }

  const seen = new Set<string>()
  const add = (list: MessageRow[]) => {
    for (const row of list) {
      if (seen.has(row.id as string)) continue
      seen.add(row.id as string)
      state.messages.push(hubMessage(row))
    }
  }
  const base = () => db.select(MESSAGE_COLUMNS).from(messages).innerJoin(conversations, eq(conversations.id, messages.conversationId))
  // Open question/plan cards and ask mirrors, and unconsumed input (a person's, or another session's).
  add(
    await base()
      .where(
        and(
          inArray(conversations.sessionId, ids),
          or(
            and(isNull(messages.answeredAt), sql`${messages.card} ->> 'kind' IN ('question', 'plan', 'ask')`),
            and(
              isNull(messages.consumedAt),
              or(eq(messages.authorKind, `user`), and(isNotNull(messages.authorSessionId), ne(messages.authorSessionId, conversations.sessionId)))
            )
          )
        )
      )
      .limit(5000)
  )
  // Each session's last own reply (a report's fallback summary).
  add(
    (await db.execute(sql`
      SELECT DISTINCT ON (c.session_id) m.id, c.session_id AS "sessionId", m.author_kind AS "authorKind",
        m.author_session_id AS "authorSessionId", m.body, m.card, m.consumed_at AS "consumedAt",
        m.answered_at AS "answeredAt", m.created_at AS "createdAt"
      FROM ${messages} m JOIN ${conversations} c ON c.id = m.conversation_id
      WHERE c.session_id IN (${sql.join(
        ids.map((id) => sql`${id}::uuid`),
        sql`, `
      )})
        AND m.author_kind = 'agent' AND m.card IS NULL AND m.author_session_id IS NULL
      ORDER BY c.session_id, m.created_at DESC, m.id DESC`)).rows.map((row) => ({
      ...(row as MessageRow),
      createdAt: new Date((row as { createdAt: string | Date }).createdAt),
      consumedAt: (row as { consumedAt: unknown }).consumedAt ?? null,
      answeredAt: (row as { answeredAt: unknown }).answeredAt ?? null,
    }))
  )
  // #FEED-172: the caller's latest exchange with the target's tree, both ways.
  const caller = scope.callerId ? rows.get(scope.callerId) : undefined
  const target = scope.targetId ? rows.get(scope.targetId) : undefined
  if (caller && target) {
    const tree = [...rows.values()].filter((row) => row.rootId === target.rootId).map((row) => row.id)
    add(
      await base()
        .where(and(eq(conversations.sessionId, caller.id), eq(messages.authorKind, `agent`), inArray(messages.authorSessionId, tree)))
        .orderBy(sql`${messages.createdAt} DESC`)
        .limit(1)
    )
    add(
      await base()
        .where(and(inArray(conversations.sessionId, tree), eq(messages.authorSessionId, caller.id)))
        .orderBy(sql`${messages.createdAt} DESC`)
        .limit(1)
    )
  }
  state.messages.sort((a, b) => a.at - b.at || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
  return state
}

/** The messages posted into `sessionIds` since `since` (what the server did, to diff against the core's posts). */
export async function postedSince(sessionIds: readonly string[], since: Date): Promise<HubMessage[]> {
  if (sessionIds.length === 0) return []
  const rows = await db
    .select(MESSAGE_COLUMNS)
    .from(messages)
    .innerJoin(conversations, eq(conversations.id, messages.conversationId))
    .where(and(inArray(conversations.sessionId, [...sessionIds]), sql`${messages.createdAt} >= ${since}`))
    .limit(2000)
  return rows.map(hubMessage).sort((a, b) => a.at - b.at || (a.id < b.id ? -1 : 1))
}
