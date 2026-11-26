// @vitest-environment node
// #NEW-147: the 2026-07-28 extensions on `/api/mcp` against the REAL
// migrated Postgres, through the route and the gate, as an external host
// (an OAuth token, the per-request envelope): skills list, get and read
// (skills.json); a delete asks its confirm (elicitation.json); a message
// to a session returns a task whose card is a question, whose report
// completes it and whose cancel stops it (tasks.json); every method is an
// `mcp_calls` row with its `method` (audit.json).
import { hashOAuthToken } from "@/lib/auth/oauth-token-hash"
import { createHash } from "node:crypto"
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import protocol from "@exp/emcp/fixtures/protocol.json" with { type: "json" }
import tasksFixture from "@exp/emcp/fixtures/tasks.json" with { type: "json" }
import elicitation from "@exp/emcp/fixtures/elicitation.json" with { type: "json" }
import { db } from "@/db/connection"
import { devices, labels, mcpCalls, mcpGrants, mcpTasks, messages, sessions, sessionWatches, skills, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

vi.mock(`@/lib/conversion/events`, () => ({ recordConversionEvent: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
  notifySessionAsk: async () => {},
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  getSteerRelayConfig: () => null,
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
}))

const ready = await probeRealDb(`SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'mcp_tasks') AS ok`)
if (!ready.ok) console.info(`[extensions.real-db] skipped: ${ready.reason}`)

const { Route } = await import(`@/routes/api/mcp`)
const post = (Route.options.server as { handlers: { POST: (ctx: { request: Request }) => Promise<Response> } }).handlers.POST
const { insertMessage } = await import(`@/lib/sessions/messages`)
const { endSession } = await import(`@/lib/sessions/lifecycle`)
const { resetTaskRateWindows } = await import(`./tasks`)
const { invalidateMembershipCaches } = await import(`@/lib/auth/membership-cache`)

const MODERN = `2026-07-28`
const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[], clients: [] as string[] }

async function world() {
  const userId = `ext-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${userId}, 'Tester', ${`${userId}@ext.test`}, true, false, now(), now())
  `)
  made.users.push(userId)
  const slug = `ext-${id8()}`
  const [team] = await db.insert(teams).values({ name: `Ext`, slug }).returning({ id: teams.id })
  made.teams.push(team!.id)
  await db.insert(teamMembers).values({ teamId: team!.id, userId, role: `owner` })
  invalidateMembershipCaches()
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
  return { token, userId, clientId, teamId: team!.id, slug }
}

function envelope(capabilities: Record<string, unknown>) {
  return {
    [protocol.metaKeys.protocolVersion]: MODERN,
    [protocol.metaKeys.clientInfo]: { name: `claude-code`, version: `2.1.332` },
    [protocol.metaKeys.clientCapabilities]: capabilities,
  }
}

const HOST = { elicitation: { form: {}, url: {} }, extensions: { [tasksFixture.extension]: {} } }

type Rpc = { result?: Record<string, any>; error?: { code: number; message: string; data?: unknown } }

let rpcId = 0
async function rpc(token: string, method: string, params: Record<string, unknown> = {}, capabilities: Record<string, unknown> = HOST): Promise<Rpc> {
  const message = { jsonrpc: `2.0`, id: ++rpcId, method, params: { ...params, _meta: envelope(capabilities) } }
  const headers: Record<string, string> = {
    authorization: `Bearer ${token}`,
    "content-type": `application/json`,
    accept: `application/json, text/event-stream`,
    "mcp-protocol-version": MODERN,
    "mcp-method": method,
  }
  const name = (params.name ?? params.taskId ?? params.uri) as string | undefined
  if (typeof name === `string`) headers[`mcp-name`] = name
  const response = await post({ request: new Request(`http://localhost/api/mcp`, { method: `POST`, headers, body: JSON.stringify(message) }) })
  const text = await response.text()
  const json = text.startsWith(`event:`) || text.includes(`\ndata:`) ? JSON.parse(text.split(`\n`).find((line) => line.startsWith(`data:`))!.slice(5)) : JSON.parse(text)
  return json as Rpc
}

const call = (token: string, name: string, args: Record<string, unknown>, extra: Record<string, unknown> = {}, capabilities?: Record<string, unknown>) =>
  rpc(token, `tools/call`, { name, arguments: args, ...extra }, capabilities)

async function session(userId: string, teamId: string) {
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId, deviceId, label: `mac`, kind: `desktop` })
  const id = crypto.randomUUID()
  await db.insert(sessions).values({ id, rootId: id, depth: 0, status: `idle`, shared: false, title: `t`, teamId, userId, deviceId })
  return id
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.clients.length) await db.execute(sql`DELETE FROM oauth2_clients WHERE client_id IN ${sql.raw(`('${made.clients.join(`','`)}')`)}`)
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`/api/mcp extensions (real db, #NEW-147)`, () => {
  it(`skills: list, get and read a team skill and the built-ins`, async () => {
    const w = await world()
    await db.insert(skills).values({ teamId: w.teamId, name: `deploy`, description: `Ship it`, body: `Run the train.` })
    const list = await rpc(w.token, `skills/list`)
    expect(list.error).toBeUndefined()
    const uris = (list.result!.skills as { uri: string }[]).map((skill) => skill.uri)
    const own = `skill://${w.slug}/deploy/SKILL.md`
    expect(uris[0]).toBe(own)
    expect(uris.some((uri) => uri.startsWith(`skill://exponential/`))).toBe(true)
    expect(list.result!.cacheScope).toBe(`private`)

    const got = await rpc(w.token, `skills/get`, { uri: own })
    expect(got.error).toBeUndefined()
    const read = await rpc(w.token, `resources/read`, { uri: own })
    const text = read.result!.contents[0].text as string
    expect(text).toBe(`---\nname: deploy\ndescription: "Ship it"\n---\n\nRun the train.\n`)
    const manifest = (got.result!.resources ?? got.result!.skill?.resources) as { uri: string; digest: string }[]
    expect(manifest[0]!.digest).toBe(`sha256:${createHash(`sha256`).update(text).digest(`hex`)}`)

    // Another team's skill is answered like a missing one.
    const foreign = await rpc(w.token, `skills/get`, { uri: `skill://nobody-${id8()}/deploy/SKILL.md` })
    expect(foreign.error?.code).toBe(-32602)

    // Every method is audited with its method (audit.json `method`).
    await new Promise((resolve) => setTimeout(resolve, 200))
    const rows = await db.select({ method: mcpCalls.method, tool: mcpCalls.tool, status: mcpCalls.status }).from(mcpCalls).where(eq(mcpCalls.userId, w.userId))
    expect(rows.map((row) => row.method).sort()).toEqual([`resources/read`, `skills/get`, `skills/get`, `skills/list`])
    expect(rows.find((row) => row.method === `resources/read`)!.tool).toBe(own)
  })

  it(`a delete asks for confirmation; accept runs it, decline runs nothing`, async () => {
    const w = await world()
    const [label] = await db.insert(labels).values({ teamId: w.teamId, name: `doomed-${id8()}`, color: `#ff0000` }).returning({ id: labels.id })
    const args = { op: `delete`, id: label!.id }
    const first = await call(w.token, `exponential_labels_write`, args)
    expect(first.result!.resultType).toBe(`input_required`)
    const [key, request] = Object.entries(first.result!.inputRequests as Record<string, { method: string; params: { message: string; requestedSchema: unknown } }>)[0]!
    expect(request.method).toBe(`elicitation/create`)
    expect(request.params.requestedSchema).toEqual(elicitation.confirm.schema)

    const declined = await call(w.token, `exponential_labels_write`, args, {
      inputResponses: { [key]: { action: `decline` } },
      requestState: first.result!.requestState,
    })
    expect(declined.result!.isError).toBe(true)
    expect(declined.result!.content[0].text).toBe(elicitation.results.declined)
    expect(await db.select().from(labels).where(eq(labels.id, label!.id))).toHaveLength(1)

    const again = await call(w.token, `exponential_labels_write`, args)
    const [key2] = Object.keys(again.result!.inputRequests)
    const accepted = await call(w.token, `exponential_labels_write`, args, {
      inputResponses: { [key2!]: { action: `accept`, content: { confirm: true } } },
      requestState: again.result!.requestState,
    })
    expect(accepted.result!.isError).toBeFalsy()
    expect(await db.select().from(labels).where(eq(labels.id, label!.id))).toHaveLength(0)

    // The spent requestState is refused (single use).
    const replay = await call(w.token, `exponential_labels_write`, args, {
      inputResponses: { [key2!]: { action: `accept`, content: { confirm: true } } },
      requestState: again.result!.requestState,
    })
    expect(replay.error?.code).toBe(-32602)

    // A host without form elicitation gets today's behaviour: no ask.
    const [other] = await db.insert(labels).values({ teamId: w.teamId, name: `quiet-${id8()}`, color: `#ff0000` }).returning({ id: labels.id })
    const quiet = await call(w.token, `exponential_labels_write`, { op: `delete`, id: other!.id }, {}, {})
    expect(quiet.result!.resultType).not.toBe(`input_required`)
    expect(await db.select().from(labels).where(eq(labels.id, other!.id))).toHaveLength(0)
  })

  it(`a message to a session returns a task: its card is a question, its report completes it`, async () => {
    resetTaskRateWindows()
    const w = await world()
    const sessionId = await session(w.userId, w.teamId)
    const created = await call(w.token, `exponential_sessions_send`, { sessionId, text: `Ship #EXP-1` })
    expect(created.error).toBeUndefined()
    expect(created.result!.resultType).toBe(`task`)
    const taskId = created.result!.taskId as string
    expect(taskId).toMatch(/^tsk_[A-Za-z0-9_-]{43}$/)
    expect(created.result!.status).toBe(`working`)
    const [watch] = await db.select().from(sessionWatches).where(eq(sessionWatches.taskId, taskId))
    expect(watch).toMatchObject({ watcherKind: `mcp_task`, sessionId: null, subjectId: sessionId })

    // A host without the extension is refused.
    const undeclared = await rpc(w.token, `tasks/get`, { taskId }, {})
    expect(undeclared.error).toBeDefined()

    // The session asks: the task is input_required, keyed by the card's message id.
    const messageId = await db.transaction((tx) =>
      insertMessage(tx, sessionId, { authorKind: `agent`, body: `Which database?`, card: { kind: `question`, question: `Which database?`, options: [`Postgres`, `SQLite`] } as never })
    )
    const asking = await rpc(w.token, `tasks/get`, { taskId })
    expect(asking.result!.status).toBe(`input_required`)
    expect(Object.keys(asking.result!.inputRequests)).toEqual([messageId])

    // tasks/update answers the card (card-answer, first wins).
    const updated = await rpc(w.token, `tasks/update`, { taskId, inputResponses: { [messageId]: { action: `accept`, content: { answer: `SQLite` } } } })
    expect(updated.error).toBeUndefined()
    expect(updated.result).toMatchObject({ resultType: `complete` })
    const [card] = await db.select({ answeredAt: messages.answeredAt }).from(messages).where(eq(messages.id, messageId))
    expect(card!.answeredAt).not.toBeNull()
    expect((await rpc(w.token, `tasks/get`, { taskId })).result!.status).toBe(`working`)

    // The end report completes it with the summary.
    await endSession({ userId: w.userId, sessionId, summary: `Opened PR #12` })
    const done = await rpc(w.token, `tasks/get`, { taskId })
    expect(done.result).toMatchObject({ status: `completed`, result: { content: [{ type: `text`, text: `Opened PR #12` }], isError: false } })
    expect(await db.select().from(sessionWatches).where(eq(sessionWatches.taskId, taskId))).toHaveLength(0)

    // Another principal never finds it.
    const stranger = await world()
    const hidden = await rpc(stranger.token, `tasks/get`, { taskId })
    expect(hidden.error).toMatchObject({ code: -32602, message: tasksFixture.errors.notFound })
  })

  it(`cancel stops the session; a person's End fails a live task`, async () => {
    resetTaskRateWindows()
    const w = await world()
    const sessionId = await session(w.userId, w.teamId)
    const created = await call(w.token, `exponential_sessions_send`, { sessionId, text: `Go` })
    const taskId = created.result!.taskId as string
    await db.update(sessions).set({ status: `running`, busy: true }).where(eq(sessions.id, sessionId))
    const cancelled = await rpc(w.token, `tasks/cancel`, { taskId })
    expect(cancelled.result).toMatchObject({ resultType: `complete` })
    const [task] = await db.select().from(mcpTasks).where(eq(mcpTasks.id, taskId))
    expect(task!.status).toBe(`cancelled`)
    const [stopped] = await db.select({ stoppedAt: sessions.stoppedAt, status: sessions.status }).from(sessions).where(eq(sessions.id, sessionId))
    expect(stopped!.stoppedAt).not.toBeNull()
    expect(stopped!.status).not.toBe(`ended`)

    const second = await call(w.token, `exponential_sessions_send`, { sessionId, text: `Again` })
    const secondId = second.result!.taskId as string
    await endSession({ userId: w.userId, sessionId, failure: `killed` })
    const failed = await rpc(w.token, `tasks/get`, { taskId: secondId })
    expect(failed.result).toMatchObject({ status: `failed`, error: { code: tasksFixture.failureCode, message: tasksFixture.failures.killed } })
    await new Promise((resolve) => setTimeout(resolve, 200))
    const rows = await db
      .select({ tool: mcpCalls.tool })
      .from(mcpCalls)
      .where(and(eq(mcpCalls.userId, w.userId), eq(mcpCalls.method, `tasks/cancel`)))
    // A task id is never stored: the row names the method.
    expect(rows).toEqual([{ tool: `tasks/cancel` }])
  })
})

describe.skipIf(!ready.ok)(`/api/mcp: a start (real db, #NEW-147)`, () => {
  it(`a session start returns a task; a plan card is a question`, async () => {
    resetTaskRateWindows()
    const w = await world()
    const deviceId = `dev-${id8()}`
    await db.insert(devices).values({ userId: w.userId, deviceId, label: `mac`, kind: `desktop` })
    const started = await call(w.token, `exponential_sessions_send`, { text: `Fix the login bug`, deviceId, teamId: w.teamId })
    expect(started.error).toBeUndefined()
    if (started.result!.isError) throw new Error(JSON.stringify(started.result))
    expect(started.result!.resultType).toBe(`task`)
    const taskId = started.result!.taskId as string
    const [task] = await db.select().from(mcpTasks).where(eq(mcpTasks.id, taskId))
    expect(task).toMatchObject({ kind: `session`, teamId: w.teamId })
    const planId = await db.transaction((tx) =>
      insertMessage(tx, task!.ref!, {
        authorKind: `agent`,
        body: `Plan`,
        card: { kind: `plan`, plan: `1. Read\n2. Fix`, questions: [{ id: `db`, prompt: `Which db?`, options: [{ label: `Postgres` }, { label: `SQLite` }] }] } as never,
      })
    )
    const asking = await rpc(w.token, `tasks/get`, { taskId })
    expect(asking.result!.status).toBe(`input_required`)
    const request = asking.result!.inputRequests[planId] as { method: string; params: { mode: string; requestedSchema: { properties: Record<string, unknown> } } }
    expect(request.method).toBe(`elicitation/create`)
    expect(Object.keys(request.params.requestedSchema.properties)).toEqual(expect.arrayContaining([`decision`, `q_db`]))
  })

  it(`a start naming no team asks which one when the grant reaches several`, async () => {
    resetTaskRateWindows()
    const w = await world()
    const [second] = await db.insert(teams).values({ name: `Another`, slug: `ext-${id8()}` }).returning({ id: teams.id })
    made.teams.push(second!.id)
    await db.insert(teamMembers).values({ teamId: second!.id, userId: w.userId, role: `owner` })
    await db.update(mcpGrants).set({ teamIds: [w.teamId, second!.id] }).where(eq(mcpGrants.clientId, w.clientId))
    invalidateMembershipCaches()
    const deviceId = `dev-${id8()}`
    await db.insert(devices).values({ userId: w.userId, deviceId, label: `mac`, kind: `desktop` })
    const args = { text: `Fix it`, deviceId }
    const first = await call(w.token, `exponential_sessions_send`, args)
    expect(first.result!.resultType).toBe(`input_required`)
    const [key, request] = Object.entries(first.result!.inputRequests as Record<string, { params: { requestedSchema: any } }>)[0]!
    const offered = request.params.requestedSchema.properties.teamId.oneOf as { const: string; title: string }[]
    expect(offered.map((option) => option.title)).toEqual([`Another`, `Ext`])
    // A value that was not offered is refused.
    const bad = await call(w.token, `exponential_sessions_send`, args, {
      inputResponses: { [key]: { action: `accept`, content: { teamId: crypto.randomUUID() } } },
      requestState: first.result!.requestState,
    })
    expect(bad.error?.code).toBe(-32602)
    const again = await call(w.token, `exponential_sessions_send`, args)
    const [key2] = Object.keys(again.result!.inputRequests)
    const picked = await call(w.token, `exponential_sessions_send`, args, {
      inputResponses: { [key2!]: { action: `accept`, content: { teamId: second!.id } } },
      requestState: again.result!.requestState,
    })
    expect(picked.result!.resultType).toBe(`task`)
    const [task] = await db.select({ teamId: mcpTasks.teamId }).from(mcpTasks).where(eq(mcpTasks.id, picked.result!.taskId))
    expect(task!.teamId).toBe(second!.id)
  })
})
