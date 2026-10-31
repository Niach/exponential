// @vitest-environment node
// #VAPP-175 (the end-to-end check of #VAPP-181): `/api/mcp/apps` against the
// REAL migrated Postgres, through the route's own POST handler (the gate,
// the OAuth bearer, the `apps` grant, the broker, the audit). A member's
// OAuth client lists and calls an app's tools; a read grant, a missing grant,
// an ungranted action and a missing upstream are refused; every tools/call
// leaves its `mcp_calls` row naming the app, the team and the client.
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { appAccess, apps, appRecords, comments, issues, mcpCalls, mcpGrants, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { createTestBoard } from "@/test/factories"

vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  notifySessionBlocked: async () => {},
  fireAndForgetAssignmentNotify: () => {},
  fireAndForgetIssueMentionNotify: () => {},
  fireAndForgetCommentNotify: () => {},
  fireAndForgetStatusChangeNotify: () => {},
  fireAndForgetReporterResolution: () => {},
  fireAndForgetPrNotify: () => {},
}))
vi.mock(`@/lib/conversion/events`, () => ({ recordConversionEvent: async () => {} }))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns
    WHERE table_name = 'mcp_grants' AND column_name = 'all_apps') AS ok
`)
if (!ready.ok) console.info(`[apps-broker.real-db] skipped: ${ready.reason}`)

const { Route } = await import(`@/routes/api/mcp_/apps`)
const post = (Route.options.server as { handlers: { POST: (ctx: { request: Request }) => Promise<Response> } }).handlers.POST

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[], clients: [] as string[] }

async function user(): Promise<string> {
  const id = `apps-broker-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@broker.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

/** An OAuth client of `userId`'s with a live access token. */
async function client(userId: string): Promise<{ clientId: string; token: string }> {
  const clientId = `client-${id8()}`
  const token = `tok-${crypto.randomUUID()}`
  await db.execute(sql`
    INSERT INTO oauth_applications (id, name, client_id, redirect_urls, type, disabled, created_at, updated_at)
    VALUES (${clientId}, 'Test client', ${clientId}, 'http://localhost/cb', 'public', false, now(), now())
  `)
  await db.execute(sql`
    INSERT INTO oauth_access_tokens (id, access_token, refresh_token, access_token_expires_at, refresh_token_expires_at, client_id, user_id, scopes, created_at, updated_at)
    VALUES (${`at-${id8()}`}, ${token}, ${`rt-${crypto.randomUUID()}`}, now() + interval '1 hour', now() + interval '1 day', ${clientId}, ${userId}, 'openid', now(), now())
  `)
  made.clients.push(clientId)
  return { clientId, token }
}

const PACKAGE = {
  records: { entry: { schema: { type: `object`, properties: { day: { type: `string` }, hours: { type: `number` } } } } },
  upstreams: [`m5`],
  exposes: [
    {
      name: `list_entries`,
      title: `List entries`,
      description: `The time entries of a day.`,
      input: { type: `object`, properties: { day: { type: `string` } } },
      access: `read`,
      run: { records: { op: `list`, kind: `entry`, where: { day: `\${input.day}` } } },
    },
    {
      name: `add_entry`,
      title: `Add an entry`,
      description: `Log hours on a day.`,
      input: { type: `object`, properties: { day: { type: `string` }, hours: { type: `number` } }, required: [`day`, `hours`] },
      access: `write`,
      run: { records: { op: `upsert`, kind: `entry`, data: { day: `\${input.day}`, hours: `\${input.hours}` } } },
    },
    {
      name: `log_comment`,
      title: `Log a comment`,
      description: `Comment the logged hours on an issue.`,
      input: { type: `object`, properties: { issue: { type: `string` }, hours: { type: `number` } }, required: [`issue`, `hours`] },
      access: `write`,
      run: { action: `exponential.comments.create`, args: { issueId: `\${input.issue}`, body: `Logged \${input.hours} h` } },
    },
    {
      name: `find_customer`,
      title: `Find a customer`,
      description: `Search m5 customers.`,
      input: { type: `object`, properties: { q: { type: `string` } } },
      access: `read`,
      run: { upstream: `m5`, tool: `search_customers`, args: { query: `\${input.q}` } },
    },
  ],
}

async function world(opts: { grantComments?: boolean } = {}) {
  const owner = await user()
  const [row] = await db.insert(teams).values({ name: `Broker`, slug: `broker-${id8()}` }).returning({ id: teams.id })
  const teamId = row!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  const board = await createTestBoard({ teamId, prefix: `BRK` })
  const [issue] = await db.insert(issues).values({ boardId: board.id, teamId, title: `Hours`, creatorId: owner }).returning({ id: issues.id })
  const [app] = await db
    .insert(apps)
    .values({
      teamId,
      slug: `timesheet`,
      name: `Timesheet`,
      package: PACKAGE as never,
      headVersion: 1,
      createdBy: owner,
      grantedTools: opts.grantComments === false ? [] : [`exponential.comments.create`],
    })
    .returning({ id: apps.id })
  return { owner, teamId, issueId: issue!.id, appId: app!.id }
}

async function grant(userId: string, clientId: string, teamId: string, access: `read` | `full`) {
  await db.insert(mcpGrants).values({ userId, clientId, kind: `apps`, teamIds: [teamId], allApps: true, access })
}

let rpcId = 0
async function rpc(token: string, method: string, params: Record<string, unknown> = {}) {
  const response = await post({
    request: new Request(`http://localhost/api/mcp/apps`, {
      method: `POST`,
      headers: { authorization: `Bearer ${token}`, "content-type": `application/json`, accept: `application/json, text/event-stream` },
      body: JSON.stringify({ jsonrpc: `2.0`, id: ++rpcId, method, params }),
    }),
  })
  return { status: response.status, body: (await response.json()) as { result?: Record<string, unknown>; error?: { code: number; message: string } } }
}

const toolNames = (body: { result?: Record<string, unknown> }) => ((body.result?.tools ?? []) as { name: string }[]).map((t) => t.name).sort()
const text = (body: { result?: Record<string, unknown> }) => ((body.result?.content ?? []) as { text?: string }[])[0]?.text ?? ``

/** The audit is written after the response; wait for `count` rows. */
async function auditOf(clientId: string, count: number) {
  for (let i = 0; i < 50; i++) {
    const rows = await db.select().from(mcpCalls).where(eq(mcpCalls.clientId, clientId))
    if (rows.length >= count) return rows
    await new Promise((r) => setTimeout(r, 50))
  }
  return db.select().from(mcpCalls).where(eq(mcpCalls.clientId, clientId))
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.clients.length) {
    await db.execute(sql`DELETE FROM mcp_calls WHERE client_id IN ${sql.raw(`('${made.clients.join(`','`)}')`)}`)
    await db.execute(sql`DELETE FROM oauth_applications WHERE client_id IN ${sql.raw(`('${made.clients.join(`','`)}')`)}`)
  }
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`/api/mcp/apps end to end (real db)`, () => {
  it(`a token without an apps grant gets 403; no token gets 401`, async () => {
    const w = await world()
    const { token } = await client(w.owner)
    const refused = await rpc(token, `tools/list`)
    expect(refused.status).toBe(403)
    expect(refused.body.error?.message).toContain(`no apps grant`)
    expect((await rpc(`not-a-token`, `tools/list`)).status).toBe(401)
  })

  it(`lists, calls and audits a full grant's tools`, async () => {
    const w = await world()
    const { clientId, token } = await client(w.owner)
    await grant(w.owner, clientId, w.teamId, `full`)

    const listed = await rpc(token, `tools/list`)
    expect(listed.status).toBe(200)
    expect(toolNames(listed.body)).toEqual([`timesheet__add_entry`, `timesheet__find_customer`, `timesheet__list_entries`, `timesheet__log_comment`])
    const listEntries = (listed.body.result!.tools as { name: string; annotations: Record<string, unknown>; _meta: Record<string, unknown> }[]).find((t) => t.name === `timesheet__list_entries`)!
    expect(listEntries.annotations.readOnlyHint).toBe(true)
    expect(listEntries._meta[`at.exponential/app`]).toBe(`timesheet`)

    // A records write, then a read that sees it.
    const added = await rpc(token, `tools/call`, { name: `timesheet__add_entry`, arguments: { day: `2026-10-31`, hours: 3 } })
    expect(added.body.result?.isError, text(added.body)).toBeFalsy()
    const [record] = await db.select({ data: appRecords.data }).from(appRecords).where(and(eq(appRecords.appId, w.appId), eq(appRecords.kind, `entry`)))
    expect(record?.data).toEqual({ day: `2026-10-31`, hours: 3 })
    const read = await rpc(token, `tools/call`, { name: `timesheet__list_entries`, arguments: { day: `2026-10-31` } })
    expect((read.body.result?.structuredContent as { count: number }).count).toBe(1)
    expect((read.body.result?._meta as Record<string, unknown>)[`at.exponential/tool`]).toBe(`list_entries`)

    // A granted action runs as the member, confined to the team.
    const commented = await rpc(token, `tools/call`, { name: `timesheet__log_comment`, arguments: { issue: w.issueId, hours: 3 } })
    expect(commented.body.result?.isError, text(commented.body)).toBeFalsy()
    const [comment] = await db
      .select({ body: comments.body, authorId: comments.authorId, source: comments.source, viaAppId: comments.viaAppId })
      .from(comments)
      .where(eq(comments.issueId, w.issueId))
    // #VAPP-186: written as the member, attributed "via Timesheet".
    expect(comment).toEqual({ body: `Logged 3 h`, authorId: w.owner, source: `mcp`, viaAppId: w.appId })

    // An upstream the team does not have answers upstreamMissing.
    const missing = await rpc(token, `tools/call`, { name: `timesheet__find_customer`, arguments: { q: `acme` } })
    expect(missing.body.result?.isError).toBe(true)
    expect(text(missing.body)).toMatch(/^upstreamMissing: /)

    // Invalid arguments are refused before anything runs.
    const invalid = await rpc(token, `tools/call`, { name: `timesheet__add_entry`, arguments: { day: `2026-11-01` } })
    expect(text(invalid.body)).toMatch(/^invalidArguments: missing hours/)

    const audit = await auditOf(clientId, 5)
    expect(audit).toHaveLength(5)
    for (const row of audit) {
      expect(row).toMatchObject({ teamId: w.teamId, userId: w.owner, clientId, appId: w.appId })
      expect(row.argsHash).toMatch(/^sha256:/)
    }
    const byTool = Object.fromEntries(audit.map((r) => [`${r.tool}:${r.status}`, r.error]))
    expect(byTool).toMatchObject({
      "timesheet__list_entries:ok": null,
      "timesheet__log_comment:ok": null,
      "timesheet__find_customer:upstream": `upstreamMissing`,
      "timesheet__add_entry:invalid": `invalidArguments`,
    })
  })

  it(`a read grant never sees or calls a write tool`, async () => {
    const w = await world()
    const { clientId, token } = await client(w.owner)
    await grant(w.owner, clientId, w.teamId, `read`)

    expect(toolNames((await rpc(token, `tools/list`)).body)).toEqual([`timesheet__find_customer`, `timesheet__list_entries`])
    const refused = await rpc(token, `tools/call`, { name: `timesheet__add_entry`, arguments: { day: `x`, hours: 1 } })
    expect(refused.body.error?.message).toContain(`not found`)
    expect(await db.select().from(appRecords).where(eq(appRecords.appId, w.appId))).toHaveLength(0)
    const [row] = await auditOf(clientId, 1)
    expect(row).toMatchObject({ tool: `timesheet__add_entry`, status: `denied`, error: `unknownTool`, appId: null })
  })

  it(`an action the app was not granted is not listed and never runs`, async () => {
    const w = await world({ grantComments: false })
    const { clientId, token } = await client(w.owner)
    await grant(w.owner, clientId, w.teamId, `full`)

    expect(toolNames((await rpc(token, `tools/list`)).body)).not.toContain(`timesheet__log_comment`)
    const refused = await rpc(token, `tools/call`, { name: `timesheet__log_comment`, arguments: { issue: w.issueId, hours: 1 } })
    expect(refused.body.error?.message).toContain(`not found`)
    expect(await db.select().from(comments).where(eq(comments.issueId, w.issueId))).toHaveLength(0)
  })

  it(`a member who left the team sees nothing; a trashed app drops its tools`, async () => {
    const w = await world()
    const { clientId, token } = await client(w.owner)
    await grant(w.owner, clientId, w.teamId, `full`)
    await db.update(apps).set({ deletedAt: new Date() }).where(eq(apps.id, w.appId))
    expect(toolNames((await rpc(token, `tools/list`)).body)).toEqual([])
    await db.update(apps).set({ deletedAt: null }).where(eq(apps.id, w.appId))
    await db.delete(teamMembers).where(and(eq(teamMembers.teamId, w.teamId), eq(teamMembers.userId, w.owner)))
    expect(toolNames((await rpc(token, `tools/list`)).body)).toEqual([])
  })
})

describe.skipIf(!ready.ok)(`the apps consent (real db, VAPP-182)`, () => {
  it(`offers the member's apps with tool counts and upstream states, clamps and writes the grant`, async () => {
    const { appsConsentTree, clampAppsSelection, writeAppsGrant } = await import(`./apps-consent`)
    const w = await world()
    const tree = await appsConsentTree(w.owner)
    const team = tree.find((t) => t.id === w.teamId)!
    expect(team.apps).toEqual([
      expect.objectContaining({ id: w.appId, slug: `timesheet`, toolCount: 4, upstreams: [{ name: `m5`, serverId: null, auth: null, state: null }] }),
    ])

    // A stranger's team and an unknown app id are clamped away.
    const outsider = await user()
    expect(await clampAppsSelection(outsider, { teamId: w.teamId, allApps: true, appIds: [], access: `full` })).toBeNull()
    expect(await clampAppsSelection(w.owner, { teamId: w.teamId, allApps: false, appIds: [crypto.randomUUID()], access: `full` })).toBeNull()
    const selection = await clampAppsSelection(w.owner, { teamId: w.teamId, allApps: false, appIds: [w.appId, w.appId], access: `read` })
    expect(selection).toEqual({ teamId: w.teamId, allApps: false, appIds: [w.appId], access: `read` })

    // The grant is what the endpoint reads: a read grant lists read tools only.
    const { clientId, token } = await client(w.owner)
    await writeAppsGrant(w.owner, clientId, selection!)
    expect(toolNames((await rpc(token, `tools/list`)).body)).toEqual([`timesheet__find_customer`, `timesheet__list_entries`])
    await writeAppsGrant(w.owner, clientId, { ...selection!, access: `full` })
    expect(toolNames((await rpc(token, `tools/list`)).body)).toHaveLength(4)
    expect(await db.select().from(mcpGrants).where(eq(mcpGrants.clientId, clientId))).toHaveLength(1)
  })
})

describe.skipIf(!ready.ok)(`the member policy and the tool switch (real db, VAPP-183)`, () => {
  const asUser = async (id: string) => {
    const { caller } = await import(`./tools`)
    const [row] = await db.execute(sql`SELECT id, name, email, image, email_verified AS "emailVerified", is_admin AS "isAdmin" FROM users WHERE id = ${id}`).then((r) => r.rows)
    return caller(row as never, new Request(`http://localhost/api/trpc`))
  }

  it(`a member's Read only policy caps a full grant; None hides the app; the owner switches a tool off`, async () => {
    const w = await world()
    const mate = await user()
    await db.insert(teamMembers).values({ teamId: w.teamId, userId: mate, role: `member` })
    const { clientId, token } = await client(mate)
    await grant(mate, clientId, w.teamId, `full`)
    expect(toolNames((await rpc(token, `tools/list`)).body)).toHaveLength(4)

    const owner = await asUser(w.owner)
    const member = await asUser(mate)
    // Only an owner sets a level; a member reads only their own row.
    await expect(member.appAccess.set({ appId: w.appId, userId: mate, level: `write` })).rejects.toThrow(/owner/)
    await owner.appAccess.set({ appId: w.appId, userId: mate, level: `read` })
    expect(toolNames((await rpc(token, `tools/list`)).body)).toEqual([`timesheet__find_customer`, `timesheet__list_entries`])
    const mine = await member.appAccess.list({ teamId: w.teamId })
    expect(mine.canEdit).toBe(false)
    expect(mine.rows.map((r) => [r.userId, r.levels[w.appId]])).toEqual([[mate, `read`]])
    expect((await owner.appAccess.list({ teamId: w.teamId })).rows).toHaveLength(2)

    await owner.appAccess.set({ appId: w.appId, userId: mate, level: `none` })
    expect(toolNames((await rpc(token, `tools/list`)).body)).toEqual([])
    // Full = no row.
    await owner.appAccess.set({ appId: w.appId, userId: mate, level: `write` })
    expect(await db.select().from(appAccess).where(eq(appAccess.appId, w.appId))).toHaveLength(0)

    await expect(member.appAccess.setToolEnabled({ appId: w.appId, tool: `add_entry`, enabled: false })).rejects.toThrow(/owner/)
    await owner.appAccess.setToolEnabled({ appId: w.appId, tool: `add_entry`, enabled: false })
    expect(toolNames((await rpc(token, `tools/list`)).body)).toEqual([`timesheet__find_customer`, `timesheet__list_entries`, `timesheet__log_comment`])
    const tools = await member.appAccess.appTools({ appId: w.appId })
    expect(tools.canToggle).toBe(false)
    expect(tools.tools.find((t) => t.name === `add_entry`)).toMatchObject({ enabled: false, access: `write`, via: `records` })
    expect(tools.upstreams).toEqual([{ name: `m5`, serverId: null, auth: null, state: null }])
  })
})
