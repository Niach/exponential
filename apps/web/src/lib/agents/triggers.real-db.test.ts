// NEW-2: the server fires agent triggers. Real-DB (the compose Postgres with
// the custom triggers): a schedule tick fires ONCE into the owner's
// conversation (created on demand), a missed tick is caught up, an unread
// firing never stacks, an event lands on every listener but its emitter.
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, boards, devices, events, issues, messages, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

vi.mock(`@/lib/steer`, () => ({
  getSteerRelayConfig: () => null,
  relayPostNudge: async () => {},
  relayPostKill: async () => {},
  relayPostInterrupt: async () => {},
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  notifySessionBlocked: () => {},
  fireAndForgetAssignmentNotify: () => {},
  fireAndForgetIssueMentionNotify: () => {},
}))

const ready = await probeRealDb(`
  SELECT (
    EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'agents')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
  ) AS ok
`)
if (!ready.ok) console.info(`[agent-triggers.real-db] skipped: ${ready.reason}`)

const { fireScheduledTriggers, fireEventTriggersInTx } = await import(`@/lib/agents/triggers`)
const { emitEventInTx } = await import(`@/lib/events`)
const { sendSession } = await import(`@/lib/sessions/send`)
const { recordIssueEvent } = await import(`@/lib/integrations/activity`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `trig-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@triggers.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function world() {
  const owner = await user()
  const [team] = await db.insert(teams).values({ name: `Triggers`, slug: `trig-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([{ teamId, userId: owner, role: `owner` }])
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const agent = async (triggers: unknown[], name = `Bot`) =>
    (
      await db
        .insert(agents)
        .values({ teamId, name, deviceId, ownerId: owner, triggers: triggers as never })
        .returning()
    )[0]!
  return { owner, teamId, deviceId, agent }
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) {
    await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  }
  if (made.users.length) {
    await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
  }
})

const conversation = async (agentId: string, userId: string) =>
  (
    await db
      .select()
      .from(sessions)
      .where(and(eq(sessions.agentId, agentId), eq(sessions.userId, userId)))
  )[0]

const firings = async (sessionId: string) =>
  db
    .select({ body: messages.body, card: messages.card, consumedAt: messages.consumedAt, authorId: messages.authorId })
    .from(messages)
    .where(and(eq(messages.sessionId, sessionId), sql`${messages.card}->>'event' = 'trigger_fired'`))

describe.skipIf(!ready.ok)(`agent schedules (NEW-2)`, () => {
  it(`arms on the first tick, fires once on the due minute into the owner's conversation, catches a missed tick up and never stacks`, async () => {
    const w = await world()
    const trigger = { id: crypto.randomUUID(), enabled: true, kind: `schedule`, cron: `30 9 * * *`, tz: `UTC` }
    const bot = await w.agent([trigger])
    // Far past, so the WHOLE database's other agents (whose state is live)
    // never fire in this test; only ours walks these minutes.
    const t0 = new Date(`2020-01-01T09:00:00Z`)
    await fireScheduledTriggers(t0)
    expect(await conversation(bot.id, w.owner)).toBeUndefined()
    const armed = (await db.select().from(agents).where(eq(agents.id, bot.id)))[0]!
    expect(armed.triggerState[trigger.id]?.checkedAt).toBe(t0.toISOString())

    // 09:10: nothing due yet.
    await fireScheduledTriggers(new Date(`2020-01-01T09:10:00Z`))
    expect(await conversation(bot.id, w.owner)).toBeUndefined()
    // 09:31: due (09:30 lies in the window): one firing, the conversation
    // created queued on the agent's device, the owner as author.
    await fireScheduledTriggers(new Date(`2020-01-01T09:31:00Z`))
    const convo = (await conversation(bot.id, w.owner))!
    expect(convo).toMatchObject({ agentId: bot.id, deviceId: w.deviceId, status: `queued`, kind: `agent`, name: `Bot`, title: `Bot` })
    let fired = await firings(convo.id)
    expect(fired).toHaveLength(1)
    expect(fired[0]).toMatchObject({ authorId: w.owner, consumedAt: null })
    expect(fired[0]!.body).toContain(`30 9 * * *`)
    expect(fired[0]!.card).toMatchObject({ kind: `event`, event: `trigger_fired`, agentId: bot.id, triggerId: trigger.id })
    const state = (await db.select().from(agents).where(eq(agents.id, bot.id)))[0]!.triggerState[trigger.id]!
    expect(state.firedAt).toBe(`2020-01-01T09:30:00.000Z`)

    // The next day, the server slept over 09:30: the 13:00 tick catches it
    // up once; the unread firing of yesterday is still there, so it SKIPS.
    await fireScheduledTriggers(new Date(`2020-01-02T13:00:00Z`))
    expect(await firings(convo.id)).toHaveLength(1)
    // The device consumed it: the next due minute fires again (caught up).
    // S-3: nobody wrote since, so it folds into the same card, unread again.
    await db.update(messages).set({ consumedAt: new Date() }).where(eq(messages.sessionId, convo.id))
    await fireScheduledTriggers(new Date(`2020-01-03T10:00:00Z`))
    fired = await firings(convo.id)
    expect(fired).toHaveLength(1)
    expect(fired[0]!.body).toContain(`caught up`)
    expect(fired[0]!.consumedAt).toBeNull()
    // A paused trigger never fires (its bookkeeping waits for a resume); a
    // removed one loses its entry on the next tick.
    await db.update(agents).set({ triggers: [{ ...trigger, enabled: false }] as never }).where(eq(agents.id, bot.id))
    await fireScheduledTriggers(new Date(`2020-01-04T10:00:00Z`))
    expect(await firings(convo.id)).toHaveLength(1)
    const other = { id: crypto.randomUUID(), enabled: true, kind: `schedule`, cron: `0 0 1 1 *` }
    await db.update(agents).set({ triggers: [other] as never }).where(eq(agents.id, bot.id))
    await fireScheduledTriggers(new Date(`2020-01-04T10:01:00Z`))
    expect(Object.keys((await db.select().from(agents).where(eq(agents.id, bot.id)))[0]!.triggerState)).toEqual([other.id])
  })
})

describe.skipIf(!ready.ok)(`S-3: one tick card per agent conversation`, () => {
  it(`two schedules due on one tick post ONE card; a read tick nobody answered is re-armed in place; a person message makes the next one new`, async () => {
    const w = await world()
    const a = { id: crypto.randomUUID(), enabled: true, kind: `schedule`, cron: `0 * * * *`, tz: `UTC` }
    const b = { id: crypto.randomUUID(), enabled: true, kind: `schedule`, cron: `0 */2 * * *`, tz: `UTC` }
    const bot = await w.agent([a, b])
    await fireScheduledTriggers(new Date(`2020-02-01T09:30:00Z`))
    await fireScheduledTriggers(new Date(`2020-02-01T10:01:00Z`))
    const convo = (await conversation(bot.id, w.owner))!
    let fired = await firings(convo.id)
    expect(fired).toHaveLength(1)
    expect(fired[0]!.body).toContain(`0 * * * *`)
    expect(fired[0]!.body).toContain(`0 */2 * * *`)
    // Read, nobody answered: the 11:00 tick edits the same card, unread again.
    await db.update(messages).set({ consumedAt: new Date() }).where(eq(messages.sessionId, convo.id))
    await fireScheduledTriggers(new Date(`2020-02-01T11:01:00Z`))
    fired = await firings(convo.id)
    expect(fired).toHaveLength(1)
    expect(fired[0]!.consumedAt).toBeNull()
    expect(fired[0]!.body).not.toContain(`0 */2 * * *`)
    // A person wrote after it: the next tick is a new card.
    await db.update(messages).set({ consumedAt: new Date() }).where(eq(messages.sessionId, convo.id))
    await db.insert(messages).values({
      conversationId: sql`(SELECT id FROM conversations WHERE session_id = ${convo.id})`,
      sessionId: convo.id,
      rootId: convo.id,
      teamId: w.teamId,
      authorKind: `user`,
      authorId: w.owner,
      body: `status?`,
      createdAt: new Date(`2030-01-01T00:00:00Z`),
    } as never)
    await fireScheduledTriggers(new Date(`2020-02-01T12:01:00Z`))
    expect(await firings(convo.id)).toHaveLength(2)
  })

  it(`skips a tick when nothing moved in the threads the agent started`, async () => {
    const w = await world()
    const t = { id: crypto.randomUUID(), enabled: true, kind: `schedule`, cron: `0 * * * *`, tz: `UTC` }
    const bot = await w.agent([t])
    await fireScheduledTriggers(new Date(`2020-03-01T09:30:00Z`))
    await fireScheduledTriggers(new Date(`2020-03-01T10:01:00Z`))
    const convo = (await conversation(bot.id, w.owner))!
    // A thread it started, last active before the 10:00 firing.
    const threadId = crypto.randomUUID()
    const [thread] = await db
      .insert(sessions)
      .values({
        id: threadId,
        rootId: threadId,
        teamId: w.teamId,
        userId: w.owner,
        kind: `thread`,
        deviceId: w.deviceId,
        startedByAgentId: bot.id,
        lastActivityAt: new Date(`2020-03-01T09:00:00Z`),
      } as never)
      .returning({ id: sessions.id })
    await db.update(messages).set({ body: `old`, consumedAt: new Date() }).where(eq(messages.sessionId, convo.id))
    await fireScheduledTriggers(new Date(`2020-03-01T11:01:00Z`))
    let fired = await firings(convo.id)
    expect(fired[0]!.body).toBe(`old`)
    expect(fired[0]!.consumedAt).not.toBeNull()
    // It moved: the 12:00 tick fires (folded into the read card).
    await db.update(sessions).set({ lastActivityAt: new Date(`2020-03-01T11:30:00Z`) }).where(eq(sessions.id, thread!.id))
    await fireScheduledTriggers(new Date(`2020-03-01T12:01:00Z`))
    fired = await firings(convo.id)
    expect(fired[0]!.body).toContain(`0 * * * *`)
    expect(fired[0]!.consumedAt).toBeNull()
  })
})

describe.skipIf(!ready.ok)(`events (NEW-2)`, () => {
  it(`an emit lands on every listener but its emitter; builtins ride the issue writers`, async () => {
    const w = await world()
    const listener = await w.agent([{ id: crypto.randomUUID(), enabled: true, kind: `event`, name: `ui-changed` }], `Shots`)
    const other = await w.agent([{ id: crypto.randomUUID(), enabled: true, kind: `event`, name: `deploy` }], `Ops`)
    const emitter = await w.agent([{ id: crypto.randomUUID(), enabled: true, kind: `event`, name: `ui-changed` }], `Designer`)
    // The emitter's own conversation session emits.
    const designer = await sendSession({ userId: w.owner, input: { agentId: emitter.id, text: `hi` } })
    const event = await db.transaction((tx) =>
      emitEventInTx(tx, {
        teamId: w.teamId,
        name: `ui-changed`,
        payload: { files: [`a.tsx`] },
        emittedBy: { sessionId: designer.sessionId },
      })
    )
    expect(event).toMatchObject({ name: `ui-changed`, emittedBy: `session`, sessionId: designer.sessionId })
    const shots = (await conversation(listener.id, w.owner))!
    const fired = await firings(shots.id)
    expect(fired).toHaveLength(1)
    expect(fired[0]!.body).toContain(`a.tsx`)
    expect(fired[0]!.card).toMatchObject({ event: `trigger_fired`, agentId: listener.id, eventId: event.id, name: `ui-changed` })
    expect(await conversation(other.id, w.owner)).toBeUndefined()
    // Never its own emit.
    expect(await firings(designer.sessionId)).toHaveLength(0)
    // The same transaction helper, idempotent per listener trigger.
    await db.transaction((tx) => fireEventTriggersInTx(tx, event))
    expect(await firings(shots.id)).toHaveLength(2)

    // A builtin: an issue created records `issue.created` with its payload.
    const created = await w.agent([{ id: crypto.randomUUID(), enabled: true, kind: `event`, name: `issue.created` }], `Triage`)
    const [board] = await db.insert(boards).values({ teamId: w.teamId, name: `Core`, slug: `core-${id8()}`, prefix: `CR` }).returning({ id: boards.id })
    const [issue] = await db.insert(issues).values({ boardId: board!.id, teamId: w.teamId, title: `Crash`, creatorId: w.owner }).returning({ id: issues.id })
    await db.transaction((tx) =>
      recordIssueEvent(tx, { issueId: issue!.id, teamId: w.teamId, actorUserId: w.owner, type: `created` })
    )
    const rows = await db.select().from(events).where(and(eq(events.teamId, w.teamId), eq(events.name, `issue.created`)))
    expect(rows).toHaveLength(1)
    expect(rows[0]).toMatchObject({ emittedBy: `user`, userId: w.owner })
    expect(rows[0]!.payload).toMatchObject({ issueId: issue!.id, title: `Crash`, boardId: board!.id })
    const triage = (await conversation(created.id, w.owner))!
    expect(await firings(triage.id)).toHaveLength(1)
  })
})
