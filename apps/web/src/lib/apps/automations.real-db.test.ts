// @vitest-environment node
// #VAPP-220 (eApps A1) + #EXP-1829 against the REAL migrated Postgres: the
// boot pass moves a package's automations onto the app agent's triggers
// (creating the agent); a record write and a team event run them as the
// agent's OWNER through the broker, each run leaves its `mcp_calls` row; a
// run's own write fires nothing (the loop guard); a read-only owner pauses
// the trigger and notifies the owners once; a schedule runs once per due
// minute; a wake trigger posts the card into the owner's conversation;
// Settings lists the last run.
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, appAccess, appRecords, appVersions, apps, devices, mcpCalls, messages, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

const paused = vi.fn(async () => {})
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  notifySessionBlocked: async () => {},
  fireAndForgetAssignmentNotify: () => {},
  fireAndForgetIssueMentionNotify: () => {},
  fireAndForgetCommentNotify: () => {},
  fireAndForgetStatusChangeNotify: () => {},
  fireAndForgetReporterResolution: () => {},
  fireAndForgetPrNotify: () => {},
  notifyAutomationPaused: (...args: unknown[]) => paused(...(args as [])),
}))
vi.mock(`@/lib/conversion/events`, () => ({ recordConversionEvent: async () => {} }))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns
    WHERE table_name = 'apps' AND column_name = 'automation_state') AS ok
`)
if (!ready.ok) console.info(`[automations.real-db] skipped: ${ready.reason}`)

const { upsertRecord } = await import(`@/lib/apps/records-service`)
const { emitEventInTx } = await import(`@/lib/events`)
const { automationRows, runsLastMinute } = await import(`@/lib/apps/automations`)
const { fireScheduledTriggers } = await import(`@/lib/agents/triggers`)
const { migrateAppAutomations } = await import(`@/lib/apps/automations-migrate`)
const { triggerAutomationName } = await import(`@/lib/agents/app-triggers`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `automations-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@automations.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

const PACKAGE = {
  records: {
    request: { schema: { type: `object`, properties: { title: { type: `string` } } } },
    log: { schema: { type: `object`, properties: { title: { type: `string` }, at: { type: `string` } } } },
  },
  automations: [
    { name: `log_request`, on: { record: { kind: `request`, op: `create` } }, run: { records: { op: `upsert`, kind: `log`, data: { title: `\${record.title}`, at: `\${firedAt}` } } } },
    // Would loop forever without the guard: a log write writing a log.
    { name: `log_log`, on: { record: { kind: `log`, op: `create` } }, run: { records: { op: `upsert`, kind: `log`, data: { title: `again` } } } },
    { name: `on_deploy`, on: { event: `deploy.done` }, run: { records: { op: `upsert`, kind: `log`, data: { title: `deploy \${event.version}` } } } },
    { name: `morning`, on: { schedule: { cron: `0 8 * * *`, tz: `UTC` } }, run: { records: { op: `upsert`, kind: `log`, data: { title: `morning`, at: `\${firedAt}` } } } },
  ],
}

async function world() {
  const owner = await user()
  const member = await user()
  const [row] = await db.insert(teams).values({ name: `Automations`, slug: `auto-${id8()}` }).returning({ id: teams.id })
  const teamId = row!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner, role: `owner` },
    { teamId, userId: member, role: `member` },
  ])
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `desktop`, lastSeenAt: new Date(), isDefault: true, sharedTeamIds: [teamId] })
  // A pre-#EXP-1829 app: automations in its package, no agent yet.
  const [app] = await db
    .insert(apps)
    .values({ teamId, slug: `requests`, name: `Requests`, package: PACKAGE as never, headVersion: 1, publishedVersion: 1, publishedBy: owner, createdBy: owner, automationState: { morning: { checkedAt: `2026-11-03T07:59:30.000Z` } } })
    .returning({ id: apps.id })
  await db.insert(appVersions).values({ appId: app!.id, teamId, number: 1, package: PACKAGE as never })
  await migrateAppAutomations()
  const [moved] = await db.select({ agentId: apps.agentId, package: apps.package }).from(apps).where(eq(apps.id, app!.id))
  const [agent] = await db.select().from(agents).where(eq(agents.id, moved!.agentId!))
  // The triggers keep the package's order: name → its audit client id.
  const ids = Object.fromEntries(PACKAGE.automations.map((a, i) => [a.name, agent!.triggers[i]!.id]))
  const cid = (name: string) => `automation:${triggerAutomationName(ids[name]!)}`
  return { owner, member, teamId, appId: app!.id, agentId: agent!.id, agent: agent!, pkg: moved!.package, ids, cid }
}

const logs = (appId: string) => db.select({ data: appRecords.data, authorId: appRecords.authorId, viaAppId: appRecords.viaAppId }).from(appRecords).where(and(eq(appRecords.appId, appId), eq(appRecords.kind, `log`)))
const calls = (appId: string) => db.select().from(mcpCalls).where(eq(mcpCalls.appId, appId))

async function until<T>(read: () => Promise<T[]>, count: number): Promise<T[]> {
  for (let i = 0; i < 100; i++) {
    const rows = await read()
    if (rows.length >= count) return rows
    await new Promise((r) => setTimeout(r, 50))
  }
  return read()
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) {
    await db.execute(sql`DELETE FROM mcp_calls WHERE team_id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
    await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  }
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`app automations (real db)`, () => {
  it(`the boot pass moved the automations onto a new agent, idempotently`, async () => {
    const w = await world()
    expect(w.agent.ownerId).toBe(w.owner)
    expect(w.agent.triggers.map((t) => t.kind)).toEqual([`record`, `record`, `event`, `schedule`])
    expect(w.agent.triggers[3]).toMatchObject({ enabled: true, cron: `0 8 * * *`, tz: `UTC`, run: { records: { kind: `log` } } })
    expect(w.agent.triggerState[w.ids.morning!]).toEqual({ checkedAt: `2026-11-03T07:59:30.000Z` })
    expect(w.pkg).not.toHaveProperty(`automations`)
    const [version] = await db.select({ package: appVersions.package }).from(appVersions).where(eq(appVersions.appId, w.appId))
    expect(version?.package).not.toHaveProperty(`automations`)
    expect(await migrateAppAutomations()).toBe(0)
    const [again] = await db.select({ triggers: agents.triggers }).from(agents).where(eq(agents.id, w.agentId))
    expect(again?.triggers).toHaveLength(4)
  })

  it(`a record create runs the trigger as the agent's owner; its own write fires nothing`, async () => {
    const w = await world()
    await upsertRecord(w.member, { id: w.appId }, { kind: `request`, data: { title: `Leave` } })
    const [log] = await until(() => logs(w.appId), 1)
    expect(log?.data.title).toBe(`Leave`)
    expect(log?.authorId).toBe(w.owner)
    // M3 (#VAPP-229): the run's write reads "via <app>".
    expect(log?.viaAppId).toBe(w.appId)
    const [call] = await until(() => calls(w.appId), 1)
    expect(call).toMatchObject({ clientId: w.cid(`log_request`), userId: w.owner, tool: `records.upsert:log`, status: `ok` })
    // The loop guard: `log_log` never ran on the automation's log write.
    await new Promise((r) => setTimeout(r, 300))
    expect(await logs(w.appId)).toHaveLength(1)
    expect((await calls(w.appId)).map((c) => c.clientId)).toEqual([w.cid(`log_request`)])
  })

  it(`a team event runs the automations on its name, after the commit`, async () => {
    const w = await world()
    await db.transaction((tx) => emitEventInTx(tx, { teamId: w.teamId, name: `deploy.done`, payload: { version: `1.2` }, emittedBy: { userId: w.member } }))
    const [log] = await until(() => logs(w.appId), 1)
    expect(log?.data.title).toBe(`deploy 1.2`)
    const [call] = await until(() => calls(w.appId), 1)
    expect(call?.clientId).toBe(w.cid(`on_deploy`))
  })

  it(`a read-only owner pauses the trigger and notifies the owners once`, async () => {
    const w = await world()
    await db.update(agents).set({ ownerId: w.member }).where(eq(agents.id, w.agentId))
    await db.insert(appAccess).values({ appId: w.appId, teamId: w.teamId, userId: w.member, level: `read` })
    paused.mockClear()
    await upsertRecord(w.owner, { id: w.appId }, { kind: `request`, data: { title: `A` } })
    const [call] = await until(() => calls(w.appId), 1)
    expect(call).toMatchObject({ status: `denied`, error: `sponsorReadOnly`, userId: w.member })
    await upsertRecord(w.owner, { id: w.appId }, { kind: `request`, data: { title: `B` } })
    await until(() => calls(w.appId), 2)
    expect(await logs(w.appId)).toHaveLength(0)
    expect(paused).toHaveBeenCalledTimes(1)
    const [row] = await db.select({ state: agents.triggerState }).from(agents).where(eq(agents.id, w.agentId))
    expect(row?.state[w.ids.log_request!]?.paused).toBe(`sponsorReadOnly`)
    const listed = await automationRows({ id: w.appId })
    expect(listed.find((r) => r.name === `When a request is created`)).toMatchObject({ paused: `sponsorReadOnly`, lastStatus: `denied`, lastError: `sponsorReadOnly` })
  })

  it(`a wake trigger posts the card into the owner's conversation, audited`, async () => {
    const w = await world()
    const wake = { id: crypto.randomUUID(), enabled: true, kind: `record`, record: `request`, op: `create`, prompt: `Triage \${record.title}` }
    await db.update(agents).set({ triggers: [wake as never] }).where(eq(agents.id, w.agentId))
    await upsertRecord(w.member, { id: w.appId }, { kind: `request`, data: { title: `Leave` } })
    const [call] = await until(() => calls(w.appId), 1)
    expect(call).toMatchObject({ clientId: `automation:${triggerAutomationName(wake.id)}`, tool: `agent:requests`, status: `ok`, userId: w.owner })
    const [root] = await db.select().from(sessions).where(and(eq(sessions.agentId, w.agentId), eq(sessions.userId, w.owner)))
    const [card] = await db.select().from(messages).where(eq(messages.sessionId, root!.id))
    expect(card?.body).toContain(`Triage Leave`)
    expect(card?.card).toMatchObject({ event: `trigger_fired`, triggerId: wake.id })
  })

  it(`Settings counts the runs the cap dropped in the last 24 h (#VAPP-255)`, async () => {
    const w = await world()
    const dropped = (createdAt: Date) => ({ teamId: w.teamId, userId: w.owner, clientId: w.cid(`log_request`), appId: w.appId, tool: `records.upsert:log`, status: `denied` as const, error: `rateLimited`, latencyMs: 0, argsHash: `x`, createdAt })
    await db.insert(mcpCalls).values([dropped(new Date()), dropped(new Date()), dropped(new Date(Date.now() - 25 * 3_600_000))])
    const listed = await automationRows({ id: w.appId })
    expect(listed.find((r) => r.name === `When a request is created`)?.droppedRuns).toBe(2)
    expect(listed.find((r) => r.name.startsWith(`Schedule`))?.droppedRuns).toBe(0)
  })

  it(`a schedule runs once per due minute`, async () => {
    const w = await world()
    const now = new Date(`2026-11-03T08:00:30.000Z`)
    await fireScheduledTriggers(now)
    await fireScheduledTriggers(now)
    const mine = (await until(() => logs(w.appId), 1)).filter((l) => l.data.title === `morning`)
    expect(mine).toHaveLength(1)
    expect(mine[0]?.data.at).toBe(`2026-11-03T08:00:00.000Z`)
    const listed = await automationRows({ id: w.appId })
    expect(listed.map((r) => r.name)).toEqual([`When a request is created`, `When a log is created`, `On deploy.done`, `Schedule 0 8 * * * (UTC)`])
    expect(listed.find((r) => r.name.startsWith(`Schedule`))).toMatchObject({ lastStatus: `ok`, paused: null, lastFailureAt: null })
  })
})
