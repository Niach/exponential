// @vitest-environment node
// #VAPP-175 (the end-to-end check of #VAPP-181): `/api/mcp/apps` against the
// REAL migrated Postgres, through the route's own POST handler (the gate,
// the OAuth bearer, the `apps` grant, the broker, the audit). A member's
// OAuth client lists and calls an app's tools; a read grant, a missing grant,
// an ungranted action and a missing upstream are refused; every tools/call
// leaves its `mcp_calls` row naming the app, the team and the client.
import { hashOAuthToken } from "@/lib/auth/oauth-token-hash"
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { appAccess, apps, appRecords, comments, issues, mcpCalls, mcpGrants, mcpServers, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { createTestBoard } from "@/test/factories"
import { VIA_APP_RECORDS_FUNCTION, type ViaAppCall } from "@exp/domain-contract/via-app"
import { verifiedViaApp } from "@/lib/apps/via-app-claim"

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
    INSERT INTO oauth2_clients (id, name, client_id, redirect_uris, token_endpoint_auth_method, disabled, created_at, updated_at)
    VALUES (${clientId}, 'Test client', ${clientId}, ARRAY['http://localhost/cb'], 'none', false, now(), now())
  `)
  await db.execute(sql`
    INSERT INTO oauth2_access_tokens (id, token, client_id, user_id, scopes, expires_at, created_at)
    VALUES (${`at-${id8()}`}, ${hashOAuthToken(token)}, ${clientId}, ${userId}, ARRAY['openid'], now() + interval '1 hour', now())
  `)
  made.clients.push(clientId)
  return { clientId, token }
}

const PACKAGE = {
  records: { entry: { schema: { type: `object`, properties: { day: { type: `string` }, hours: { type: `number` } } } } },
  upstreams: [`crm`],
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
      description: `Search CRM customers.`,
      input: { type: `object`, properties: { q: { type: `string` } } },
      access: `read`,
      run: { upstream: `crm`, tool: `search_customers`, args: { query: `\${input.q}` } },
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
    await db.execute(sql`DELETE FROM oauth2_clients WHERE client_id IN ${sql.raw(`('${made.clients.join(`','`)}')`)}`)
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
    const [record] = await db.select({ data: appRecords.data, viaAppId: appRecords.viaAppId }).from(appRecords).where(and(eq(appRecords.appId, w.appId), eq(appRecords.kind, `entry`)))
    expect(record?.data).toEqual({ day: `2026-10-31`, hours: 3 })
    // VAPP-229: the app's own exposed tool created it = "via Timesheet".
    expect(record?.viaAppId).toBe(w.appId)
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

  // VAPP-229: an issue an app's granted action files reads "via <app>";
  // the member's own create (no broker) carries none.
  it(`an issue an app's action creates is stamped with the app`, async () => {
    const w = await world()
    const board = await createTestBoard({ teamId: w.teamId, prefix: `FIL` })
    const [filer] = await db
      .insert(apps)
      .values({
        teamId: w.teamId,
        slug: `filer`,
        name: `Filer`,
        headVersion: 1,
        createdBy: w.owner,
        grantedTools: [`exponential.issues.create`],
        package: {
          functions: [`exponential.issues.create`],
          exposes: [
            {
              name: `file_issue`,
              title: `File an issue`,
              description: `File an issue on a board.`,
              input: { type: `object`, properties: { title: { type: `string` } }, required: [`title`] },
              access: `write`,
              run: { action: `exponential.issues.create`, args: { boardId: board.id, title: `\${input.title}` } },
            },
          ],
        } as never,
      })
      .returning({ id: apps.id })
    const { clientId, token } = await client(w.owner)
    await grant(w.owner, clientId, w.teamId, `full`)
    const filed = await rpc(token, `tools/call`, { name: `filer__file_issue`, arguments: { title: `From the app` } })
    expect(filed.body.result?.isError, text(filed.body)).toBeFalsy()
    const [issue] = await db.select({ viaAppId: issues.viaAppId, creatorId: issues.creatorId }).from(issues).where(and(eq(issues.boardId, board.id), eq(issues.title, `From the app`)))
    expect(issue).toEqual({ viaAppId: filer!.id, creatorId: w.owner })
    const [plain] = await db.select({ viaAppId: issues.viaAppId }).from(issues).where(eq(issues.id, w.issueId))
    expect(plain!.viaAppId).toBeNull()
  })

  // VAPP-231 (via-app.json `claim`): a host's claim adds a caption only when it holds.
  it(`a claim holds for a member, a published version and a declared function`, async () => {
    const w = await world()
    const [app] = await db
      .insert(apps)
      .values({ teamId: w.teamId, slug: `claimed`, name: `Claimed`, headVersion: 2, publishedVersion: 1, createdBy: w.owner, package: { formatVersion: 5, functions: [`records.*`, `harness.mcp`, `exponential.issues.create`] } as never })
      .returning({ id: apps.id })
    const appId = app!.id
    const check = (userId: string, version: number, fn: ViaAppCall) => verifiedViaApp(userId, { appId, version }, [fn])
    // Round 9 §14: a stored format 5 list, judged by the flat names.
    expect(await check(w.owner, 1, { fn: VIA_APP_RECORDS_FUNCTION })).toBe(appId)
    expect(await check(w.owner, 1, { tool: `exponential_issues`, op: `create` })).toBe(appId)
    expect(await check(w.owner, 2, { fn: VIA_APP_RECORDS_FUNCTION })).toBeNull()
    expect(await check(`not-a-member`, 1, { fn: VIA_APP_RECORDS_FUNCTION })).toBeNull()
    expect(await verifiedViaApp(w.owner, { appId: `00000000-0000-4000-8000-000000000000`, version: 1 }, [{ fn: VIA_APP_RECORDS_FUNCTION }])).toBeNull()
  })

  // VAPP-187: the broker's records patch runs the records router's ownership
  // rule: another member's row is refused (FORBIDDEN → toolError), the
  // caller's own row patches.
  it(`a records patch on another member's row is refused`, async () => {
    const w = await world()
    const mate = await user()
    await db.insert(teamMembers).values({ teamId: w.teamId, userId: mate, role: `member` })
    const [fixer] = await db
      .insert(apps)
      .values({
        teamId: w.teamId,
        slug: `fixer`,
        name: `Fixer`,
        headVersion: 1,
        createdBy: w.owner,
        package: {
          records: PACKAGE.records,
          exposes: [
            {
              name: `fix_entry`,
              title: `Fix an entry`,
              description: `Correct the hours of an entry.`,
              input: { type: `object`, properties: { id: { type: `string` }, hours: { type: `number` } }, required: [`id`, `hours`] },
              access: `write`,
              run: { records: { op: `upsert`, kind: `entry`, id: `\${input.id}`, data: { hours: `\${input.hours}` } } },
            },
          ],
        } as never,
      })
      .returning({ id: apps.id })
    const [theirs] = await db.insert(appRecords).values({ appId: fixer!.id, teamId: w.teamId, kind: `entry`, data: { day: `2026-10-31`, hours: 3 }, createdBy: w.owner, authorId: w.owner, updatedBy: w.owner }).returning({ id: appRecords.id })
    const [own] = await db.insert(appRecords).values({ appId: fixer!.id, teamId: w.teamId, kind: `entry`, data: { day: `2026-10-31`, hours: 1 }, createdBy: mate, authorId: mate, updatedBy: mate }).returning({ id: appRecords.id })
    const { clientId, token } = await client(mate)
    await grant(mate, clientId, w.teamId, `full`)

    const refused = await rpc(token, `tools/call`, { name: `fixer__fix_entry`, arguments: { id: theirs!.id, hours: 9 } })
    expect(refused.body.result?.isError).toBe(true)
    expect(text(refused.body)).toBe(`toolError: Only the record's author or a team owner can change it`)
    const [untouched] = await db.select({ data: appRecords.data }).from(appRecords).where(eq(appRecords.id, theirs!.id))
    expect(untouched!.data).toEqual({ day: `2026-10-31`, hours: 3 })

    const fixed = await rpc(token, `tools/call`, { name: `fixer__fix_entry`, arguments: { id: own!.id, hours: 2 } })
    expect(fixed.body.result?.isError, text(fixed.body)).toBeFalsy()
    const [after] = await db.select({ data: appRecords.data, updatedBy: appRecords.updatedBy }).from(appRecords).where(eq(appRecords.id, own!.id))
    expect(after).toEqual({ data: { day: `2026-10-31`, hours: 2 }, updatedBy: mate })
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
      expect.objectContaining({ id: w.appId, slug: `timesheet`, toolCount: 4, upstreams: [{ name: `crm`, serverId: null, serverName: null, auth: null, state: null }] }),
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
    expect(tools.upstreams).toEqual([{ name: `crm`, serverId: null, serverName: null, auth: null, state: null }])
    expect(tools.servers).toEqual([])

    // #VAPP-254: an owner maps another server; the row and audit name it.
    const [hubspot] = await db.insert(mcpServers).values({ teamId: w.teamId, name: `hubspot`, transport: `http`, url: `https://hubspot.example/mcp`, auth: `none` }).returning({ id: mcpServers.id })
    await expect(member.appAccess.setUpstreamMapping({ appId: w.appId, upstream: `crm`, serverId: hubspot!.id })).rejects.toThrow(/owner/)
    await expect(owner.appAccess.setUpstreamMapping({ appId: w.appId, upstream: `stripe`, serverId: hubspot!.id })).rejects.toThrow(/declare/)
    await expect(owner.appAccess.setUpstreamMapping({ appId: w.appId, upstream: `crm`, serverId: crypto.randomUUID() })).rejects.toThrow(/team/)
    await owner.appAccess.setUpstreamMapping({ appId: w.appId, upstream: `crm`, serverId: hubspot!.id })
    expect((await member.appAccess.appTools({ appId: w.appId })).upstreams[0]).toMatchObject({ name: `crm`, serverId: hubspot!.id, serverName: `hubspot` })
    expect((await owner.appAccess.appTools({ appId: w.appId })).servers).toEqual([{ id: hubspot!.id, name: `hubspot` }])
    await owner.appAccess.setUpstreamMapping({ appId: w.appId, upstream: `crm`, serverId: null })
    expect((await member.appAccess.appTools({ appId: w.appId })).upstreams[0]).toMatchObject({ serverId: null })
    const audited = await db.select({ tool: mcpCalls.tool }).from(mcpCalls).where(eq(mcpCalls.appId, w.appId))
    expect(audited.map((r) => r.tool)).toEqual(expect.arrayContaining([`upstream:crm=hubspot`, `upstream:crm=auto`]))
  })
})
