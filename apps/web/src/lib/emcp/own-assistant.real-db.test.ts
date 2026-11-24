// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, devices, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import type { McpUser } from "./server"

// #EXP-2107 (emcp access.json `ownAssistant`): after the Assistant move its
// conversation sits in the member's personal team, outside every chat's tree
// teams. The chats it started still message, read and watch it (they report
// back to it); nothing else crosses into the personal team. Against the REAL
// migrated Postgres, through the MCP tools as a session's request sees them.

process.env.BETTER_AUTH_SECRET ||= `own-assistant-test-secret-0123456789abcdef`

vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
  notifySessionAsk: async () => {},
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  getSteerRelayConfig: () => ({ url: `http://relay.test`, secret: `s` }),
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: true }),
}))
vi.mock(`@/lib/conversion/events`, () => ({ recordConversionEvent: async () => {} }))

const ready = await probeRealDb(
  `SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'sessions' AND column_name = 'scope_team_ids') AS ok`
)
if (!ready.ok) console.info(`[own-assistant.real-db] skipped: ${ready.reason}`)

const { sendSession, TEAM_OUT_OF_SCOPE_MESSAGE } = await import(`@/lib/sessions/send`)
const { confineMcpAccessToActingSession } = await import(`@/lib/sessions/acting`)
const { FULL_ACCESS } = await import(`./scope`)
const { registerNounTools } = await import(`./nouns`)
const { watchSessions } = await import(`@/lib/sessions/bus`)
const { invalidateMembershipCaches } = await import(`@/lib/auth/membership-cache`)

type ToolResult = { isError?: boolean; content: Array<{ type: string; text?: string }> }

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

async function user(): Promise<McpUser> {
  const id = `own-as-${id8()}`
  const email = `${id}@own.test`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${email}, true, false, now(), now())
  `)
  made.users.push(id)
  return { id, email, name: `Tester`, image: null, emailVerified: true, isAdmin: false } as unknown as McpUser
}

async function team(owner: string, kind: `team` | `personal` = `team`): Promise<string> {
  const [row] = await db
    .insert(teams)
    .values({ name: kind === `personal` ? `Personal` : `Team`, slug: `own-${id8()}`, kind, ownerId: kind === `personal` ? owner : null })
    .returning({ id: teams.id })
  made.teams.push(row!.id)
  if (kind === `team`) await db.insert(teamMembers).values({ teamId: row!.id, userId: owner, role: `owner` })
  return row!.id
}

async function world() {
  const me = await user()
  const mate = await user()
  const a = await team(me.id)
  await db.insert(teamMembers).values({ teamId: a, userId: mate.id, role: `member` })
  const personal = await team(me.id, `personal`)
  const matePersonal = await team(mate.id, `personal`)
  invalidateMembershipCaches()
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: me.id, deviceId, label: `mac`, kind: `desktop`, sharedTeamIds: [a] })
  const mateDevice = `dev-${id8()}`
  await db.insert(devices).values({ userId: mate.id, deviceId: mateDevice, label: `box`, kind: `desktop` })
  const [assistant] = await db
    .insert(agents)
    .values({ teamId: personal, name: `Assistant`, instructions: ``, triggers: [], deviceId, ownerId: me.id, role: `assistant` })
    .returning()
  const [mateAssistant] = await db
    .insert(agents)
    .values({ teamId: matePersonal, name: `Assistant`, instructions: ``, triggers: [], deviceId: mateDevice, ownerId: mate.id, role: `assistant` })
    .returning()
  const [teamAgent] = await db
    .insert(agents)
    .values({ teamId: a, name: `Sloppy`, instructions: ``, triggers: [], deviceId, ownerId: me.id, role: `agent` })
    .returning()
  const conv = (await sendSession({ userId: me.id, input: { agentId: assistant!.id, text: `Hi.` } })).sessionId
  const mateConv = (await sendSession({ userId: mate.id, input: { agentId: mateAssistant!.id, text: `Hi.` } })).sessionId
  // A chat the Assistant started in team A (the worker that reports back).
  const chat = (
    await sendSession({ userId: me.id, callerSessionId: conv, viaMcp: true, input: { teamId: a, text: `Work in A.` } })
  ).sessionId
  return { me, mate, a, personal, deviceId, conv, mateConv, chat, teamAgent: teamAgent! }
}

/** The tools as `userId`'s request speaking for `callerId` sees them. */
async function tools(u: McpUser, callerId: string) {
  const request = new Request(`https://x.test/api/mcp`, { headers: { "x-exp-session-id": callerId } })
  const access = await confineMcpAccessToActingSession(FULL_ACCESS, request, u.id, callerId)
  const handlers = new Map<string, (args: Record<string, unknown>, extra: unknown) => Promise<ToolResult>>()
  const server = {
    registerTool: (name: string, _def: unknown, handler: (args: Record<string, unknown>, extra: unknown) => Promise<ToolResult>) => {
      handlers.set(name, handler)
    },
  }
  registerNounTools(server as never, u, request, access, callerId)
  return async (name: string, args: Record<string, unknown>): Promise<ToolResult> => {
    try {
      return await handlers.get(`exponential_${name}`)!(args, {})
    } catch (e) {
      return { isError: true, content: [{ type: `text`, text: e instanceof Error ? e.message : String(e) }] }
    }
  }
}

const text = (result: ToolResult) => result.content.map((part) => part.text ?? ``).join(``)

describe.skipIf(!ready.ok)(`#EXP-2107 a chat reaches its member's own Assistant`, () => {
  it(`a chat the Assistant started messages, reads and watches its conversation in the personal team`, async () => {
    const w = await world()
    const call = await tools(w.me, w.chat)
    const sent = await call(`sessions_send`, { sessionId: w.conv, text: `Done in A.` })
    expect(sent.isError, text(sent)).toBeFalsy()
    const read = await call(`sessions`, { op: `get`, id: w.conv, messages: 1 })
    expect(read.isError, text(read)).toBeFalsy()
    expect(JSON.parse(text(read)).root.id).toBe(w.conv)
    const watched = await call(`sessions_send`, { wakeOn: [w.conv] })
    expect(watched.isError, text(watched)).toBeFalsy()
    expect(await db.transaction((tx) => watchSessions(tx, w.chat, [w.conv]))).toEqual([w.conv])
    // The core alone (tRPC, the bus) takes it too.
    const core = await sendSession({ userId: w.me.id, callerSessionId: w.chat, viaMcp: true, input: { sessionId: w.conv, text: `Again.` } })
    expect(core.sessionId).toBe(w.conv)
  })

  it(`nothing else crosses into the personal team`, async () => {
    const w = await world()
    const call = await tools(w.me, w.chat)
    const child = await call(`sessions_send`, { parentId: w.conv, text: `A child there.` })
    expect(child.isError).toBe(true)
    const root = await call(`sessions_send`, { teamId: w.personal, deviceId: w.deviceId, text: `A chat there.` })
    expect(root.isError).toBe(true)
    await expect(
      sendSession({ userId: w.me.id, callerSessionId: w.chat, viaMcp: true, input: { parentId: w.conv, text: `Child.` } })
    ).rejects.toThrow(TEAM_OUT_OF_SCOPE_MESSAGE)
  })

  it(`a team agent's tree still cannot reach the Assistant`, async () => {
    const w = await world()
    const agentConv = (await sendSession({ userId: w.me.id, input: { agentId: w.teamAgent.id, text: `Hi.` } })).sessionId
    const call = await tools(w.me, agentConv)
    const sent = await call(`sessions_send`, { sessionId: w.conv, text: `From the agent.` })
    expect(sent.isError).toBe(true)
    expect(await db.transaction((tx) => watchSessions(tx, agentConv, [w.conv]))).toEqual([])
  })

  it(`a teammate's chat never reaches my Assistant, nor does my chat reach theirs`, async () => {
    const w = await world()
    const mateChat = (
      await sendSession({ userId: w.mate.id, input: { teamId: w.a, deviceId: w.deviceId, text: `Mate's work.` } })
    ).sessionId
    const mateCall = await tools(w.mate, mateChat)
    const toMine = await mateCall(`sessions_send`, { sessionId: w.conv, text: `Peek.` })
    expect(toMine.isError).toBe(true)
    expect((await mateCall(`sessions`, { op: `get`, id: w.conv })).isError).toBe(true)
    const call = await tools(w.me, w.chat)
    expect((await call(`sessions_send`, { sessionId: w.mateConv, text: `Peek.` })).isError).toBe(true)
    // The host of the shared device (me) speaking for the teammate's chat
    // on it reaches nothing of my personal team through that chat.
    const hostCall = await tools(w.me, mateChat)
    expect((await hostCall(`sessions_send`, { sessionId: w.conv, text: `Via the host.` })).isError).toBe(true)
  })
})
