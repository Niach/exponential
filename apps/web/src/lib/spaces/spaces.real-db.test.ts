// @vitest-environment node
//
// #EXP-2161 (domain-contract member-space.json): member spaces against the
// REAL migrated Postgres, through the real routers, the real api keys and the
// real credential chokepoint.
import { afterAll, beforeEach, describe, expect, it, vi } from "vitest"
import { and, eq, inArray, isNull, like, sql } from "drizzle-orm"
import { randomBytes } from "node:crypto"
import { MEMBER_SPACE, MEMBER_SPACE_COPY } from "@exp/domain-contract/member-space"
import { installX25519Of, reconcileTrust, emptyTrustStore, signApproval, signRegistration, type TrustStore } from "@exp/domain-contract/install-trust"
import { endpointIdOf } from "@exp/domain-contract/wrap-signature"
import { db } from "@/db/connection"
import { apikeys } from "@/db/auth-schema"
import { deviceCommands, devices, installs, messages, notifications, sessionKeys, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

process.env.BETTER_AUTH_SECRET ||= `member-spaces-test-secret-0123456789abcdef`

const nudges: string[] = []
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  getSteerRelayConfig: () => ({ url: `http://relay.test`, secret: `s` }),
  relayPostNudge: async (_config: unknown, userId: string, deviceId: string) => {
    nudges.push(`${userId}:${deviceId}`)
    return { delivered: false }
  },
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: false }),
}))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'clamp_device_assistant_teams') AS ok
`)
if (!ready.ok) console.info(`[spaces.real-db] skipped: ${ready.reason}`)

const { appRouter } = await import(`@/routes/api/trpc/$`)
const { resolveMcpCredential, resolveSession, resolveSessionOrSpace, invalidateSessionCache } = await import(`@/lib/auth/resolve-bearer`)
const { loadSpaceGrant, spaceRootIds } = await import(`@/lib/spaces/grant`)
const { endInvalidSpaces, endSpacesInTx } = await import(`@/lib/spaces/end`)
const { spaceMayReadShape, spaceShapeWhere } = await import(`@/lib/spaces/shapes`)
const { runInSpace } = await import(`@/lib/spaces/context`)
const { authenticateMcpRequest } = await import(`@/lib/emcp/gate`)
const { treeReaders } = await import(`@/lib/sessions/tree-keys`)
const { treeTeams } = await import(`@/lib/sessions/tree-teams`)
const { getUserTeamIds } = await import(`@/lib/auth/membership`)
const { invalidateMembershipCaches } = await import(`@/lib/auth/membership-cache`)
const { conversationInTx } = await import(`@/lib/agents/conversation`)
const { createTreeKeyInTx } = await import(`@/lib/sessions/tree-keys`)
const { findAssistant } = await import(`@/lib/agents/assistant`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `space-t-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, ${`User ${id}`}, ${`${id}@space.test`}, true, false, now(), now())`)
  made.users.push(id)
  return id
}

async function team(owner: string, ...members: string[]): Promise<string> {
  const [row] = await db.insert(teams).values({ name: `Space`, slug: `space-${id8()}` }).returning({ id: teams.id })
  made.teams.push(row!.id)
  for (const [at, userId] of [owner, ...members].entries()) {
    await db.insert(teamMembers).values({ teamId: row!.id, userId, role: at === 0 ? `owner` : `member` })
  }
  return row!.id
}

const doctor = (state: string) => ({ checkedAt: new Date().toISOString(), items: [{ key: MEMBER_SPACE.doctorKey, group: `setup`, state, detail: `No user namespaces` }] })

async function device(userId: string, sharedTeamIds: string[], over: Record<string, unknown> = {}) {
  const deviceId = `srv-${id8()}`
  const [row] = await db
    .insert(devices)
    .values({ userId, deviceId, label: `unraid`, kind: `server`, sharedTeamIds, doctor: doctor(`ok`) as never, ...over })
    .returning()
  return row!
}

const as = (userId: string, extra: Record<string, unknown> = {}) =>
  appRouter.createCaller({ db, request: new Request(`http://x.test`), session: { user: { id: userId, name: userId }, session: { id: `web` } }, ...extra } as never)
const bearer = (key: string, headers: Record<string, string> = {}) => new Request(`http://x.test`, { headers: { authorization: `Bearer ${key}`, ...headers } })

/** What the tRPC route builds for a key: the caller, or null (401). */
async function viaKey(key: string, headers: Record<string, string> = {}) {
  const request = bearer(key, headers)
  const { session, keyKind, keySpaceId, keyScope } = await resolveMcpCredential(request)
  if (keyScope || !keySpaceId || keyKind !== `space` || !session?.user) return null
  const space = await loadSpaceGrant(db, session.user.id, keySpaceId)
  return space ? appRouter.createCaller({ db, request, session, space } as never) : null
}

const code = (promise: Promise<unknown>) => promise.then(() => `ok`, (error: { code?: string; message?: string }) => error.code ?? error.message)

type Install = { userId: string; installId: string; identity: Uint8Array; endpointId: string; x25519Public: string; store: TrustStore }
async function registerInstall(caller: ReturnType<typeof as>, userId: string, kind: `web` | `daemon` | `space`, deviceId?: string): Promise<Install> {
  const identity = new Uint8Array(randomBytes(32))
  const signedAt = new Date().toISOString()
  const endpointId = endpointIdOf(identity)
  const x25519Public = installX25519Of(identity)
  const { installId } = await caller.installs.register({
    kind,
    ...(deviceId ? { deviceId } : {}),
    label: `mine`,
    endpointId,
    x25519Public,
    signedAt,
    signature: signRegistration(identity, userId, signedAt, kind === `space` ? `space` : `install`),
  })
  const store = emptyTrustStore(endpointId, userId)
  if (kind === `space`) store.space = true
  return { userId, installId, identity, endpointId, x25519Public, store }
}
const subjectOf = (who: Install) => ({ userId: who.userId, endpointId: who.endpointId, x25519Public: who.x25519Public })
const claim = (who: Install) =>
  as(who.userId).installs.claimRoot({ installId: who.installId, signature: signApproval(who.identity, `root`, subjectOf(who)) })
const approveSpace = (by: Install, space: Install) =>
  as(by.userId).installs.approve({ installId: by.installId, subjectInstallId: space.installId, kind: `space`, signature: signApproval(by.identity, `space`, subjectOf(space)) })

/** A team (its owner, the host's owner, the member), a host opted in, the
 *  member's Assistant on their own laptop with a conversation, and the space. */
async function scene(opts: { request?: boolean } = {}) {
  const [teamOwner, hostOwner, member] = [await user(), await user(), await user()]
  const teamId = await team(teamOwner, hostOwner, member)
  const other = await team(teamOwner, hostOwner, member)
  const laptop = await device(member, [], { deviceId: `lap-${id8()}`, kind: `desktop`, label: `laptop` })
  const host = await device(hostOwner, [teamId, other])
  await as(hostOwner).devices.setAssistantTeam({ deviceId: host.deviceId, teamId, allowed: true })
  const { agent } = await as(member).agents.ensureAssistant()
  const old = await db.transaction(async (tx) => (await conversationInTx(tx, agent as never, member)).session)
  const phone = await registerInstall(as(member), member, `web`)
  await claim(phone)
  let spaceId = ``
  let deviceId = ``
  if (opts.request !== false) ({ spaceId, deviceId } = await as(member).memberSpaces.request({ deviceId: host.deviceId, teamId }))
  return { teamOwner, hostOwner, member, teamId, other, laptop, host, old, phone, agentId: agent!.id, spaceId, deviceId }
}

async function running(opts: { approve?: boolean; key?: boolean } = {}) {
  const s = await scene()
  const { key } = await as(s.hostOwner).memberSpaces.provision({ spaceId: s.spaceId })
  const caller = (await viaKey(key))!
  const install = await registerInstall(caller as never, s.member, `space`, s.deviceId)
  if (opts.approve !== false) await approveSpace(s.phone, install)
  const assistant = (await findAssistant(db, s.member))!
  const root = await db.transaction(async (tx) => (await conversationInTx(tx, assistant, s.member)).session)
  if (opts.key !== false) await keyTree(s.member, s.phone, root.id, opts.approve === false ? [s.phone] : [s.phone, install])
  return { ...s, key, caller, install, root }
}

const wrapped = () => randomBytes(81).toString(`base64url`)
const keyTree = (userId: string, maker: Install, rootId: string, targets: Install[], epoch = 1) =>
  db.transaction((tx) =>
    createTreeKeyInTx(tx, userId, { rootId, installId: maker.installId, epoch, publicKey: maker.x25519Public, wraps: targets.map((t) => ({ installId: t.installId, wrapped: wrapped() })) })
  )
const row = async (id: string) => (await db.select().from(devices).where(eq(devices.id, id)))[0]
const session = async (id: string) => (await db.select().from(sessions).where(eq(sessions.id, id)))[0]!
const notices = async (userId: string) =>
  (await db.select({ title: notifications.title }).from(notifications).where(and(eq(notifications.userId, userId), eq(notifications.type, `agent_message`)))).map((n) => n.title)

beforeEach(() => {
  nudges.length = 0
  invalidateMembershipCaches()
  invalidateSessionCache()
})

afterAll(async () => {
  if (!ready.ok) return
  if (made.users.length) {
    const ids = sql.raw(`('${made.users.join(`','`)}')`)
    await db.execute(sql`DELETE FROM agents WHERE owner_id IN ${ids}`)
    await db.execute(sql`DELETE FROM teams WHERE owner_id IN ${ids}`)
    if (made.teams.length) await db.delete(teams).where(inArray(teams.id, made.teams))
    await db.execute(sql`DELETE FROM users WHERE id IN ${ids}`)
  }
})

describe.skipIf(!ready.ok)(`#EXP-2161 member spaces`, () => {
  it(`optInCases through the procedures`, async () => {
    for (const c of MEMBER_SPACE.optInCases) {
      const [owner, host, member] = [await user(), await user(), await user()]
      const teamId = await team(owner, host, member)
      const d = await device(host, c.shared ? [teamId] : [], { kind: c.kind, doctor: c.doctor ? doctor(c.doctor) : null })
      const who = c.caller === `deviceOwner` ? host : c.caller === `teamOwner` ? owner : member
      const got =
        c.caller === `deviceOwner`
          ? await code(as(who).devices.setAssistantTeam({ deviceId: d.deviceId, teamId, allowed: c.allowed }))
          : c.allowed
            ? // Nobody else has an on: the row is not theirs.
              await code(as(who).devices.setAssistantTeam({ deviceId: d.deviceId, teamId, allowed: true }))
            : await code(as(who).devices.dropAssistantTeam({ id: d.id, teamId }))
      expect(got === `ok`, `${c.name}: ${got}`).toBe(c.expected)
      expect((await row(d.id))!.assistantTeamIds.includes(teamId), c.name).toBe(c.expected && c.allowed)
    }
  })

  it(`the opt-in is always inside the share, whichever path writes it`, async () => {
    const s = await scene({ request: false })
    await db.update(devices).set({ sharedTeamIds: [s.other] }).where(eq(devices.id, s.host.id))
    expect((await row(s.host.id))!.assistantTeamIds).toEqual([])
    await db.update(devices).set({ assistantTeamIds: [s.teamId, s.other] }).where(eq(devices.id, s.host.id))
    expect((await row(s.host.id))!.assistantTeamIds).toEqual([s.other])
    await db.update(devices).set({ kind: `desktop` }).where(eq(devices.id, s.host.id))
    expect((await row(s.host.id))!.assistantTeamIds).toEqual([])
    const gone = await team(s.teamOwner, s.hostOwner)
    const d = await device(s.hostOwner, [gone], { assistantTeamIds: [gone] })
    await db.delete(teams).where(eq(teams.id, gone))
    expect(await row(d.id)).toMatchObject({ sharedTeamIds: [], assistantTeamIds: [] })
  })

  it(`picking: the row, the detached conversation, the fresh one, idempotent`, async () => {
    const s = await scene()
    expect(await row(s.spaceId)).toMatchObject({ userId: s.member, deviceId: s.deviceId, kind: `server`, label: `unraid`, spaceHostId: s.host.id, spaceTeamId: s.teamId, sharedTeamIds: [], isDefault: false })
    expect(s.deviceId.startsWith(MEMBER_SPACE.deviceIdPrefix)).toBe(true)
    expect(await session(s.old.id)).toMatchObject({ status: `ended`, agentId: null, deviceId: s.laptop.deviceId })
    const agent = (await findAssistant(db, s.member))!
    expect(agent).toMatchObject({ deviceId: s.deviceId, account: null })
    expect(nudges).toContain(`${s.hostOwner}:${s.host.deviceId}`)
    expect(await as(s.member).memberSpaces.request({ deviceId: s.host.deviceId, teamId: s.teamId })).toMatchObject({ spaceId: s.spaceId })
    const fresh = await db.transaction(async (tx) => (await conversationInTx(tx, agent, s.member)).session)
    expect(fresh.id).not.toBe(s.old.id)
    expect(fresh).toMatchObject({ deviceId: s.deviceId, agentId: s.agentId, shared: false })
    // Never a device: no list, no default, no share, no command.
    expect((await as(s.hostOwner).memberSpaces.hosted({ deviceId: s.host.deviceId })).spaces).toEqual([
      { spaceId: s.spaceId, deviceId: s.deviceId, memberId: s.member, memberName: `User ${s.member}`, label: `unraid` },
    ])
    expect(await code(as(s.member).devices.setDefault({ deviceId: s.deviceId, isDefault: true }))).toBe(`NOT_FOUND`)
    expect(await code(as(s.member).devices.setShared({ deviceId: s.deviceId, teamId: s.teamId, shared: true }))).toBe(`NOT_FOUND`)
    expect(await code(as(s.member).devices.createCommand({ deviceId: s.deviceId, kind: `agent_update`, agent: `claude` }))).toBe(`NOT_FOUND`)
    expect(await code(as(s.member).devices.register({ deviceId: s.deviceId, label: `x`, kind: `server`, doctor: doctor(`ok`) as never }))).toBe(`FORBIDDEN`)
    expect(await code(as(s.teamOwner).devices.register({ deviceId: s.deviceId, label: `x`, kind: `server`, doctor: doctor(`ok`) as never }))).toBe(`FORBIDDEN`)
  })

  it(`picking is refused: not opted in, the own device, a stranger, agents.update`, async () => {
    const s = await scene({ request: false })
    expect(await code(as(s.member).memberSpaces.request({ deviceId: s.host.deviceId, teamId: s.other }))).toBe(`FORBIDDEN`)
    expect(await code(as(s.hostOwner).memberSpaces.request({ deviceId: s.host.deviceId, teamId: s.teamId }))).toBe(`BAD_REQUEST`)
    expect(await code(as(await user()).memberSpaces.request({ deviceId: s.host.deviceId, teamId: s.teamId }))).toBe(`FORBIDDEN`)
    expect(await code(as(s.member).agents.update({ id: s.agentId, deviceId: s.host.deviceId }))).not.toBe(`ok`)
    expect(await code(as(s.member).agents.update({ id: s.agentId, deviceId: `space-${crypto.randomUUID()}` }))).toBe(`BAD_REQUEST`)
  })

  it(`startCases: only the Assistant's conversation on its space, and its children`, async () => {
    const r = await running({ key: false })
    const send = (input: Record<string, unknown>) => code(as(r.member).sessions.send({ text: `go`, ...input } as never))
    // A child of that conversation.
    expect(await send({ parentId: r.root.id, teamId: r.teamId, deviceId: r.deviceId })).toBe(`ok`)
    // A plain chat, a team agent, an app's slot.
    expect(await send({ teamId: r.teamId, deviceId: r.deviceId })).toBe(`NOT_FOUND`)
    expect(await code(as(r.teamOwner).agents.create({ teamId: r.teamId, name: `Bot`, instructions: ``, triggers: [], deviceId: r.deviceId } as never))).toBe(`NOT_FOUND`)
    // A chat elsewhere: no move there, no child there.
    const chat = await as(r.member).sessions.send({ text: `hi`, teamId: r.teamId, deviceId: r.laptop.deviceId } as never)
    expect(await send({ sessionId: chat.sessionId, deviceId: r.deviceId })).not.toBe(`ok`)
    expect(await send({ parentId: chat.sessionId, deviceId: r.deviceId })).toBe(`NOT_FOUND`)
    // A child of the conversation in a team the space does not work in.
    expect(await send({ parentId: r.root.id, teamId: r.other, deviceId: r.deviceId })).toBe(`FORBIDDEN`)
  })

  it(`the credential: exactly its procedures and shapes, 401 once the space ends`, async () => {
    const r = await running()
    const { key } = r
    const request = bearer(key)
    expect(await resolveSession(request)).toBeNull()
    expect((await resolveSessionOrSpace(request)).space?.spaceId).toBe(r.spaceId)
    // Every procedure of the router outside the list is FORBIDDEN.
    const record = (appRouter as unknown as { _def: { procedures: Record<string, unknown> } })._def.procedures
    const paths = Object.keys(record)
    expect(MEMBER_SPACE.procedures.filter((path) => !paths.includes(path))).toEqual([])
    for (const path of paths) {
      if ((MEMBER_SPACE.procedures as string[]).includes(path)) continue
      const call = path.split(`.`).reduce<unknown>((at, name) => (at as Record<string, unknown>)[name], r.caller) as (input?: unknown) => Promise<unknown>
      expect(await code(call({})), path).toBe(`FORBIDDEN`)
    }
    for (const table of MEMBER_SPACE.shapes) expect(spaceMayReadShape(table)).toBe(true)
    for (const table of [`devices`, `agents`, `notifications`, `comments`, `composer_drafts`, `skills`, `events`]) expect(spaceMayReadShape(table)).toBe(false)
    // /api/mcp never.
    expect(`response` in (await authenticateMcpRequest(bearer(key, { "x-exp-session-id": r.root.id })))).toBe(true)
    // The earlier key goes with a fresh one.
    const again = await as(r.hostOwner).memberSpaces.provision({ spaceId: r.spaceId })
    expect(await viaKey(key)).toBeNull()
    expect(await viaKey(again.key)).not.toBeNull()
    expect(again).toMatchObject({ userId: r.member, deviceId: r.deviceId })
    for (const who of [r.member, r.teamOwner]) expect(await code(as(who).memberSpaces.provision({ spaceId: r.spaceId }))).toBe(`NOT_FOUND`)
    expect((await as(r.member).users.listPersonalApiKeys()).keys).toEqual([])
    await as(r.hostOwner).memberSpaces.remove({ spaceId: r.spaceId })
    expect(await viaKey(again.key)).toBeNull()
    expect((await resolveSessionOrSpace(bearer(again.key))).session).toBeNull()
  })

  it(`the credential reads its own trees only: sessions, shapes, devices, installs, teams`, async () => {
    const r = await running({ key: false })
    // Other sessions: the member's own chat, their old conversation, a team-mate's shared chat, a child of the tree elsewhere.
    const chat = (await as(r.member).sessions.send({ text: `private`, teamId: r.teamId, deviceId: r.laptop.deviceId } as never)).sessionId
    const mate = (await as(r.hostOwner).sessions.send({ text: `shared`, teamId: r.teamId, deviceId: r.host.deviceId } as never)).sessionId
    const child = (await as(r.member).sessions.send({ text: `x`, parentId: r.root.id, teamId: r.teamId, deviceId: r.laptop.deviceId } as never)).sessionId
    await keyTree(r.member, r.phone, r.root.id, [r.phone, r.install])
    await keyTree(r.member, r.phone, chat, [r.phone])
    for (const id of [chat, mate, r.old.id, child]) {
      expect(await code(r.caller.sessions.brief({ sessionId: id })), id).toBe(`NOT_FOUND`)
      expect(await code(r.caller.sessions.report({ sessionId: id, status: `idle` })), id).toBe(`NOT_FOUND`)
      expect(await code(r.caller.users.mintPersonalApiKey({ purpose: `agent`, sessionId: id })), id).toBe(`NOT_FOUND`)
    }
    expect(await code(r.caller.sessions.brief({ sessionId: r.root.id }))).toBe(`ok`)
    expect(await code(r.caller.sessionKeys.requestKey({ rootId: chat }))).toBe(`NOT_FOUND`)
    expect(await r.caller.sessionKeys.publicKeys({ rootIds: [r.root.id, r.old.id, chat, mate] })).toHaveLength(1)
    expect(await r.caller.sessions.heldGone({ sessionIds: [r.root.id, chat] } as never).catch(() => null)).not.toEqual(expect.objectContaining({ gone: [chat] }))
    // Other machines.
    expect(await code(r.caller.devices.heartbeat({ deviceId: r.laptop.deviceId, activeSessions: 0, defaultsSyncedAt: null }))).toBe(`FORBIDDEN`)
    expect(await code(r.caller.devices.register({ deviceId: r.laptop.deviceId, label: `x`, kind: `server`, doctor: doctor(`ok`) as never }))).toBe(`FORBIDDEN`)
    expect(await r.caller.devices.register({ deviceId: r.deviceId, label: `evil`, kind: `desktop`, version: `1.2.3`, doctor: doctor(`ok`) as never })).toMatchObject({ ok: true })
    expect(await row(r.spaceId)).toMatchObject({ label: `unraid`, kind: `server`, version: `1.2.3` })
    const beat = await r.caller.devices.heartbeat({ deviceId: r.deviceId, activeSessions: 1, defaultsSyncedAt: null, agentAccounts: { claude: { signedIn: true, email: `host@x.test` } } as never })
    expect(beat).toMatchObject({ ok: true })
    expect(beat).not.toHaveProperty(`voice`)
    const [command] = await db.insert(deviceCommands).values({ deviceRowId: r.laptop.id, userId: r.member, kind: `agent_update`, payload: {} }).returning()
    expect(await code(r.caller.devices.completeCommand({ commandId: command!.id, ok: true }))).toBe(`FORBIDDEN`)
    // Teams and installs.
    const space = (await loadSpaceGrant(db, r.member, r.spaceId))!
    expect(await runInSpace(space, () => getUserTeamIds(r.member))).toEqual(expect.arrayContaining(space.reach))
    expect((await runInSpace(space, () => getUserTeamIds(r.member))).length).toBe(2)
    expect(await code(r.caller.teams.getAgentPrompt({ teamId: r.other }))).toBe(`FORBIDDEN`)
    expect(await code(r.caller.teams.getAgentPrompt({ teamId: r.teamId }))).toBe(`ok`)
    const listed = await r.caller.installs.list({ teamId: r.teamId })
    expect(listed.every((entry) => entry.userId === r.member)).toBe(true)
    expect((await r.caller.installs.trust({ installId: r.install.installId })).installs.every((entry) => entry.userId === r.member)).toBe(true)
    expect(await code(r.caller.installs.trust({ installId: r.phone.installId }))).toBe(`FORBIDDEN`)
    expect(await code(r.caller.sessionKeys.mine({ installId: r.phone.installId }))).toBe(`FORBIDDEN`)
    expect(await code(as(r.member).sessionKeys.mine({ installId: r.install.installId }))).toBe(`FORBIDDEN`)
    // Credentials resolve for a session on the space, by header.
    expect(await code(r.caller.mcpServers.resolveForLaunch({}))).toBe(`FORBIDDEN`)
    expect(await code((await viaKey(r.key, { "x-exp-session-id": chat }))!.mcpServers.resolveForLaunch({}))).toBe(`NOT_FOUND`)
    expect(await code((await viaKey(r.key, { "x-exp-session-id": r.root.id }))!.skills.resolveForLaunch({}))).toBe(`ok`)
    // Shapes.
    expect(await spaceRootIds(db, space)).toEqual([r.root.id])
    const where = await spaceShapeWhere(space, `messages`, `x = 1`)
    expect(where).toContain(r.root.id)
    expect(where).not.toContain(chat)
    expect(await spaceShapeWhere(space, `session_issues`, null)).not.toContain(chat)
  })

  it(`agent keys minted through it: MCP-only, on the space, its teams, gone with it`, async () => {
    const r = await running()
    const bound = await r.caller.users.mintPersonalApiKey({ purpose: `agent`, sessionId: r.root.id })
    const plain = await r.caller.users.mintPersonalApiKey({ purpose: `agent` })
    expect(await code(r.caller.users.mintPersonalApiKey({ purpose: `personal` }))).toBe(`FORBIDDEN`)
    expect(await code(r.caller.users.mintPersonalApiKey({ purpose: `agent`, teamId: r.other }))).toBe(`FORBIDDEN`)
    expect(await resolveSession(bearer(plain.key))).toBeNull()
    expect((await resolveSessionOrSpace(bearer(plain.key))).session).toBeNull()
    expect(await viaKey(plain.key)).toBeNull()
    const status = async (key: string, headers: Record<string, string> = {}) => {
      const gated = await authenticateMcpRequest(bearer(key, headers))
      return `response` in gated ? gated.response.status : [...gated.caller.access.visibleTeamIds].sort()
    }
    const grant = (await loadSpaceGrant(db, r.member, r.spaceId))!
    expect(await status(bound.key)).toEqual([...grant.reach].sort())
    expect(await status(plain.key)).toBe(403)
    const chat = (await as(r.member).sessions.send({ text: `p`, teamId: r.teamId, deviceId: r.laptop.deviceId } as never)).sessionId
    expect(await status(plain.key, { "x-exp-session-id": chat })).toBe(403)
    expect(await status(plain.key, { "x-exp-session-id": r.root.id })).toEqual([...grant.reach].sort())
    // Inside the space, the sessions lib reaches no other session.
    const { loadSessionForUser } = await import(`@/lib/sessions/access`)
    expect(await code(runInSpace(grant, () => loadSessionForUser(r.member, chat)))).toBe(`NOT_FOUND`)
    expect(await code(runInSpace(grant, () => loadSessionForUser(r.member, r.old.id)))).toBe(`NOT_FOUND`)
    expect(await code(runInSpace(grant, () => loadSessionForUser(r.member, r.root.id)))).toBe(`ok`)
    // Settings, devices, approvals: never.
    expect(await code(runInSpace(grant, () => as(r.member).agents.update({ id: r.agentId, deviceId: r.laptop.deviceId })))).toBe(`FORBIDDEN`)
    expect(await code(runInSpace(grant, () => as(r.member).devices.createCommand({ deviceId: r.laptop.deviceId, kind: `agent_update`, agent: `claude` })))).toBe(`FORBIDDEN`)
    expect(await code(runInSpace(grant, () => approveSpace(r.phone, r.install)))).toBe(`FORBIDDEN`)
    expect(await code(runInSpace(grant, () => as(r.member).memberSpaces.mine()))).toBe(`FORBIDDEN`)
    await as(r.hostOwner).memberSpaces.remove({ spaceId: r.spaceId })
    expect(await status(bound.key)).toBe(401)
    expect(await db.select().from(apikeys).where(and(eq(apikeys.referenceId, r.member), like(apikeys.metadata, `%${r.spaceId}%`)))).toEqual([])
  })

  it(`a tree's teams narrow on the next request`, async () => {
    const r = await running()
    const personal = r.root.teamId
    expect(new Set(await treeTeams(r.root.id))).toEqual(new Set([personal, r.teamId]))
    await as(r.hostOwner).devices.setAssistantTeam({ deviceId: r.host.deviceId, teamId: r.other, allowed: true })
    expect(new Set(await treeTeams(r.root.id))).toEqual(new Set([personal, r.teamId, r.other]))
    await as(r.teamOwner).devices.dropAssistantTeam({ id: r.host.id, teamId: r.other })
    expect(new Set(await treeTeams(r.root.id))).toEqual(new Set([personal, r.teamId]))
    // Another team opted out: the space stays.
    expect(await row(r.spaceId)).toBeTruthy()
  })

  it(`a space makes the first key of a chat its Assistant starts, for the chat's readers alone, and reads none of it`, async () => {
    const r = await running()
    const inSpace = async <T>(run: () => Promise<T>) => runInSpace((await loadSpaceGrant(db, r.member, r.spaceId))!, run)
    const chat = async (over: Record<string, unknown> = {}) => {
      const { sessionId } = await as(r.member).sessions.send({ text: `c`, teamId: r.teamId, deviceId: r.laptop.deviceId } as never)
      await db.update(sessions).set({ startedByAgentId: r.agentId, ...over }).where(eq(sessions.id, sessionId))
      return sessionId
    }
    const first = (id: string, targets: Install[], epoch = 1) => inSpace(() => code(keyTree(r.member, r.install, id, targets, epoch)))
    const id = await chat()
    // Its own wrap is refused: it is no reader. Nor does anything else go through.
    expect(await first(id, [r.install, r.phone])).toBe(`BAD_REQUEST`)
    expect(await first(id, [])).toBe(`BAD_REQUEST`)
    expect(await first(id, [r.phone])).toBe(`ok`)
    expect((await session(id)).sealEpoch).toBe(1)
    expect([...(await treeReaders(db, await session(id))).keys()]).not.toContain(r.install.installId)
    expect(await first(id, [r.phone], 2)).toBe(`NOT_FOUND`)
    // Not a chat it started, not one from before the pick, not outside a space's key.
    expect(await first(await chat({ startedByAgentId: null }), [r.phone])).toBe(`NOT_FOUND`)
    expect(await first(await chat({ createdAt: new Date(0) }), [r.phone])).toBe(`NOT_FOUND`)
    expect(await code(keyTree(r.member, r.install, await chat(), [r.phone]))).toBe(`FORBIDDEN`)
    // Until its member approves it: nothing.
    const w = await running({ approve: false, key: false })
    const { sessionId } = await as(w.member).sessions.send({ text: `c`, teamId: w.teamId, deviceId: w.laptop.deviceId } as never)
    await db.update(sessions).set({ startedByAgentId: w.agentId }).where(eq(sessions.id, sessionId))
    const grant = (await loadSpaceGrant(db, w.member, w.spaceId))!
    expect(await runInSpace(grant, () => code(keyTree(w.member, w.install, sessionId, [w.phone])))).not.toBe(`ok`)
  })

  it(`an unapproved space install is refused at every key path; approved, it reads its tree and no other`, async () => {
    const r = await running({ approve: false })
    const readers = async (id: string) => [...(await treeReaders(db, await session(id))).keys()]
    expect(await readers(r.root.id)).not.toContain(r.install.installId)
    expect(await code(r.caller.sessionKeys.create({ rootId: r.root.id, installId: r.install.installId, epoch: 2, publicKey: r.install.x25519Public, wraps: [{ installId: r.install.installId, wrapped: wrapped() }] }))).toBe(`FORBIDDEN`)
    expect(await as(r.member).sessionKeys.pending({ installId: r.phone.installId })).toEqual([])
    expect(await code(as(r.member).sessionKeys.wrap({ installId: r.phone.installId, wraps: [{ rootId: r.root.id, epoch: 1, targetInstallId: r.install.installId, wrapped: wrapped() }] }))).toBe(`FORBIDDEN`)
    expect(await code(db.transaction((tx) => createTreeKeyInTx(tx, r.member, { rootId: r.root.id, installId: r.phone.installId, epoch: 2, publicKey: r.phone.x25519Public, wraps: [r.phone, r.install].map((t) => ({ installId: t.installId, wrapped: wrapped() })) })))).not.toBe(`ok`)
    expect(await r.caller.sessionKeys.due({ installId: r.install.installId })).toEqual([])
    expect((await as(r.member).memberSpaces.mine()).space).toMatchObject({ status: `waiting`, endpointId: r.install.endpointId })
    expect((await as(r.member).installs.waiting()).find((w) => w.installId === r.install.installId)).toMatchObject({ kind: `newInstall`, kindOfInstall: `space`, label: `unraid` })
    // Never `approve`, never by itself, never another account's, never a root.
    expect(await code(as(r.member).installs.approve({ installId: r.phone.installId, subjectInstallId: r.install.installId, kind: `approve`, signature: signApproval(r.phone.identity, `approve`, subjectOf(r.install)) }))).toBe(`BAD_REQUEST`)
    expect(await code(as(r.member).installs.approve({ installId: r.install.installId, subjectInstallId: r.phone.installId, kind: `approve`, signature: signApproval(r.install.identity, `approve`, subjectOf(r.phone)) }))).toBe(`FORBIDDEN`)
    expect(await code(as(r.member).installs.claimRoot({ installId: r.install.installId, signature: signApproval(r.install.identity, `root`, subjectOf(r.install)) }))).toBe(`FORBIDDEN`)
    const foreign = await registerInstall(as(r.hostOwner), r.hostOwner, `web`)
    await claim(foreign)
    expect(await code(approveSpace({ ...foreign, userId: r.hostOwner }, r.install))).toBe(`NOT_FOUND`)
    // An install on no chain says nothing.
    const second = await registerInstall(as(r.member), r.member, `web`)
    expect(await code(approveSpace(second, r.install))).toBe(`FORBIDDEN`)

    await approveSpace(r.phone, r.install)
    expect(await readers(r.root.id)).toContain(r.install.installId)
    expect((await as(r.member).memberSpaces.mine()).space).toMatchObject({ status: `ready` })
    expect((await as(r.member).sessionKeys.pending({ installId: r.phone.installId })).map((p) => p.targetInstallId)).toContain(r.install.installId)
    await as(r.member).sessionKeys.wrap({ installId: r.phone.installId, wraps: [{ rootId: r.root.id, epoch: 1, targetInstallId: r.install.installId, wrapped: wrapped() }] })
    expect(await r.caller.sessionKeys.mine({ installId: r.install.installId })).toHaveLength(1)
    // No other tree, the old conversation and the member's chats included.
    const chat = (await as(r.member).sessions.send({ text: `c`, teamId: r.teamId, deviceId: r.laptop.deviceId } as never)).sessionId
    await keyTree(r.member, r.phone, chat, [r.phone])
    await keyTree(r.member, r.phone, r.old.id, [r.phone])
    for (const id of [chat, r.old.id]) {
      expect(await readers(id)).not.toContain(r.install.installId)
      expect(await code(as(r.member).sessionKeys.wrap({ installId: r.phone.installId, wraps: [{ rootId: id, epoch: 1, targetInstallId: r.install.installId, wrapped: wrapped() }] }))).toBe(`FORBIDDEN`)
    }
    expect((await as(r.member).sessionKeys.pending({ installId: r.phone.installId })).filter((p) => p.targetInstallId === r.install.installId)).toEqual([])
  })

  it(`what clients are served: an older one no space, a newer one pins it as a space`, async () => {
    const r = await running()
    const older = await as(r.member).installs.trust({ installId: r.phone.installId })
    expect(older.installs.map((i) => i.installId)).not.toContain(r.install.installId)
    expect(older.approvals.map((a) => a.kind as string)).not.toContain(`space`)
    const mate = await registerInstall(as(r.teamOwner), r.teamOwner, `web`)
    expect((await as(r.teamOwner).installs.trust({ installId: mate.installId, spaces: true })).installs.map((i) => i.installId)).not.toContain(r.install.installId)
    expect((await as(r.teamOwner).installs.list({ teamId: r.teamId })).map((i) => i.installId)).not.toContain(r.install.installId)
    const view = reconcileTrust(r.phone.store, await as(r.member).installs.trust({ installId: r.phone.installId, spaces: true }))
    expect(view.store.pins[r.install.endpointId]).toMatchObject({ class: `space`, by: r.phone.endpointId })
    expect(view.rows).toEqual([])
    const own = reconcileTrust(r.install.store, await r.caller.installs.trust({ installId: r.install.installId }))
    expect(own).toMatchObject({ status: `trusted`, rows: [] })
    expect(own.store.pins[r.phone.endpointId]).toMatchObject({ via: `chain` })
  })

  it(`installs.register: space only through its key, one per space, never turned ordinary`, async () => {
    const r = await running()
    expect(await code(registerInstall(as(r.member), r.member, `space`, r.deviceId))).toBe(`FORBIDDEN`)
    expect(await code(registerInstall(r.caller as never, r.member, `web`))).toBe(`FORBIDDEN`)
    const signedAt = new Date().toISOString()
    const ordinary = { endpointId: r.install.endpointId, x25519Public: r.install.x25519Public, signedAt }
    expect(await code(as(r.member).installs.register({ kind: `daemon`, ...ordinary, signature: signRegistration(r.install.identity, r.member, signedAt) }))).toBe(`FORBIDDEN`)
    // Signed with the ordinary label: no space registration.
    expect(await code(r.caller.installs.register({ kind: `space`, deviceId: r.deviceId, ...ordinary, signature: signRegistration(r.install.identity, r.member, signedAt) }))).toBe(`BAD_REQUEST`)
    const next = await registerInstall(r.caller as never, r.member, `space`, r.deviceId)
    const [old] = await db.select().from(installs).where(eq(installs.id, r.install.installId))
    expect(old!.revokedAt).not.toBeNull()
    expect(await db.select().from(sessionKeys).where(eq(sessionKeys.installId, r.install.installId))).toEqual([])
    expect((await session(r.root.id)).sealRotateEpoch).toBe(1)
    expect((await as(r.member).memberSpaces.mine()).space).toMatchObject({ status: `waiting`, endpointId: next.endpointId })
  })

  const causes: Array<[string, (r: Awaited<ReturnType<typeof running>>) => Promise<unknown>]> = [
    [`unshared`, (r) => as(r.hostOwner).devices.setShared({ deviceId: r.host.deviceId, teamId: r.teamId, shared: false })],
    [`unshared`, (r) => as(r.teamOwner).devices.dropShare({ id: r.host.id, teamId: r.teamId })],
    [`unshared`, (r) => as(r.teamOwner).teams.delete({ teamId: r.teamId })],
    [`optedOut`, (r) => as(r.hostOwner).devices.setAssistantTeam({ deviceId: r.host.deviceId, teamId: r.teamId, allowed: false })],
    [`optedOut`, (r) => as(r.teamOwner).devices.dropAssistantTeam({ id: r.host.id, teamId: r.teamId })],
    [`memberLeft`, (r) => leave(r.teamOwner, r.teamId, r.member)],
    [`ownerLeft`, (r) => leave(r.teamOwner, r.teamId, r.hostOwner)],
    [`ownerLeft`, (r) => as(r.hostOwner).users.deleteAccount({ confirm: true })],
    [`deviceRemoved`, (r) => as(r.hostOwner).devices.remove({ deviceId: r.host.deviceId })],
    [`picked`, (r) => as(r.member).agents.update({ id: r.agentId, deviceId: r.laptop.deviceId })],
    [`installRemoved`, (r) => as(r.member).installs.revoke({ installId: r.install.installId })],
    [`removed`, (r) => as(r.hostOwner).memberSpaces.remove({ spaceId: r.spaceId })],
    [`removed`, (r) => as(r.teamOwner).memberSpaces.remove({ spaceId: r.spaceId })],
  ]
  async function leave(owner: string, teamId: string, userId: string) {
    const [membership] = await db.select().from(teamMembers).where(and(eq(teamMembers.teamId, teamId), eq(teamMembers.userId, userId)))
    return as(owner).teamMembers.remove({ memberId: membership!.id } as never)
  }
  for (const [at, [cause, end]] of causes.entries()) {
    it(`the end: ${cause} (${at})`, async () => {
      const r = await running({ key: false })
      const agentKey = await r.caller.users.mintPersonalApiKey({ purpose: `agent`, sessionId: r.root.id })
      const child = (await as(r.member).sessions.send({ text: `x`, parentId: r.root.id, teamId: r.root.teamId, deviceId: r.deviceId } as never)).sessionId
      await keyTree(r.member, r.phone, r.root.id, [r.phone, r.install])
      const before = (await notices(r.member)).length
      await end(r)
      invalidateMembershipCaches()
      expect(await row(r.spaceId)).toBeUndefined()
      expect(await viaKey(r.key)).toBeNull()
      expect(`response` in (await authenticateMcpRequest(bearer(agentKey.key)))).toBe(true)
      expect(await db.select().from(apikeys).where(and(eq(apikeys.referenceId, r.member), like(apikeys.metadata, `%${r.spaceId}%`)))).toEqual([])
      const [install] = await db.select().from(installs).where(eq(installs.id, r.install.installId))
      expect(install!.revokedAt).not.toBeNull()
      expect(await db.select().from(sessionKeys).where(eq(sessionKeys.installId, r.install.installId))).toEqual([])
      const root = await session(r.root.id)
      expect(root.sealRotateEpoch).toBe(1)
      expect((await session(child)).status).toBe(`ended`)
      const agent = (await findAssistant(db, r.member))!
      if (cause === `picked`) {
        expect(agent.deviceId).toBe(r.laptop.deviceId)
        expect(root).toMatchObject({ agentId: r.agentId, deviceId: r.laptop.deviceId, status: `queued` })
        // The conversation's key still reaches the member's own machines only.
        expect([...(await treeReaders(db, root)).keys()]).toEqual([r.phone.installId])
      } else {
        expect(agent.deviceId).toBe(``)
        expect(root).toMatchObject({ agentId: r.agentId, status: `idle`, busy: false })
      }
      const said = (await notices(r.member)).slice(before)
      const silent = MEMBER_SPACE.silentCauses.includes(cause)
      expect(said.filter((title) => title === MEMBER_SPACE_COPY.ended.replace(`{device}`, `unraid`))).toHaveLength(silent ? 0 : 1)
      if (cause !== `deviceRemoved` && at !== 7) expect(nudges).toContain(`${r.hostOwner}:${r.host.deviceId}`)
      // Idempotent.
      expect(await db.transaction((tx) => endSpacesInTx(tx, [r.spaceId], `removed`))).toEqual([])
      if (silent || cause === `unshared`) return
      expect(await as(r.member).memberSpaces.mine()).toEqual({ space: null, ended: true })
      await db.insert(messages).values({ conversationId: sql`(SELECT id FROM conversations WHERE session_id = ${r.root.id})`, teamId: r.root.teamId, rootId: r.root.id, ownerId: r.member, shared: false, authorKind: `user`, authorId: r.member, body: `still here` } as never)
      const [queued] = await db.select().from(messages).where(and(eq(messages.rootId, r.root.id), eq(messages.body, `still here`), isNull(messages.consumedAt)))
      expect(queued).toBeTruthy()
    })
  }

  it(`the end: accountDeleted (the member's) is silent and leaves nothing`, async () => {
    const r = await running()
    await as(r.member).users.deleteAccount({ confirm: true })
    expect(await row(r.spaceId)).toBeUndefined()
    expect(await viaKey(r.key)).toBeNull()
    expect((await as(r.hostOwner).memberSpaces.hosted({ deviceId: r.host.deviceId })).spaces).toEqual([])
  })

  it(`endCases: the space picked through a team ends with it, though another remains`, async () => {
    const r = await running()
    await as(r.hostOwner).devices.setAssistantTeam({ deviceId: r.host.deviceId, teamId: r.other, allowed: true })
    await as(r.hostOwner).devices.setAssistantTeam({ deviceId: r.host.deviceId, teamId: r.teamId, allowed: false })
    expect(await row(r.spaceId)).toBeUndefined()
  })

  it(`picking another device's space ends the first; its conversation's key never reaches the new one`, async () => {
    const r = await running()
    const host2 = await device(r.hostOwner, [r.teamId], { assistantTeamIds: [r.teamId], label: `build` })
    const second = await as(r.member).memberSpaces.request({ deviceId: host2.deviceId, teamId: r.teamId })
    expect(await row(r.spaceId)).toBeUndefined()
    expect(await session(r.root.id)).toMatchObject({ status: `ended`, agentId: null })
    const { key } = await as(r.hostOwner).memberSpaces.provision({ spaceId: second.spaceId })
    const caller = (await viaKey(key))!
    const install = await registerInstall(caller as never, r.member, `space`, second.deviceId)
    await approveSpace(r.phone, install)
    expect([...(await treeReaders(db, await session(r.root.id))).keys()]).toEqual([r.phone.installId])
    expect(await caller.sessionKeys.publicKeys({ rootIds: [r.root.id] })).toEqual([])
    expect(await code(caller.sessions.brief({ sessionId: r.root.id }))).toBe(`NOT_FOUND`)
    expect((await db.select().from(devices).where(and(eq(devices.userId, r.member), sql`space_host_id IS NOT NULL`)))).toHaveLength(1)
  })

  it(`opt-out races request: never a space without its opt-in`, async () => {
    for (let i = 0; i < 6; i++) {
      const s = await scene({ request: false })
      await Promise.all([
        as(s.member).memberSpaces.request({ deviceId: s.host.deviceId, teamId: s.teamId }).catch(() => null),
        as(s.hostOwner).devices.setAssistantTeam({ deviceId: s.host.deviceId, teamId: s.teamId, allowed: false }),
      ])
      expect(await db.select().from(devices).where(and(eq(devices.userId, s.member), sql`space_host_id IS NOT NULL`))).toEqual([])
    }
  }, 60_000)

  it(`a membership that went by another path: its keys stop at once, the watchdog ends it`, async () => {
    const r = await running()
    await db.delete(teamMembers).where(and(eq(teamMembers.teamId, r.teamId), eq(teamMembers.userId, r.hostOwner)))
    expect(await viaKey(r.key)).toBeNull()
    expect([...(await treeReaders(db, await session(r.root.id))).keys()]).not.toContain(r.install.installId)
    expect((await as(r.hostOwner).memberSpaces.hosted({ deviceId: r.host.deviceId })).spaces).toEqual([])
    await endInvalidSpaces()
    expect(await row(r.spaceId)).toBeUndefined()
  })

  it(`forDevice and remove: the device's owner, an owner of its team, nobody else`, async () => {
    const r = await running()
    for (const who of [r.hostOwner, r.teamOwner]) {
      expect((await as(who).memberSpaces.forDevice({ id: r.host.id })).members).toEqual([{ spaceId: r.spaceId, userId: r.member, name: `User ${r.member}`, teamId: r.teamId }])
    }
    expect((await as(r.member).memberSpaces.forDevice({ id: r.host.id })).members).toEqual([])
    expect(await code(as(await user()).memberSpaces.remove({ spaceId: r.spaceId }))).toBe(`NOT_FOUND`)
  })

  it(`a member without any device picks one`, async () => {
    const s = await scene({ request: false })
    const bare = await user()
    await db.insert(teamMembers).values({ teamId: s.teamId, userId: bare, role: `member` })
    const { deviceId } = await as(bare).memberSpaces.request({ deviceId: s.host.deviceId, teamId: s.teamId })
    expect((await findAssistant(db, bare))!.deviceId).toBe(deviceId)
  })
})
