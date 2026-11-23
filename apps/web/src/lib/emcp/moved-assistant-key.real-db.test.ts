// @vitest-environment node
// #EXP-2079: A KEPT ASSISTANT CONVERSATION THAT STAYS LIVE ACROSS THE MOVE
// (#EXP-1984). Before the move the Assistant lived in team A and its run
// launched on the device's agent key of A (stored scope [A]). The move puts
// the conversation into its member's personal team; the run keeps that key
// until its next relaunch. Against the REAL migrated Postgres, through the
// noun tools, in exactly that state (the staging hand check's case 11):
//   * WITHOUT the gate's exception (the key's own scope): a call in the old
//     team passes, the other team is refused, `sessions get` without an id
//     passes, with its own id it is refused, a child cannot be started in
//     ANY team and a card cannot be shown;
//   * WITH it (`accessForActingSession`): all of them pass, in both teams;
//     a team outside the member's teams stays refused;
//   * the exception is exactly that state: the same key speaking for a team
//     agent's session reaches A only; a key of A minted AFTER the personal
//     team existed is never widened; another member's Assistant root is not
//     the caller's.
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js"
import { db } from "@/db/connection"
import { apikeys } from "@/db/auth-schema"
import { agents, boards, devices, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

process.env.BETTER_AUTH_SECRET ||= `moved-assistant-key-test-secret-0123456789abcdef`

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
  relayPostInterrupt: async () => ({ delivered: false }),
}))

const ready = await probeRealDb(
  `SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'teams' AND column_name = 'kind') AS ok`
)
if (!ready.ok) console.info(`[moved-assistant-key.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { accessForActingSession } = await import(`@/lib/emcp/gate`)
const { confineMcpAccessToActingSession, movedAssistantKeyScope } = await import(`@/lib/sessions/acting`)
const { resolveMcpAccessForGrant } = await import(`@/lib/emcp/scope`)
const { registerNounTools } = await import(`@/lib/emcp/nouns`)
const { MCP_SESSION_HEADER } = await import(`@/lib/emcp/session-header`)
const { invalidateMembershipCaches } = await import(`@/lib/auth/membership-cache`)
type McpAccess = Awaited<ReturnType<typeof resolveMcpAccessForGrant>>
type McpUser = Parameters<typeof registerNounTools>[1]

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }
const DAY = 24 * 60 * 60 * 1000

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

async function user(): Promise<McpUser> {
  const id = `moved-key-${id8()}`
  const email = `${id}@moved.test`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${email}, true, false, now(), now())
  `)
  made.users.push(id)
  return { id, email, name: `Tester`, image: null, emailVerified: true, isAdmin: false } as unknown as McpUser
}

async function team(owner: string, kind: `team` | `personal` = `team`) {
  const [row] = await db
    .insert(teams)
    .values({ name: kind === `personal` ? `Personal` : `Team`, slug: `moved-${id8()}`, kind, ownerId: kind === `personal` ? owner : null })
    .returning({ id: teams.id })
  made.teams.push(row!.id)
  if (kind === `team`) await db.insert(teamMembers).values({ teamId: row!.id, userId: owner, role: `owner` })
  const [board] =
    kind === `team`
      ? await db.insert(boards).values({ teamId: row!.id, name: `Board`, slug: `b-${id8()}`, prefix: `M${id8().slice(0, 3).toUpperCase()}` }).returning({ id: boards.id })
      : [undefined]
  return { id: row!.id, boardId: board?.id ?? `` }
}

/** An agent key of `teamId` as the launcher mints it, created `createdAt`. */
async function teamKey(userId: string, teamId: string, createdAt: Date) {
  const id = `key-${id8()}${id8()}`
  const scope = { allTeams: false as const, teamIds: [teamId], boardIds: [] }
  await db.insert(apikeys).values({
    id,
    configId: `default`,
    name: `Device: test (agent)`,
    referenceId: userId,
    key: `hash-${id}`,
    createdAt,
    updatedAt: createdAt,
    metadata: JSON.stringify({ kind: `agent`, scope }),
  } as never)
  return { id, kind: `agent` as const, scope }
}

/** After the move: `me` in teams A and B, their Assistant and its kept
 *  conversation in the personal team (created NOW), the key of A minted a
 *  day BEFORE it. */
async function world() {
  const me = await user()
  const a = await team(me.id)
  const b = await team(me.id)
  const other = await user()
  const foreign = await team(other.id)
  const personal = await team(me.id, `personal`)
  invalidateMembershipCaches()
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: me.id, deviceId, label: `mac`, kind: `desktop` })
  const [assistant] = await db
    .insert(agents)
    .values({ teamId: personal.id, name: `Assistant`, instructions: ``, triggers: [], deviceId, ownerId: me.id, role: `assistant` })
    .returning()
  const [teamAgent] = await db
    .insert(agents)
    .values({ teamId: a.id, name: `Sloppy`, instructions: ``, triggers: [], deviceId, ownerId: me.id, role: `agent` })
    .returning()
  const conv = (await sendSession({ userId: me.id, input: { agentId: assistant!.id, text: `Hi.` } })).sessionId
  const [root] = await db.select().from(sessions).where(eq(sessions.id, conv))
  expect(root).toMatchObject({ teamId: personal.id, agentId: assistant!.id, userId: me.id })
  const oldKey = await teamKey(me.id, a.id, new Date(Date.now() - DAY))
  return { me, other, a, b, foreign, personal, deviceId, assistant: assistant!, teamAgent: teamAgent!, conv, oldKey }
}

const requestFor = (sessionId: string) =>
  new Request(`https://x.test/api/mcp`, { method: `POST`, headers: { [MCP_SESSION_HEADER]: sessionId } })

/** The gate's access for `key` speaking for `sessionId` (authenticateMcpRequest's steps). */
async function gated(userId: string, key: Awaited<ReturnType<typeof teamKey>>, sessionId: string, request = requestFor(sessionId)) {
  const access = await resolveMcpAccessForGrant(key.scope, `agent`)
  return { request, access: await accessForActingSession({ access, key }, request, userId, sessionId) }
}

/** The key's own scope, as the gate resolved it before #EXP-2079. */
async function ungated(userId: string, key: Awaited<ReturnType<typeof teamKey>>, sessionId: string) {
  const request = requestFor(sessionId)
  const access = await resolveMcpAccessForGrant(key.scope, `agent`)
  return { request, access: await confineMcpAccessToActingSession(access, request, userId, sessionId) }
}

function tools(u: McpUser, request: Request, access: McpAccess, sessionId: string) {
  const handlers = new Map<string, (args: Record<string, unknown>, extra?: unknown) => Promise<CallToolResult>>()
  const server = { registerTool: (name: string, _def: unknown, handler: never) => handlers.set(name, handler) }
  registerNounTools(server as never, u, request, access, sessionId)
  return async (name: string, args: Record<string, unknown>) => {
    const result = await handlers.get(name)!(args, {})
    const text = (result.content[0] as { text?: string } | undefined)?.text ?? ``
    return { ok: !result.isError, text }
  }
}

const CARD = {
  title: `Status`,
  surface: {
    root: {
      id: `s`,
      component: `Stack`,
      children: [{ id: `r`, component: `Row`, props: { title: `Tests` }, slots: { trailing: { id: `g`, component: `Badge`, props: { text: `Green` } } } }],
    },
  },
}
const NOT_GRANTED = /not granted access/

describe.skipIf(!ready.ok)(`#EXP-2079 a moved Assistant conversation still live on its old team key`, () => {
  it(`the key's own scope (before): the old team only, and nothing that names the conversation's own team`, async () => {
    const w = await world()
    const { request, access } = await ungated(w.me.id, w.oldKey, w.conv)
    expect([...access.visibleTeamIds]).toEqual([w.a.id])
    const call = tools(w.me, request, access, w.conv)
    // (a) the old team passes, (b) the other team is refused.
    expect((await call(`exponential_issues`, { op: `list`, boardId: w.a.boardId })).ok).toBe(true)
    expect((await call(`exponential_issues`, { op: `list`, boardId: w.b.boardId })).text).toMatch(NOT_GRANTED)
    // (c) itself: without an id it passes (the rig's call), with its id it is refused.
    expect((await call(`exponential_sessions`, { op: `get` })).ok).toBe(true)
    expect((await call(`exponential_sessions`, { op: `get`, id: w.conv })).text).toMatch(NOT_GRANTED)
    // (d) no child in ANY team, the old one included; no card.
    for (const teamId of [w.a.id, w.b.id]) {
      const child = await call(`exponential_sessions_send`, { parentId: w.conv, teamId, text: `Do it.` })
      expect(child.text, teamId).toMatch(NOT_GRANTED)
    }
    expect((await call(`exponential_show`, CARD)).text).toMatch(NOT_GRANTED)
  }, 60_000)

  it(`through the gate (after): the Assistant's scope, clamped to its tree: both teams, itself, children, cards`, async () => {
    const w = await world()
    expect(await movedAssistantKeyScope(w.oldKey, requestFor(w.conv), w.me.id, w.conv)).toMatchObject({ assistant: true })
    const { request, access } = await gated(w.me.id, w.oldKey, w.conv)
    expect(access.full).toBe(false)
    expect([...access.visibleTeamIds].sort()).toEqual([w.a.id, w.b.id, w.personal.id].sort())
    const call = tools(w.me, request, access, w.conv)
    // (a) + (b): a call in each team of the member.
    for (const t of [w.a, w.b]) {
      const listed = await call(`exponential_issues`, { op: `list`, boardId: t.boardId })
      expect(listed.ok, listed.text).toBe(true)
    }
    // A team that is not the member's stays out.
    expect((await call(`exponential_issues`, { op: `list`, boardId: w.foreign.boardId })).ok).toBe(false)
    // (c) itself, both call shapes.
    for (const args of [{ op: `get` }, { op: `get`, id: w.conv }]) {
      const got = await call(`exponential_sessions`, args)
      expect(got.ok, got.text).toBe(true)
      expect(got.text).toContain(w.conv)
    }
    // (d) a child in each team.
    for (const t of [w.a, w.b]) {
      const started = await call(`exponential_sessions_send`, { parentId: w.conv, teamId: t.id, text: `Do it.` })
      expect(started.ok, started.text).toBe(true)
      const [child] = await db
        .select()
        .from(sessions)
        .where(sql`${sessions.parentId} = ${w.conv} AND ${sessions.teamId} = ${t.id}`)
      expect(child).toMatchObject({ rootId: w.conv, teamId: t.id })
    }
    expect((await call(`exponential_sessions_send`, { parentId: w.conv, teamId: w.foreign.id, text: `No.` })).ok).toBe(false)
    // A card in its own conversation.
    const shown = await call(`exponential_show`, CARD)
    expect(shown.text).not.toMatch(NOT_GRANTED)
    expect(shown.ok, shown.text).toBe(true)
    // A Teams pick still narrows it: B only.
    await sendSession({ userId: w.me.id, input: { agentId: w.assistant.id, text: `B only.`, scopeTeamIds: [w.b.id] } })
    const picked = await gated(w.me.id, w.oldKey, w.conv)
    expect([...picked.access.visibleTeamIds].sort()).toEqual([w.b.id, w.personal.id].sort())
  }, 120_000)

  it(`(e) a child of the moved conversation: its own team only, on the same key`, async () => {
    const w = await world()
    const child = await sendSession({
      userId: w.me.id,
      callerSessionId: w.conv,
      viaMcp: true,
      input: { parentId: w.conv, teamId: w.a.id, text: `Fix it in A.` },
    })
    expect(await movedAssistantKeyScope(w.oldKey, requestFor(child.sessionId), w.me.id, child.sessionId)).toBeNull()
  }, 60_000)

  it(`the same key speaking for a team agent's session: unchanged, its team only`, async () => {
    const w = await world()
    const agentConv = (await sendSession({ userId: w.me.id, input: { agentId: w.teamAgent.id, text: `Hi.` } })).sessionId
    expect(await movedAssistantKeyScope(w.oldKey, requestFor(agentConv), w.me.id, agentConv)).toBeNull()
    const { access } = await gated(w.me.id, w.oldKey, agentConv)
    expect([...access.visibleTeamIds]).toEqual([w.a.id])
    // No session at all: the key's own scope.
    const bare = new Request(`https://x.test/api/mcp`, { method: `POST` })
    expect(await movedAssistantKeyScope(w.oldKey, bare, w.me.id, null)).toBeNull()
  }, 60_000)

  it(`a key of the team minted AFTER the personal team existed is never widened`, async () => {
    const w = await world()
    const fresh = await teamKey(w.me.id, w.a.id, new Date(Date.now() + DAY))
    expect(await movedAssistantKeyScope(fresh, requestFor(w.conv), w.me.id, w.conv)).toBeNull()
    const { request, access } = await gated(w.me.id, fresh, w.conv)
    expect([...access.visibleTeamIds]).toEqual([w.a.id])
    const call = tools(w.me, request, access, w.conv)
    expect((await call(`exponential_show`, CARD)).text).toMatch(NOT_GRANTED)
    expect((await call(`exponential_sessions_send`, { parentId: w.conv, teamId: w.a.id, text: `Do it.` })).text).toMatch(NOT_GRANTED)
  }, 60_000)

  it(`not the caller's own Assistant root, a team they left, another kind of key: never widened`, async () => {
    const w = await world()
    // Another member's Assistant conversation: not the caller's session at all.
    const theirHome = await team(w.other.id, `personal`)
    invalidateMembershipCaches()
    const theirDevice = `dev-${id8()}`
    await db.insert(devices).values({ userId: w.other.id, deviceId: theirDevice, label: `box`, kind: `desktop` })
    const [theirs] = await db
      .insert(agents)
      .values({ teamId: theirHome.id, name: `Assistant`, instructions: ``, triggers: [], deviceId: theirDevice, ownerId: w.other.id, role: `assistant` })
      .returning()
    const theirConv = (await sendSession({ userId: w.other.id, input: { agentId: theirs!.id, text: `Hi.` } })).sessionId
    expect(await movedAssistantKeyScope(w.oldKey, requestFor(theirConv), w.me.id, theirConv)).toBeNull()
    const foreignRoot = await gated(w.me.id, w.oldKey, theirConv)
    expect([...foreignRoot.access.visibleTeamIds]).toEqual([w.a.id])
    // A personal key, a board-scoped key, the assistant key itself: their own scope.
    const personalKind = { ...w.oldKey, kind: `personal` as const }
    expect(await movedAssistantKeyScope(personalKind, requestFor(w.conv), w.me.id, w.conv)).toBeNull()
    const boardScoped = { ...w.oldKey, scope: { ...w.oldKey.scope, boardIds: [w.a.boardId] } }
    expect(await movedAssistantKeyScope(boardScoped, requestFor(w.conv), w.me.id, w.conv)).toBeNull()
    // The key of a team the member has left.
    await db.delete(teamMembers).where(sql`${teamMembers.teamId} = ${w.a.id} AND ${teamMembers.userId} = ${w.me.id}`)
    invalidateMembershipCaches()
    expect(await movedAssistantKeyScope(w.oldKey, requestFor(w.conv), w.me.id, w.conv)).toBeNull()
  }, 60_000)
})
