// @vitest-environment node
// #NEW-147: the MCP 2026-07-28 extensions on `/api/mcp` against the REAL
// migrated Postgres, through the route and the gate, as an external
// 2026-07-28 host would speak them: skills (packages/emcp fixtures/
// skills.json), elicitation (elicitation.json: the delete confirm, the
// device pick), tasks unified with the watches (tasks.json: a start returns
// a task, a plan card is `input_required`, `tasks/update` answers it, the
// report completes it, `tasks/cancel` stops the session), the single-use
// requestState across a "restart" and the `mcp_calls.method` audit.
import { hashOAuthToken } from "@/lib/auth/oauth-token-hash"
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import fixture from "@exp/emcp/fixtures/protocol.json" with { type: "json" }
import elicitation from "@exp/emcp/fixtures/elicitation.json" with { type: "json" }
import tasksFixture from "@exp/emcp/fixtures/tasks.json" with { type: "json" }
import { db } from "@/db/connection"
import { boards, devices, issues, mcpCalls, mcpGrants, mcpTasks, sessions, sessionWatches, skills, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

process.env.BETTER_AUTH_SECRET ||= `extensions-test-secret-0123456789abcdef`

vi.mock(`@/lib/conversion/events`, () => ({ recordConversionEvent: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  fireAndForgetIssueNotify: () => {},
  notifySessionBlocked: async () => {},
  notifySessionAsk: async () => {},
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: true }),
}))

const ready = await probeRealDb(`SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'mcp_tasks') AS ok`)
if (!ready.ok) console.info(`[extensions.real-db] skipped: ${ready.reason}`)

const { Route } = await import(`@/routes/api/mcp`)
const post = (Route.options.server as { handlers: { POST: (ctx: { request: Request }) => Promise<Response> } }).handlers.POST
const { endSession } = await import(`@/lib/sessions/lifecycle`)
const { busAsk } = await import(`@/lib/sessions/bus`)
const { postOwnAskCard } = await import(`@/lib/sessions/messages`)
const { dbRedeemStore, setRedeemStore } = await import(`./mrtr`)
const { resetTaskRateWindows } = await import(`./tasks`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[], clients: [] as string[] }

async function world() {
  const userId = `ext-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${userId}, 'Tester', ${`${userId}@extensions.test`}, true, false, now(), now())
  `)
  made.users.push(userId)
  const slug = `ext-${id8()}`
  const [team] = await db.insert(teams).values({ name: `Extensions`, slug }).returning({ id: teams.id })
  made.teams.push(team!.id)
  await db.insert(teamMembers).values({ teamId: team!.id, userId, role: `owner` })
  // A team the token is NOT granted: its skills are never served.
  const [other] = await db.insert(teams).values({ name: `Other`, slug: `other-${id8()}` }).returning({ id: teams.id })
  made.teams.push(other!.id)
  await db.insert(teamMembers).values({ teamId: other!.id, userId, role: `owner` })
  await db.insert(skills).values([
    { teamId: team!.id, name: `triage`, description: `Triage new bugs.`, body: `# Triage\n\nLabel, then prioritise.\n` },
    { teamId: other!.id, name: `secret-sauce`, description: `Not yours.`, body: `x` },
  ])
  const clientId = `client-${id8()}`
  await db.execute(sql`
    INSERT INTO oauth2_clients (id, name, client_id, redirect_uris, token_endpoint_auth_method, disabled, created_at, updated_at)
    VALUES (${clientId}, 'Claude Code', ${clientId}, ARRAY['http://localhost/cb'], 'none', false, now(), now())
  `)
  made.clients.push(clientId)
  const token = `tok-${crypto.randomUUID()}`
  await db.execute(sql`
    INSERT INTO oauth2_access_tokens (id, token, client_id, user_id, scopes, expires_at, created_at)
    VALUES (${`at-${id8()}`}, ${hashOAuthToken(token)}, ${clientId}, ${userId}, ARRAY['openid'], now() + interval '1 hour', now())
  `)
  await db.insert(mcpGrants).values({ userId, clientId, kind: `builtin`, teamIds: [team!.id] })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId, deviceId, label: `box`, kind: `server`, lastSeenAt: new Date() })
  const [board] = await db.insert(boards).values({ teamId: team!.id, name: `Core`, slug: `core-${id8()}`, prefix: `EX` }).returning({ id: boards.id })
  return { userId, token, teamId: team!.id, slug, deviceId, boardId: board!.id, clientId }
}

type Rpc = { result?: Record<string, unknown>; error?: { code: number; message: string; data?: unknown } }

const envelope = (capabilities: Record<string, unknown>) => ({
  [fixture.metaKeys.protocolVersion]: `2026-07-28`,
  [fixture.metaKeys.clientInfo]: { name: `claude-code`, version: `2.1.332` },
  [fixture.metaKeys.clientCapabilities]: capabilities,
})

const CAPS = { elicitation: { form: {}, url: {} }, extensions: { [tasksFixture.extension]: {} } }

let nextId = 1
async function rpc(token: string, method: string, params: Record<string, unknown>, caps: Record<string, unknown> = CAPS): Promise<Rpc> {
  const headers: Record<string, string> = {
    authorization: `Bearer ${token}`,
    "content-type": `application/json`,
    accept: `application/json, text/event-stream`,
    "mcp-protocol-version": `2026-07-28`,
    "mcp-method": method,
  }
  if (method === `tools/call` || method === `prompts/get`) headers[`mcp-name`] = String(params.name)
  if (method === `resources/read`) headers[`mcp-name`] = String(params.uri)
  if (method.startsWith(`tasks/`)) headers[`mcp-name`] = String(params.taskId)
  const body = { jsonrpc: `2.0`, id: nextId++, method, params: { ...params, _meta: envelope(caps) } }
  const response = await post({ request: new Request(`http://localhost/api/mcp`, { method: `POST`, headers, body: JSON.stringify(body) }) })
  return (await response.json()) as Rpc
}

const call = (token: string, name: string, args: Record<string, unknown>, extra: Record<string, unknown> = {}, caps?: Record<string, unknown>) =>
  rpc(token, `tools/call`, { name, arguments: args, ...extra }, caps)

const text = (r: Rpc) => ((r.result?.content as { text: string }[] | undefined) ?? [])[0]?.text ?? ``

afterAll(async () => {
  if (!ready.ok) return
  if (made.clients.length) await db.execute(sql`DELETE FROM oauth2_clients WHERE client_id IN ${sql.raw(`('${made.clients.join(`','`)}')`)}`)
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`MCP extensions on /api/mcp (real db, #NEW-147)`, () => {
  it(`skills: the granted team's own skills and the built-ins, never another team's; list, get, read, directory`, async () => {
    const w = await world()
    const discover = await rpc(w.token, `server/discover`, {})
    expect((discover.result?.capabilities as { extensions: Record<string, unknown> }).extensions[`io.modelcontextprotocol/skills`]).toEqual({ directoryRead: true })
    const list = await rpc(w.token, `skills/list`, {})
    const uris = (list.result?.skills as { uri: string }[]).map((s) => s.uri)
    expect(uris).toEqual([`skill://${w.slug}/triage/SKILL.md`, `skill://exponential/exponential-genui/SKILL.md`, `skill://exponential/exponential-workflow/SKILL.md`])
    expect(list.result?.cacheScope).toBe(`private`)
    expect(list.result?.ttlMs).toBe(300000)
    const get = await rpc(w.token, `skills/get`, { uri: `skill://${w.slug}/triage/SKILL.md` })
    expect((get.result?.skill as { frontmatter: unknown }).frontmatter).toEqual({ name: `triage`, description: `Triage new bugs.` })
    const read = await rpc(w.token, `resources/read`, { uri: `skill://exponential/exponential-genui/SKILL.md` })
    const contents = read.result?.contents as { text: string; mimeType: string }[]
    expect(contents[0]!.mimeType).toBe(`text/markdown`)
    expect(contents[0]!.text).toContain(`name: exponential-genui`)
    expect(read.result?.cacheScope).toBe(`public`)
    const dir = await rpc(w.token, `resources/directory/read`, { uri: `skill://exponential/exponential-genui` })
    expect((dir.result?.resources as { name: string }[]).map((r) => r.name)).toEqual([`SKILL.md`, `references`])
    // Another team's skill reads exactly like one that does not exist.
    const [otherSlug] = await db.execute(sql`SELECT t.slug FROM teams t JOIN skills s ON s.team_id = t.id WHERE s.name = 'secret-sauce' AND t.id = ANY(${`{${made.teams.join(`,`)}}`}::uuid[]) LIMIT 1`).then((r) => r.rows as { slug: string }[])
    const hidden = await rpc(w.token, `skills/get`, { uri: `skill://${otherSlug!.slug}/secret-sauce/SKILL.md` })
    expect(hidden.error).toEqual({ code: -32602, message: `No skill is served at skill://${otherSlug!.slug}/secret-sauce/SKILL.md` })
    // Every method is audited with its method.
    const rows = await db.select({ method: mcpCalls.method, tool: mcpCalls.tool }).from(mcpCalls).where(eq(mcpCalls.userId, w.userId))
    await new Promise((resolve) => setTimeout(resolve, 50))
    const methods = (await db.select({ method: mcpCalls.method }).from(mcpCalls).where(eq(mcpCalls.userId, w.userId))).map((r) => r.method)
    expect(rows.length + methods.length).toBeGreaterThan(0)
    expect(methods).toEqual(expect.arrayContaining([`server/discover`, `skills/list`, `skills/get`, `resources/read`, `resources/directory/read`]))
  })

  it(`a delete asks first: decline keeps it, accept deletes it, the state never replays (even after a restart)`, async () => {
    const w = await world()
    const [issue] = await db
      .insert(issues)
      .values({ teamId: w.teamId, boardId: w.boardId, number: 1, identifier: `EX-1`, title: `Doomed`, status: `backlog`, creatorId: w.userId, sortOrder: 0 } as never)
      .returning({ id: issues.id })
    const args = { op: `delete`, id: issue!.id }
    const asked = await call(w.token, `exponential_issues_write`, args)
    expect(asked.result?.resultType).toBe(`input_required`)
    const request = (asked.result?.inputRequests as Record<string, { params: { message: string } }>)[elicitation.confirm.key]!
    expect(request.params.message).toBe(`Delete issue ${issue!.id}?`)
    const declined = await call(w.token, `exponential_issues_write`, args, {
      inputResponses: { confirm: { action: `decline` } },
      requestState: asked.result?.requestState,
    })
    expect(text(declined)).toBe(elicitation.results.declined)
    expect((await db.select({ deletedAt: issues.deletedAt }).from(issues).where(eq(issues.id, issue!.id)))[0]!.deletedAt).toBeNull()

    const again = await call(w.token, `exponential_issues_write`, args)
    const accept = { inputResponses: { confirm: { action: `accept`, content: { confirm: true } } }, requestState: again.result?.requestState }
    const done = await call(w.token, `exponential_issues_write`, args, accept)
    expect(done.result?.isError).toBeFalsy()
    expect((await db.select({ deletedAt: issues.deletedAt }).from(issues).where(eq(issues.id, issue!.id)))[0]!.deletedAt).not.toBeNull()
    // A new store (another process, a restart) still refuses the spent state.
    setRedeemStore(dbRedeemStore())
    const replay = await call(w.token, `exponential_issues_write`, args, accept)
    expect(replay.error?.message).toBe(fixture.errors.invalidRequestState)
    // A client without elicitation deletes as today (no ask).
    const [second] = await db
      .insert(issues)
      .values({ teamId: w.teamId, boardId: w.boardId, number: 2, identifier: `EX-2`, title: `Doomed too`, status: `backlog`, creatorId: w.userId, sortOrder: 1 } as never)
      .returning({ id: issues.id })
    const plain = await call(w.token, `exponential_issues_write`, { op: `delete`, id: second!.id }, {}, {})
    expect(plain.result?.resultType).toBe(`complete`)
  })

  it(`tasks: a start returns a task, a plan card is input_required, tasks/update answers it, the report completes it`, async () => {
    resetTaskRateWindows()
    const w = await world()
    const started = await call(w.token, `exponential_sessions_send`, { teamId: w.teamId, deviceId: w.deviceId, text: `Ship it.` })
    expect(started.result?.resultType).toBe(`task`)
    const taskId = started.result?.taskId as string
    expect(taskId).toMatch(/^tsk_[A-Za-z0-9_-]{43}$/)
    expect(started.result?.status).toBe(`working`)
    const [task] = await db.select().from(mcpTasks).where(eq(mcpTasks.id, taskId))
    const sessionId = task!.ref!
    const watchers = await db.select().from(sessionWatches).where(eq(sessionWatches.taskId, taskId))
    expect(watchers).toMatchObject([{ watcherKind: `mcp_task`, sessionId: null, kind: `session`, subjectId: sessionId }])

    const got = await rpc(w.token, `tasks/get`, { taskId })
    expect(got.result).toMatchObject({ taskId, status: `working`, resultType: `complete` })

    // The session proposes a plan: the task asks it as a form.
    const [row] = await db.select().from(sessions).where(eq(sessions.id, sessionId))
    const card = { kind: `plan` as const, plan: `1. Do it.\n2. Ship it.`, questions: [{ id: `db`, prompt: `Which database?`, options: [{ label: `Postgres` }, { label: `SQLite` }] }] }
    const messageId = await db.transaction(async (tx) => {
      const id = await postOwnAskCard(tx, row as never, card as never)
      await busAsk(tx, row as never, id, card as never)
      return id
    })
    const asking = await rpc(w.token, `tasks/get`, { taskId })
    expect(asking.result?.status).toBe(`input_required`)
    const request = (asking.result?.inputRequests as Record<string, { params: { requestedSchema: { properties: Record<string, unknown> } } }>)[messageId]!
    expect(Object.keys(request.params.requestedSchema.properties)).toEqual([`decision`, `q_db`])

    const updated = await rpc(w.token, `tasks/update`, { taskId, inputResponses: { [messageId]: { action: `accept`, content: { decision: `approved`, q_db: `Postgres` } } } })
    expect(updated.result).toEqual({ resultType: `complete`, _meta: expect.anything() })
    const back = await rpc(w.token, `tasks/get`, { taskId })
    expect(back.result?.status).toBe(`working`)

    await endSession({ userId: w.userId, sessionId, summary: `Opened PR #42: https://github.com/org/repo/pull/42` })
    const done = await rpc(w.token, `tasks/get`, { taskId })
    expect(done.result?.status).toBe(`completed`)
    expect(done.result?.result).toEqual({ content: [{ type: `text`, text: `Opened PR #42: https://github.com/org/repo/pull/42` }], isError: false })
    expect(await db.select().from(sessionWatches).where(eq(sessionWatches.taskId, taskId))).toEqual([])
  })

  it(`tasks: cancel stops the session; another principal's task is not found; no extension = today's answer`, async () => {
    resetTaskRateWindows()
    const w = await world()
    const started = await call(w.token, `exponential_sessions_send`, { teamId: w.teamId, deviceId: w.deviceId, text: `Long job.` })
    const taskId = started.result?.taskId as string
    const [task] = await db.select().from(mcpTasks).where(eq(mcpTasks.id, taskId))
    // Make it look running on an online device so the stop is delivered.
    await db.update(sessions).set({ status: `running`, busy: true }).where(eq(sessions.id, task!.ref!))
    const other = await world()
    const foreign = await rpc(other.token, `tasks/get`, { taskId })
    expect(foreign.error).toEqual({ code: -32602, message: tasksFixture.errors.notFound })

    const cancelled = await rpc(w.token, `tasks/cancel`, { taskId })
    expect(cancelled.result?.resultType).toBe(`complete`)
    const after = await rpc(w.token, `tasks/get`, { taskId })
    expect(after.result).toMatchObject({ status: `cancelled`, statusMessage: tasksFixture.copy.cancelled })
    // The stop reached the session: its busy turn is stamped stopped.
    expect((await db.select({ stoppedAt: sessions.stoppedAt }).from(sessions).where(eq(sessions.id, task!.ref!)))[0]!.stoppedAt).not.toBeNull()
    expect(await db.select().from(sessionWatches).where(eq(sessionWatches.taskId, taskId))).toEqual([])
    // The session itself stays (a stopped session, not an ended one).
    expect((await db.select({ status: sessions.status }).from(sessions).where(eq(sessions.id, task!.ref!)))[0]!.status).not.toBe(`ended`)

    const undeclared = await call(w.token, `exponential_sessions_send`, { teamId: w.teamId, deviceId: w.deviceId, text: `Plain.` }, {}, { elicitation: {} })
    expect(undeclared.result?.resultType).toBe(`complete`)
    expect(JSON.parse(text(undeclared)).sessionId).toBeTruthy()
    const missing = await rpc(w.token, `tasks/get`, { taskId }, { elicitation: {} })
    expect(missing.error?.code).toBe(tasksFixture.errorCodes.missingCapability)
  })

  it(`tasks: a wakeOn of event specs is one task over its event watches; the time watch firing completes it`, async () => {
    resetTaskRateWindows()
    const w = await world()
    const at = new Date(Date.now() + 60_000).toISOString()
    const started = await call(w.token, `exponential_sessions_send`, { wakeOn: [`at:${at}`] })
    expect(started.result?.resultType).toBe(`task`)
    const taskId = started.result?.taskId as string
    expect(await db.select({ kind: sessionWatches.kind, watcherKind: sessionWatches.watcherKind }).from(sessionWatches).where(eq(sessionWatches.taskId, taskId))).toEqual([
      { kind: `at`, watcherKind: `mcp_task` },
    ])
    const { fireDueTaskTimeWatches } = await import(`@/lib/sessions/task-watchers`)
    const { WATCH_COPY } = await import(`@/lib/sessions/watches`)
    expect(await fireDueTaskTimeWatches(new Date(Date.now() + 120_000), WATCH_COPY.at)).toBe(1)
    const done = await rpc(w.token, `tasks/get`, { taskId })
    expect(done.result?.status).toBe(`completed`)
    expect((done.result?.result as { content: { text: string }[] }).content[0]!.text).toMatch(/the time you asked to be woken at/)
  })

  it(`a new root without a device asks which one when several are online; one is used silently`, async () => {
    const w = await world()
    const second = `dev-${id8()}`
    await db.insert(devices).values({ userId: w.userId, deviceId: second, label: `another`, kind: `server`, lastSeenAt: new Date() })
    const args = { teamId: w.teamId, text: `Pick for me.` }
    const asked = await call(w.token, `exponential_sessions_send`, args, {}, { elicitation: {} })
    expect(asked.result?.resultType).toBe(`input_required`)
    const request = (asked.result?.inputRequests as Record<string, { params: { requestedSchema: { properties: { deviceId: { oneOf: { const: string }[] } } } } }>)[elicitation.devicePick.key]!
    const offered = request.params.requestedSchema.properties.deviceId.oneOf.map((o) => o.const)
    expect(offered.sort()).toEqual([w.deviceId, second].sort())
    const forged = await call(w.token, `exponential_sessions_send`, args, { inputResponses: { device: { action: `accept`, content: { deviceId: `nope` } } }, requestState: asked.result?.requestState }, { elicitation: {} })
    expect(forged.result?.isError).toBe(true)
    const again = await call(w.token, `exponential_sessions_send`, args, {}, { elicitation: {} })
    const picked = await call(w.token, `exponential_sessions_send`, args, { inputResponses: { device: { action: `accept`, content: { deviceId: second } } }, requestState: again.result?.requestState }, { elicitation: {} })
    const sessionId = JSON.parse(text(picked)).sessionId as string
    expect((await db.select({ deviceId: sessions.deviceId }).from(sessions).where(and(eq(sessions.id, sessionId))))[0]!.deviceId).toBe(second)
  })
})
