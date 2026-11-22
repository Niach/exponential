// @vitest-environment node
// #EXP-2028 (access.json `actingSession`) against the REAL migrated
// Postgres: the launcher's key of a plain chat's session is bound to it at
// mint. Every /api/mcp request on that key speaks for the session with or
// without `X-Exp-Session-Id`, so a header-less call reaches only the tree's
// teams; a header naming another session is refused; the key is MCP-only,
// never listed, and replaced (or swept) per session.
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { apikeys } from "@/db/auth-schema"
import { boards, devices, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

vi.mock(`@/lib/steer`, async (importOriginal) => {
  const real = await importOriginal<typeof import("@/lib/steer")>()
  return {
    ...real,
    getSteerRelayConfig: () => ({ url: `http://relay.test`, secret: `s` }),
    relayPostNudge: async () => {},
    relayPostKill: async () => {},
    relayPostInterrupt: async () => ({ delivered: true }),
  }
})
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
    WHERE table_name = 'sessions' AND column_name = 'scope_team_ids') AS ok
`)
if (!ready.ok) console.info(`[session-bound-key.real-db] skipped: ${ready.reason}`)

const { Route } = await import(`@/routes/api/mcp`)
const post = (Route.options.server as { handlers: { POST: (ctx: { request: Request }) => Promise<Response> } }).handlers.POST
const { usersRouter } = await import(`@/lib/trpc/users`)
const { sendSession } = await import(`@/lib/sessions/send`)
const { resolveSession, invalidateSessionCache } = await import(`@/lib/auth/resolve-bearer`)
const { BOUND_KEY_MISMATCH_MESSAGE, BOUND_KEY_UNKNOWN_MESSAGE } = await import(`./gate`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `bound-key-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@bound.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function team(owner: string, name: string) {
  const [row] = await db.insert(teams).values({ name, slug: `bound-${id8()}` }).returning({ id: teams.id })
  made.teams.push(row!.id)
  await db.insert(teamMembers).values({ teamId: row!.id, userId: owner, role: `owner` })
  const [board] = await db
    .insert(boards)
    .values({ teamId: row!.id, name: `${name} board`, slug: `b-${id8()}`, prefix: `BK` })
    .returning({ id: boards.id })
  return { teamId: row!.id, boardId: board!.id }
}

async function world() {
  const owner = await user()
  const mate = await user()
  const one = await team(owner, `One`)
  const two = await team(owner, `Two`)
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const chat = async () =>
    (await sendSession({ userId: owner, input: { teamId: one.teamId, deviceId, text: `Chat` } as never })).sessionId
  return { owner, mate, one, two, deviceId, chat }
}

/** A person's own app request (a cookie session, never a key). */
const users = (userId: string) =>
  usersRouter.createCaller({
    db,
    request: new Request(`http://x.test`),
    session: { user: { id: userId }, session: { id: `cookie-${id8()}`, userId } },
  } as never)

async function call(key: string, tool: string, args: Record<string, unknown>, sessionHeader?: string) {
  return post({
    request: new Request(`http://localhost/api/mcp`, {
      method: `POST`,
      headers: {
        authorization: `Bearer ${key}`,
        "content-type": `application/json`,
        accept: `application/json, text/event-stream`,
        ...(sessionHeader ? { "x-exp-session-id": sessionHeader } : {}),
      },
      body: JSON.stringify({ jsonrpc: `2.0`, id: 1, method: `tools/call`, params: { name: tool, arguments: args } }),
    }),
  })
}

/** The board ids a `boards list` call answers (across the visible teams). */
async function boardIds(response: Response): Promise<string[]> {
  expect(response.status).toBe(200)
  const body = (await response.json()) as { result: { content: Array<{ text: string }>; isError?: boolean } }
  expect(body.result.isError).toBeFalsy()
  const parsed = JSON.parse(body.result.content[0]!.text) as unknown
  const rows = (Array.isArray(parsed) ? parsed : (parsed as { items: unknown[] }).items) as Array<{ id: string }>
  return rows.map((row) => row.id)
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`a launcher key bound to its session (#EXP-2028)`, () => {
  it(`a header-less call on a plain chat's key reaches only its tree's teams`, async () => {
    const w = await world()
    const chat = await w.chat()
    const minted = await users(w.owner).mintPersonalApiKey({ purpose: `agent`, sessionId: chat })
    expect(minted.sessionId).toBe(chat)

    // No header: the key speaks for its chat, whose tree is its own team.
    const headerless = await boardIds(await call(minted.key, `exponential_boards`, { op: `list` }))
    expect(headerless).toContain(w.one.boardId)
    expect(headerless).not.toContain(w.two.boardId)
    // Its own header: the same.
    const own = await boardIds(await call(minted.key, `exponential_boards`, { op: `list` }, chat))
    expect(own).toEqual(headerless)

    // An unbound device key on the same member reaches both teams header-less.
    const device = await users(w.owner).mintPersonalApiKey({ purpose: `agent` })
    expect(device.sessionId).toBeNull()
    const full = await boardIds(await call(device.key, `exponential_boards`, { op: `list` }))
    expect(full).toEqual(expect.arrayContaining([w.one.boardId, w.two.boardId]))
  })

  it(`a header naming another session is refused, so is a bound session that is gone`, async () => {
    const w = await world()
    const chat = await w.chat()
    const other = await w.chat()
    const { key } = await users(w.owner).mintPersonalApiKey({ purpose: `agent`, sessionId: chat })
    const mismatch = await call(key, `exponential_boards`, { op: `list` }, other)
    expect(mismatch.status).toBe(403)
    expect(await mismatch.json()).toEqual({ error: BOUND_KEY_MISMATCH_MESSAGE })

    await db.delete(sessions).where(eq(sessions.id, chat))
    const gone = await call(key, `exponential_boards`, { op: `list` })
    expect(gone.status).toBe(403)
    expect(await gone.json()).toEqual({ error: BOUND_KEY_UNKNOWN_MESSAGE })
  })

  it(`is MCP-only, never listed, replaced per session and swept with ended sessions`, async () => {
    const w = await world()
    const chat = await w.chat()
    const ended = await w.chat()
    const first = await users(w.owner).mintPersonalApiKey({ purpose: `agent`, sessionId: chat })
    const endedKey = await users(w.owner).mintPersonalApiKey({ purpose: `agent`, sessionId: ended })
    await db.update(sessions).set({ status: `ended` }).where(eq(sessions.id, ended))

    // The general API surface refuses it (no tRPC, shapes, attachments).
    expect(await resolveSession(new Request(`http://x.test`, { headers: { authorization: `Bearer ${first.key}` } }))).toBeNull()
    // The person's key list never shows a session's key.
    const listed = (await users(w.owner).listPersonalApiKeys()).keys.map((row) => row.id)
    expect(listed).not.toContain(first.id)

    // Minting again for the chat replaces its key; the ended chat's goes too.
    const second = await users(w.owner).mintPersonalApiKey({ purpose: `agent`, sessionId: chat })
    const left = (await db.select({ id: apikeys.id }).from(apikeys).where(eq(apikeys.referenceId, w.owner))).map((row) => row.id)
    expect(left).toContain(second.id)
    expect(left).not.toContain(first.id)
    expect(left).not.toContain(endedKey.id)
    invalidateSessionCache()
    expect((await call(first.key, `exponential_boards`, { op: `list` })).status).toBe(401)
  })

  it(`only the session's principal mints its key, and only as the agent's unscoped key`, async () => {
    const w = await world()
    const chat = await w.chat()
    await expect(users(w.mate).mintPersonalApiKey({ purpose: `agent`, sessionId: chat })).rejects.toThrow(/not yours/)
    await expect(users(w.owner).mintPersonalApiKey({ sessionId: chat })).rejects.toThrow(/purpose 'agent'/)
    await expect(users(w.owner).mintPersonalApiKey({ purpose: `agent`, sessionId: chat, teamId: w.one.teamId })).rejects.toThrow(
      /purpose 'agent'/
    )
  })
})
