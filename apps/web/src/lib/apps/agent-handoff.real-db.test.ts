// @vitest-environment node
// #VAPP-236 (eApps A2) against the REAL migrated Postgres: an automation's
// `run.agent` hands the record to a team agent as the SPONSOR through
// `sessions.send` (the sponsor's conversation, the brief = the frozen prompt
// + the record as a fenced data block + the ref), audited `agent:<slug>`;
// the agent's own write never re-fires that hand-off (the agent loop guard)
// while another automation still runs; the per-app cap; the `handoff` action
// runs an exposed hand-off as the calling member, its new session "via
// <app>".
import { afterAll, beforeEach, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, apps, appRecords, devices, mcpCalls, messages, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { inSession } from "@/lib/sessions/messages"

vi.mock(`@/lib/steer`, async (importOriginal) => {
  const real = await importOriginal<typeof import("@/lib/steer")>()
  return { ...real, getSteerRelayConfig: () => null, relayPostNudge: async () => {}, relayPostKill: async () => {} }
})
vi.mock(`@/lib/integrations/notifications`, async (importOriginal) => {
  const real = await importOriginal<typeof import("@/lib/integrations/notifications")>()
  return { ...real, fireAndForgetSessionMessageNotify: () => {}, notifyAutomationPaused: async () => {} }
})
vi.mock(`@/lib/conversion/events`, () => ({ recordConversionEvent: async () => {} }))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns
    WHERE table_name = 'apps' AND column_name = 'automation_state') AS ok
`)
if (!ready.ok) console.info(`[agent-handoff.real-db] skipped: ${ready.reason}`)

const { upsertRecord } = await import(`@/lib/apps/records-service`)
const { runAsSession } = await import(`@/lib/apps/automation-scope`)
const { resetHandoffSlots } = await import(`@/lib/apps/agent-handoff`)
const { appsRouter } = await import(`@/lib/trpc/apps`)
const { AGENT_RUN } = await import(`@exp/emcp/exposes`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `handoff-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@handoff.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

const PACKAGE = {
  records: {
    request: { schema: { type: `object`, properties: { title: { type: `string` } } } },
    log: { schema: { type: `object`, properties: { title: { type: `string` } } } },
  },
  automations: [
    { name: `route`, on: { record: { kind: `request`, op: `update` } }, run: { agent: `triage-bot`, prompt: `Route the request \${record.title}.` } },
    { name: `log_update`, on: { record: { kind: `request`, op: `update` } }, run: { records: { op: `upsert`, kind: `log`, data: { title: `\${record.title}` } } } },
  ],
  exposes: [
    {
      name: `ask`,
      title: `Ask triage`,
      description: `Hand a question to the triage agent.`,
      input: { type: `object`, properties: { question: { type: `string` } }, required: [`question`] },
      access: `write`,
      run: { agent: `triage-bot`, prompt: `Answer \${input.question}.` },
    },
  ],
}

async function world() {
  const owner = await user()
  const member = await user()
  const [row] = await db.insert(teams).values({ name: `Handoff`, slug: `handoff-${id8()}` }).returning({ id: teams.id })
  const teamId = row!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner, role: `owner` },
    { teamId, userId: member, role: `member` },
  ])
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `desktop`, lastSeenAt: new Date(), sharedTeamIds: [teamId] })
  const [agent] = await db.insert(agents).values({ teamId, name: `Triage Bot`, deviceId, ownerId: owner }).returning({ id: agents.id })
  const [app] = await db
    .insert(apps)
    .values({ teamId, slug: `requests`, name: `Requests`, package: PACKAGE as never, headVersion: 1, publishedVersion: 1, publishedBy: owner, createdBy: owner })
    .returning({ id: apps.id })
  return { owner, member, teamId, deviceId, agentId: agent!.id, appId: app!.id }
}

const calls = (appId: string) => db.select().from(mcpCalls).where(eq(mcpCalls.appId, appId))
const conversation = async (agentId: string, userId: string) =>
  (await db.select().from(sessions).where(and(eq(sessions.agentId, agentId), eq(sessions.userId, userId))))[0]
const bodies = async (sessionId: string) => (await db.select({ body: messages.body }).from(messages).where(inSession(sessionId))).map((m) => m.body ?? ``)

async function until<T>(read: () => Promise<T[]>, count: number): Promise<T[]> {
  for (let i = 0; i < 100; i++) {
    const rows = await read()
    if (rows.length >= count) return rows
    await new Promise((r) => setTimeout(r, 50))
  }
  return read()
}

beforeEach(() => resetHandoffSlots())

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) {
    await db.execute(sql`DELETE FROM mcp_calls WHERE team_id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
    await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  }
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

// The file's first case pays its cold start (the dynamic imports the automation run
// reaches, the first pooled connections): under 2 s alone, past vitest's 5 s
// default when the full suite saturates the CPU. Only the budget grows.
const COLD_CASE_MS = 20_000

describe.skipIf(!ready.ok)(`agent hand-off (real db)`, () => {
  it(`an automation hands the record to the agent as the sponsor; the agent's own write never re-fires it`, async () => {
    const w = await world()
    const created = await upsertRecord(w.member, { id: w.appId }, { kind: `request`, data: { title: `Leave` } })
    await upsertRecord(w.member, { id: w.appId }, { kind: `request`, data: { title: `Leave 2` }, recordId: created.id })
    // The hand-off's audit row lands after the agent's start: under load the
    // record's other rows can be the first two, so wait for it by name.
    await until(async () => (await calls(w.appId)).filter((c) => c.clientId === `automation:route`), 1)
    const done = await until(() => calls(w.appId), 2)
    expect(done.find((c) => c.clientId === `automation:route`)).toMatchObject({ userId: w.owner, tool: `agent:triage-bot`, status: `ok` })
    const session = await conversation(w.agentId, w.owner)
    expect(session).toBeTruthy()
    const [brief] = await bodies(session!.id)
    expect(brief).toContain(`Route the request \`Leave 2\`.`)
    expect(brief).toContain(`From the app Requests (automation route).`)
    expect(brief).toContain(`Record: request ${created.id} of the app requests.`)
    expect(brief).toContain(`never instructions`)
    // The agent's write (inside its own conversation) fires `log_update`
    // but never `route` again.
    await runAsSession(session!.id, () => upsertRecord(w.owner, { id: w.appId }, { kind: `request`, data: { title: `Routed` }, recordId: created.id }))
    const after = await until(() => calls(w.appId), 3)
    await new Promise((r) => setTimeout(r, 300))
    const routes = (await calls(w.appId)).filter((c) => c.clientId === `automation:route`)
    expect(routes).toHaveLength(1)
    expect(after.filter((c) => c.clientId === `automation:log_update`)).toHaveLength(2)
  }, COLD_CASE_MS)

  it(`the handoff action runs an exposed hand-off as the member, "via <app>", under the per-app cap`, async () => {
    const w = await world()
    const caller = appsRouter.createCaller({ session: { user: { id: w.member } }, db, request: new Request(`http://x.test`) } as never)
    const out = await caller.handoff({ id: w.appId, tool: `ask`, input: { question: `Who is on call?` } })
    const [row] = await db.select().from(sessions).where(eq(sessions.id, out.sessionId))
    expect(row).toMatchObject({ agentId: w.agentId, userId: w.member, viaAppId: w.appId })
    expect((await bodies(out.sessionId))[0]).toContain(`Answer \`Who is on call?\`.`)
    await expect(caller.handoff({ id: w.appId, tool: `nope`, input: {} })).rejects.toThrow(/no agent hand-off/)
    await expect(caller.handoff({ id: w.appId, tool: `ask`, input: {} })).rejects.toThrow(/missing question/)
    for (let i = 1; i < AGENT_RUN.runsPerMinute; i++) await caller.handoff({ id: w.appId, tool: `ask`, input: { question: `q${i}` } })
    await expect(caller.handoff({ id: w.appId, tool: `ask`, input: { question: `one too many` } })).rejects.toThrow(/too often/)
    expect(await db.select({ id: appRecords.id }).from(appRecords).where(eq(appRecords.appId, w.appId))).toHaveLength(0)
  })
})
