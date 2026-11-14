// @vitest-environment node
// #VAPP-220 (eApps A1) against the REAL migrated Postgres: a record write and
// a team event run an app's automations as the SPONSOR (the publisher)
// through the broker, each run leaves its `mcp_calls` row; an automation's
// own write fires nothing (the loop guard); a read-only sponsor pauses the
// automation and notifies the owners once; a schedule runs once per due
// minute; Settings lists the last run.
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { appAccess, appRecords, apps, mcpCalls, teamMembers, teams } from "@/db/schema"
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
const { automationRows, fireScheduledAutomations, runsLastMinute } = await import(`@/lib/apps/automations`)

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
  const [app] = await db
    .insert(apps)
    .values({ teamId, slug: `requests`, name: `Requests`, package: PACKAGE as never, headVersion: 1, publishedVersion: 1, publishedBy: owner, createdBy: owner })
    .returning({ id: apps.id })
  return { owner, member, teamId, appId: app!.id }
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
  it(`a record create runs the automation as the publisher; its own write fires nothing`, async () => {
    const w = await world()
    await upsertRecord(w.member, { id: w.appId }, { kind: `request`, data: { title: `Leave` } })
    const [log] = await until(() => logs(w.appId), 1)
    expect(log?.data.title).toBe(`Leave`)
    expect(log?.authorId).toBe(w.owner)
    // M3 (#VAPP-229): the run's write reads "via <app>".
    expect(log?.viaAppId).toBe(w.appId)
    const [call] = await until(() => calls(w.appId), 1)
    expect(call).toMatchObject({ clientId: `automation:log_request`, userId: w.owner, tool: `records.upsert:log`, status: `ok` })
    // The loop guard: `log_log` never ran on the automation's log write.
    await new Promise((r) => setTimeout(r, 300))
    expect(await logs(w.appId)).toHaveLength(1)
    expect((await calls(w.appId)).map((c) => c.clientId)).toEqual([`automation:log_request`])
  })

  it(`a team event runs the automations on its name, after the commit`, async () => {
    const w = await world()
    await db.transaction((tx) => emitEventInTx(tx, { teamId: w.teamId, name: `deploy.done`, payload: { version: `1.2` }, emittedBy: { userId: w.member } }))
    const [log] = await until(() => logs(w.appId), 1)
    expect(log?.data.title).toBe(`deploy 1.2`)
    const [call] = await until(() => calls(w.appId), 1)
    expect(call?.clientId).toBe(`automation:on_deploy`)
  })

  it(`a read-only sponsor pauses the automation and notifies the owners once`, async () => {
    const w = await world()
    await db.update(apps).set({ publishedBy: w.member }).where(eq(apps.id, w.appId))
    await db.insert(appAccess).values({ appId: w.appId, teamId: w.teamId, userId: w.member, level: `read` })
    paused.mockClear()
    await upsertRecord(w.owner, { id: w.appId }, { kind: `request`, data: { title: `A` } })
    const [call] = await until(() => calls(w.appId), 1)
    expect(call).toMatchObject({ status: `denied`, error: `sponsorReadOnly`, userId: w.member })
    await upsertRecord(w.owner, { id: w.appId }, { kind: `request`, data: { title: `B` } })
    await until(() => calls(w.appId), 2)
    expect(await logs(w.appId)).toHaveLength(0)
    expect(paused).toHaveBeenCalledTimes(1)
    const [row] = await db.select({ state: apps.automationState }).from(apps).where(eq(apps.id, w.appId))
    expect(row?.state.log_request?.paused).toBe(`sponsorReadOnly`)
    const listed = await automationRows({ id: w.appId, teamId: w.teamId, package: PACKAGE as never, publishedBy: w.member })
    expect(listed.find((r) => r.name === `log_request`)).toMatchObject({ paused: `sponsorReadOnly`, lastStatus: `denied`, lastError: `sponsorReadOnly` })
  })

  it(`Settings counts the runs the cap dropped in the last 24 h (#VAPP-255)`, async () => {
    const w = await world()
    const dropped = (createdAt: Date) => ({ teamId: w.teamId, userId: w.owner, clientId: `automation:log_request`, appId: w.appId, tool: `records.upsert:log`, status: `denied` as const, error: `rateLimited`, latencyMs: 0, argsHash: `x`, createdAt })
    await db.insert(mcpCalls).values([dropped(new Date()), dropped(new Date()), dropped(new Date(Date.now() - 25 * 3_600_000))])
    const listed = await automationRows({ id: w.appId, teamId: w.teamId, package: PACKAGE as never, publishedBy: w.owner })
    expect(listed.find((r) => r.name === `log_request`)?.droppedRuns).toBe(2)
    expect(listed.find((r) => r.name === `morning`)?.droppedRuns).toBe(0)
  })

  it(`the cap's own refusals never fill the window (#VAPP-257)`, async () => {
    const w = await world()
    const row = (error: string | null, status: `ok` | `denied`) => ({ teamId: w.teamId, userId: w.owner, clientId: `automation:log_request`, appId: w.appId, tool: `records.upsert:log`, status, error, latencyMs: 0, argsHash: `x` })
    await db.insert(mcpCalls).values([
      ...Array.from({ length: 40 }, () => row(`rateLimited`, `denied`)),
      row(null, `ok`),
      row(`sponsorReadOnly`, `denied`),
    ])
    expect(await runsLastMinute(w.appId)).toBe(2)
  })

  it(`a schedule runs once per due minute`, async () => {
    const w = await world()
    await db.update(apps).set({ automationState: { morning: { checkedAt: `2026-11-03T07:59:30.000Z` } } }).where(eq(apps.id, w.appId))
    const now = new Date(`2026-11-03T08:00:30.000Z`)
    await fireScheduledAutomations(now)
    await fireScheduledAutomations(now)
    const mine = (await logs(w.appId)).filter((l) => l.data.title === `morning`)
    expect(mine).toHaveLength(1)
    expect(mine[0]?.data.at).toBe(`2026-11-03T08:00:00.000Z`)
    const listed = await automationRows({ id: w.appId, teamId: w.teamId, package: PACKAGE as never, publishedBy: w.owner })
    expect(listed.map((r) => r.name)).toEqual([`log_request`, `log_log`, `on_deploy`, `morning`])
    expect(listed.find((r) => r.name === `morning`)).toMatchObject({ lastStatus: `ok`, paused: null, lastFailureAt: null })
  })
})
