// @vitest-environment node
//
// #EXP-1941: the chat of a first call (`voice.callConversation`), the pending
// wraps of ONE root (`sessionKeys.pending {rootIds}`) and `sendTarget`'s
// `rotationDue`, through the routers against the REAL migrated Postgres.
// The sealed ids come from the lib's mint; the Enable case takes its id
// from `startCall` (a claim and the device's `call-seal` cap).
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest"
import { and, eq, inArray, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, devices, installs, messages, sessionKeys, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { generateTreeKey, STALE_TREE_REFUSAL, wrapTreeKey } from "@exp/domain-contract/sealed-content"

process.env.BETTER_AUTH_SECRET ||= `call-conversation-test-secret`

const h = vi.hoisted(() => ({ nudges: [] as string[] }))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  getSteerRelayConfig: () => ({ url: `https://relay.example`, secret: `relay-secret` }),
  relayPostNudge: async (_config: unknown, ownerId: string, deviceId: string) => {
    h.nudges.push(`${ownerId}/${deviceId}`)
    return { delivered: true }
  },
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
  notifySessionAsk: async () => {},
}))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'session_keys') AS ok
`)
if (!ready.ok) console.info(`[voice-call-conversation.real-db] skipped: ${ready.reason}`)

const { voiceRouter } = await import(`@/lib/trpc/voice`)
const { sessionKeysRouter } = await import(`@/lib/trpc/session-keys`)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
const { mintCallId } = await import(`@/lib/voice-call-id`)
const { createTreeKeyInTx } = await import(`@/lib/sessions/tree-keys`)

const NOW = new Date(`2026-11-18T09:00:00.000Z`)
const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }
const ctx = (userId: string) => ({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) }) as never
const voice = (userId: string) => voiceRouter.createCaller(ctx(userId))
const keys = (userId: string) => sessionKeysRouter.createCaller(ctx(userId))
const sessionsOf = (userId: string) => sessionsRouter.createCaller(ctx(userId))

async function user(): Promise<string> {
  const id = `call-conv-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@call.test`}, true, false, ${NOW.toISOString()}, ${NOW.toISOString()})
  `)
  made.users.push(id)
  return id
}

async function team(members: Record<string, `owner` | `member`>): Promise<string> {
  const [row] = await db.insert(teams).values({ name: `Call`, slug: `call-${id8()}` }).returning({ id: teams.id })
  made.teams.push(row!.id)
  await db.insert(teamMembers).values(Object.entries(members).map(([userId, role]) => ({ teamId: row!.id, userId, role })))
  return row!.id
}

async function device(userId: string, sharedTeamIds: string[] = []): Promise<string> {
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId, deviceId, label: deviceId, kind: `desktop`, sharedTeamIds, lastSeenAt: NOW })
  return deviceId
}

async function agent(teamId: string, ownerId: string, deviceId: string, role: `assistant` | `agent` = `agent`): Promise<string> {
  const [row] = await db
    .insert(agents)
    .values({ teamId, name: `Helper`, icon: null, color: null, instructions: ``, deviceId, ownerId, role, createdAt: NOW })
    .returning({ id: agents.id })
  return row!.id
}

/** An install of `userId` and a fresh root seal wrapped for it (and for `others`). */
async function install(userId: string) {
  const key = generateTreeKey()
  const [row] = await db
    .insert(installs)
    .values({ userId, kind: `desktop`, endpointId: `ep-${crypto.randomUUID()}`, x25519Public: key.publicKey, createdAt: NOW })
    .returning({ id: installs.id })
  const id = row!.id
  const seal = (others: { id: string; publicKey: string }[] = []) => {
    const tree = generateTreeKey()
    return {
      installId: id,
      publicKey: tree.publicKey,
      epoch: 1 as const,
      wraps: [{ id, publicKey: key.publicKey }, ...others].map((to) => ({ installId: to.id, wrapped: wrapTreeKey(to.publicKey, tree.secret) })),
    }
  }
  return { id, publicKey: key.publicKey, seal }
}

/** A member with a desktop, an install on it and an agent that runs there. */
async function world() {
  const me = await user()
  const teamId = await team({ [me]: `owner` })
  const deviceId = await device(me)
  const agentId = await agent(teamId, me, deviceId)
  const mine = await install(me)
  const sealedId = (patch: { userId?: string; deviceId?: string; agentId?: string; sealed?: boolean } = {}) =>
    mintCallId({ userId: me, deviceId, agentId, sealed: true, ...patch })!
  return { me, teamId, deviceId, agentId, mine, sealedId }
}

const rootRow = async (rootId: string) => (await db.select().from(sessions).where(eq(sessions.id, rootId)))[0]!
const wrapsOf = async (rootId: string) => (await db.select().from(sessionKeys).where(eq(sessionKeys.rootId, rootId))).length
const refusedCall = { code: `NOT_FOUND`, message: `No such call` }

beforeAll(() => {
  vi.useFakeTimers({ toFake: [`Date`] })
  vi.setSystemTime(NOW)
})

afterAll(async () => {
  vi.useRealTimers()
  if (!ready.ok) return
  if (made.teams.length) await db.delete(teams).where(inArray(teams.id, made.teams))
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`#EXP-1941 voice.callConversation`, () => {
  it(`a first call: a keyed conversation with no message, queued, its device nudged; sendTarget names it`, async () => {
    const w = await world()
    const seal = w.mine.seal()
    h.nudges = []
    const out = await voice(w.me).callConversation({ agentId: w.agentId, callId: w.sealedId(), deviceId: w.deviceId, seal })
    expect(out).toMatchObject({ epoch: 1, publicKey: seal.publicKey })
    const root = await rootRow(out.rootId)
    expect(root).toMatchObject({ agentId: w.agentId, userId: w.me, parentId: null, status: `queued`, sealEpoch: 1, sealPublicKey: seal.publicKey })
    expect(await db.select().from(messages).where(eq(messages.sessionId, out.rootId))).toEqual([])
    expect(await wrapsOf(out.rootId)).toBe(1)
    expect(h.nudges).toEqual([`${w.me}/${w.deviceId}`])
    expect(await sessionsOf(w.me).sendTarget({ agentId: w.agentId })).toEqual({
      rootId: out.rootId,
      epoch: 1,
      publicKey: seal.publicKey,
      brief: null,
      clearTree: false,
      rotationDue: false,
    })
  })

  it(`Enable: startCall seals for a claim and the device's call-seal cap, and only that id opens the chat of a first call`, async () => {
    const w = await world()
    const doctor = { checkedAt: NOW.toISOString(), items: [{ key: `speech`, group: `voice`, state: `ok` }] } as never
    const row = and(eq(devices.userId, w.me), eq(devices.deviceId, w.deviceId))
    const call = (callId: string) => voice(w.me).callConversation({ agentId: w.agentId, callId, deviceId: w.deviceId, seal: w.mine.seal() })
    // The caller is the web app (no client header); the switch names it below.
    process.env.VOICE_CALL_SEAL = ``
    await db.update(devices).set({ doctor, caps: [`voice`, `call-seal`] }).where(row)
    const off = await voice(w.me).startCall({ agentId: w.agentId, platform: `web`, seal: true })
    expect(off.seal).toBe(false)
    await expect(call(off.callId)).rejects.toMatchObject(refusedCall)
    process.env.VOICE_CALL_SEAL = `web`

    // A host without a key holder advertises no cap: a claiming caller gets a clear call.
    await db.update(devices).set({ doctor, caps: [`voice`] }).where(row)
    const clear = await voice(w.me).startCall({ agentId: w.agentId, platform: `web`, seal: true })
    expect(clear.seal).toBe(false)
    await expect(call(clear.callId)).rejects.toMatchObject(refusedCall)
    await expect(voice(w.me).callHostTicket({ callId: clear.callId, deviceId: w.deviceId, agentId: w.agentId })).resolves.toMatchObject({ seal: false })

    await db.update(devices).set({ caps: [`voice`, `call-seal`] }).where(row)
    // An older caller claims nothing: clear, whatever the host can do.
    const old = await voice(w.me).startCall({ agentId: w.agentId, platform: `web` })
    expect(old.seal).toBe(false)
    await expect(call(old.callId)).rejects.toMatchObject(refusedCall)

    const sealed = await voice(w.me).startCall({ agentId: w.agentId, platform: `web`, seal: true })
    expect(sealed.seal).toBe(true)
    await expect(voice(w.me).callHostTicket({ callId: sealed.callId, deviceId: w.deviceId, agentId: w.agentId })).resolves.toMatchObject({ seal: true })
    const out = await call(sealed.callId)
    expect((await rootRow(out.rootId)).sealEpoch).toBe(1)
    delete process.env.VOICE_CALL_SEAL
  })

  it(`a second call answers the stored key and writes none: the host's own seal is not adopted`, async () => {
    const w = await world()
    const first = await voice(w.me).callConversation({ agentId: w.agentId, callId: w.sealedId(), deviceId: w.deviceId, seal: w.mine.seal() })
    h.nudges = []
    const other = w.mine.seal()
    const again = await voice(w.me).callConversation({ agentId: w.agentId, callId: w.sealedId(), deviceId: w.deviceId, seal: other })
    expect(again).toMatchObject({ rootId: first.rootId, epoch: 1, publicKey: first.publicKey })
    expect(again.publicKey).not.toBe(other.publicKey)
    expect(await wrapsOf(first.rootId)).toBe(1)
    expect(h.nudges).toEqual([])
  })

  it(`a conversation that exists without a key (an older chat) is keyed, with no second row`, async () => {
    const w = await world()
    const id = crypto.randomUUID()
    await db.insert(sessions).values({
      id,
      teamId: w.teamId,
      parentId: null,
      rootId: id,
      depth: 0,
      agentId: w.agentId,
      userId: w.me,
      deviceId: w.deviceId,
      status: `idle`,
      title: `Helper`,
    })
    const seal = w.mine.seal()
    const out = await voice(w.me).callConversation({ agentId: w.agentId, callId: w.sealedId(), deviceId: w.deviceId, seal })
    expect(out).toMatchObject({ rootId: id, epoch: 1, publicKey: seal.publicKey })
    expect(await db.select().from(sessions).where(and(eq(sessions.agentId, w.agentId), eq(sessions.userId, w.me)))).toHaveLength(1)
    expect((await rootRow(out.rootId)).status).toBe(`idle`)
  })

  it(`a root keyed by another writer in between answers the stored key, where a plain key write is the stale-tree conflict`, async () => {
    const w = await world()
    const first = await voice(w.me).callConversation({ agentId: w.agentId, callId: w.sealedId(), deviceId: w.deviceId, seal: w.mine.seal() })
    // The conflict a text-less inline seal would meet on this keyed root.
    await expect(
      db.transaction((tx) => createTreeKeyInTx(tx, w.me, { ...w.mine.seal(), rootId: first.rootId }))
    ).rejects.toMatchObject({ code: `CONFLICT`, message: STALE_TREE_REFUSAL })
    await expect(
      voice(w.me).callConversation({ agentId: w.agentId, callId: w.sealedId(), deviceId: w.deviceId, seal: w.mine.seal() })
    ).resolves.toMatchObject({ rootId: first.rootId, publicKey: first.publicKey })
  })

  it(`refuses without a sealed id the server minted for this member, device and agent, and creates nothing`, async () => {
    const w = await world()
    const otherAgent = await agent(w.teamId, w.me, w.deviceId)
    const seal = w.mine.seal()
    const call = (callId: string) => voice(w.me).callConversation({ agentId: w.agentId, callId, deviceId: w.deviceId, seal })
    await expect(call(crypto.randomUUID())).rejects.toMatchObject(refusedCall)
    await expect(call(w.sealedId({ sealed: false }))).rejects.toMatchObject(refusedCall)
    await expect(call(w.sealedId({ agentId: otherAgent }))).rejects.toMatchObject(refusedCall)
    await expect(call(w.sealedId({ deviceId: `dev-other` }))).rejects.toMatchObject(refusedCall)
    await expect(call(w.sealedId({ userId: `someone-else` }))).rejects.toMatchObject(refusedCall)
    // The clear id of a real call, as `startCall` mints it today.
    await db.update(devices).set({ doctor: { checkedAt: NOW.toISOString(), items: [{ key: `speech`, group: `voice`, state: `ok` }] } as never }).where(eq(devices.deviceId, w.deviceId))
    const started = await voice(w.me).startCall({ agentId: w.agentId, platform: `phone` })
    expect(started.seal).toBe(false)
    await expect(call(started.callId)).rejects.toMatchObject(refusedCall)
    // An id older than its window.
    vi.setSystemTime(new Date(NOW.getTime() - 4 * 60_000))
    const old = w.sealedId()
    vi.setSystemTime(NOW)
    await expect(call(old)).rejects.toMatchObject(refusedCall)
    expect(await db.select().from(sessions).where(eq(sessions.agentId, w.agentId))).toEqual([])
  })

  it(`refuses an agent the member may not message: another team's, another member's Assistant, a foreign unshared device`, async () => {
    const w = await world()
    const mate = await user()
    const theirTeam = await team({ [mate]: `owner` })
    const theirDevice = await device(mate)
    const outside = await agent(theirTeam, mate, theirDevice)
    const call = (agentId: string) =>
      voice(w.me).callConversation({ agentId, callId: w.sealedId({ agentId }), deviceId: w.deviceId, seal: w.mine.seal() })
    await expect(call(outside)).rejects.toMatchObject({ code: `NOT_FOUND`, message: `Agent not found` })
    // A team-mate now: their Assistant is theirs alone, and their agent on an unshared device talks to its owner only.
    await db.insert(teamMembers).values({ teamId: theirTeam, userId: w.me, role: `member` })
    const assistant = await agent(theirTeam, mate, theirDevice, `assistant`)
    await expect(call(assistant)).rejects.toMatchObject({ code: `NOT_FOUND`, message: `Agent not found` })
    await expect(call(outside)).rejects.toMatchObject({ code: `NOT_FOUND`, message: `This device is not yours and not shared with the team` })
    expect(await db.select().from(sessions).where(inArray(sessions.agentId, [outside, assistant]))).toEqual([])
  })

  it(`refuses a wrap for an install that is no reader, and an install that is not the caller's: no chat, no key`, async () => {
    const w = await world()
    const mate = await user()
    const theirs = await install(mate)
    const call = (seal: ReturnType<typeof w.mine.seal>) =>
      voice(w.me).callConversation({ agentId: w.agentId, callId: w.sealedId(), deviceId: w.deviceId, seal })
    await expect(call(w.mine.seal([theirs]))).rejects.toMatchObject({ code: `BAD_REQUEST`, message: `A wrap names an install that is not a reader` })
    await expect(call(theirs.seal())).rejects.toMatchObject({ code: `FORBIDDEN`, message: `Not your install` })
    // The refused transaction left nothing behind: the next call is still a first call.
    expect(await db.select().from(sessions).where(eq(sessions.agentId, w.agentId))).toEqual([])
    await expect(call(w.mine.seal())).resolves.toMatchObject({ epoch: 1 })
  })
})

describe.skipIf(!ready.ok)(`#EXP-1941 sessionKeys.pending {rootIds} and sendTarget rotationDue`, () => {
  it(`pending names the pairs of the asked roots only`, async () => {
    const w = await world()
    const other = await agent(w.teamId, w.me, w.deviceId)
    const call = (agentId: string) =>
      voice(w.me).callConversation({ agentId, callId: w.sealedId({ agentId }), deviceId: w.deviceId, seal: w.mine.seal() })
    const [a, b] = [await call(w.agentId), await call(other)]
    // A new install of the member (the phone that calls): a reader of both roots, with no wrap yet.
    const phone = await install(w.me)
    const all = await keys(w.me).pending({ installId: w.mine.id })
    expect(all.map((pair) => pair.rootId).sort()).toEqual([a.rootId, b.rootId].sort())
    const one = await keys(w.me).pending({ installId: w.mine.id, rootIds: [b.rootId] })
    expect(one).toEqual([{ rootId: b.rootId, epoch: 1, publicKey: b.publicKey, targetInstallId: phone.id, targetX25519Public: phone.publicKey }])
    expect(await keys(w.me).pending({ installId: w.mine.id, rootIds: [] })).toEqual([])
    // A root this install holds no wrap of is not answered, whoever asks for it.
    expect(await keys(w.me).pending({ installId: phone.id, rootIds: [a.rootId] })).toEqual([])
  })

  it(`sendTarget says rotationDue while the root's rotation is flagged, and false after the rotation`, async () => {
    const w = await world()
    const out = await voice(w.me).callConversation({ agentId: w.agentId, callId: w.sealedId(), deviceId: w.deviceId, seal: w.mine.seal() })
    const target = () => sessionsOf(w.me).sendTarget({ agentId: w.agentId })
    expect((await target()).rotationDue).toBe(false)
    await db.update(sessions).set({ sealRotateEpoch: 1 }).where(eq(sessions.id, out.rootId))
    expect(await target()).toMatchObject({ rootId: out.rootId, epoch: 1, rotationDue: true })
    const next = w.mine.seal()
    await keys(w.me).create({ rootId: out.rootId, installId: w.mine.id, epoch: 2, publicKey: next.publicKey, wraps: next.wraps })
    expect(await target()).toMatchObject({ epoch: 2, publicKey: next.publicKey, rotationDue: false })
  })
})
