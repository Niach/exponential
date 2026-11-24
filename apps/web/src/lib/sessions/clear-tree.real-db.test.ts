// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { boards, devices, installs, messages, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import fixture from "@exp/domain-contract/fixtures/sealed-content.json"
import { generateTreeKey, sealBody, sealCard, wrapTreeKey } from "@exp/domain-contract/sealed-content"

// #EDE-97 (sealed-content.json `clearTree`): an outside MCP client's chat
// stays in clear for life, a server-written first message is re-sealed once
// by its executing device, plaintext cards are refused in keyed trees.

process.env.BETTER_AUTH_SECRET ||= `clear-tree-test-secret-0123456789abcdef`

vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
  notifySessionAsk: async () => {},
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: false }),
}))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns
    WHERE table_name = 'sessions' AND column_name = 'clear_tree') AS ok
`)
if (!ready.ok) console.info(`[clear-tree.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { postShowCard, buildPlanCard, buildSealedShowCard } = await import(`@/lib/sessions/plan`)
const { postReplies, postSuggestions } = await import(`@/lib/sessions/suggestions`)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
const { createTreeKeyInTx } = await import(`@/lib/sessions/tree-keys`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `clear-tree-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@clear.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

const caller = (userId: string) =>
  sessionsRouter.createCaller({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) } as never)

async function world() {
  const owner = await user()
  const [team] = await db.insert(teams).values({ name: `Clear`, slug: `clear-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `CT` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const installKey = generateTreeKey()
  const [install] = await db
    .insert(installs)
    .values({ userId: owner, kind: `desktop`, endpointId: `ep-${crypto.randomUUID()}`, x25519Public: installKey.publicKey })
    .returning({ id: installs.id })
  const keyOf = (rootId: string) => {
    const tree = generateTreeKey()
    return {
      tree,
      input: {
        rootId,
        installId: install!.id,
        epoch: 1,
        publicKey: tree.publicKey,
        wraps: [{ installId: install!.id, wrapped: wrapTreeKey(installKey.publicKey, tree.secret) }],
      },
    }
  }
  /** A root the server (or an older app) started in clear, then keyed by its device. */
  const keyedServerRoot = async (text = `run the nightly check`) => {
    const { sessionId, messageId } = await sendSession({ userId: owner, input: { teamId, deviceId, text } })
    const key = keyOf(sessionId)
    await db.transaction((tx) => createTreeKeyInTx(tx, owner, key.input))
    return { rootId: sessionId, messageId: messageId!, tree: key.tree }
  }
  return { owner, teamId, deviceId, keyOf, keyedServerRoot }
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

const root = async (id: string) => (await db.select().from(sessions).where(eq(sessions.id, id)).limit(1))[0]!
const message = async (id: string) => (await db.select().from(messages).where(eq(messages.id, id)).limit(1))[0]!

describe.skipIf(!ready.ok)(`clear trees (startCases)`, () => {
  it(`only an outside MCP client's start is clear for life; nobody keys it; plaintext keeps flowing`, async () => {
    const w = await world()
    const outside = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `hi` }, viaMcp: true })
    expect((await root(outside.sessionId)).clearTree).toBe(true)
    const app = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `hi` } })
    expect((await root(app.sessionId)).clearTree).toBe(false)
    // sessionKeys.create (and so an inline seal) is refused for the clear root.
    await expect(db.transaction((tx) => createTreeKeyInTx(tx, w.owner, w.keyOf(outside.sessionId).input))).rejects.toThrow(
      fixture.clearTree.clearRefusal
    )
    expect((await root(outside.sessionId)).sealEpoch).toBeNull()
    const again = await sendSession({ userId: w.owner, input: { sessionId: outside.sessionId, text: `more` }, viaMcp: true })
    expect((await message(again.messageId!)).body).toBe(`more`)
    // Plaintext cards stay legal there (the clear tree's agent posts in clear).
    const plan = buildPlanCard({ plan: `Do it` })
    await expect(postShowCard({ userId: w.owner, sessionId: outside.sessionId, card: plan, replace: false })).resolves.toBeTruthy()
  })
})

describe.skipIf(!ready.ok)(`re-seal (resealCases)`, () => {
  it(`the executing device re-seals the first message once`, async () => {
    const w = await world()
    const r = await w.keyedServerRoot()
    const api = caller(w.owner)
    const target = await api.resealTarget({ sessionId: r.rootId, deviceId: w.deviceId })
    expect(target).toEqual({ messageId: r.messageId, body: `run the nightly check` })
    // Another device is refused.
    await expect(api.resealTarget({ sessionId: r.rootId, deviceId: `other` })).rejects.toThrow()
    const body = sealBody(r.tree.publicKey, 1, `run the nightly check`)
    await expect(api.reseal({ sessionId: r.rootId, deviceId: `other`, messageId: r.messageId, body })).rejects.toThrow()
    // Not an envelope / a future epoch.
    await expect(api.reseal({ sessionId: r.rootId, deviceId: w.deviceId, messageId: r.messageId, body: `plain` })).rejects.toThrow()
    await expect(
      api.reseal({ sessionId: r.rootId, deviceId: w.deviceId, messageId: r.messageId, body: sealBody(r.tree.publicKey, 2, `x`) })
    ).rejects.toThrow()
    await api.reseal({ sessionId: r.rootId, deviceId: w.deviceId, messageId: r.messageId, body })
    expect((await message(r.messageId)).body).toBe(body)
    // One rewrite only.
    expect(await api.resealTarget({ sessionId: r.rootId, deviceId: w.deviceId })).toBeNull()
    await expect(api.reseal({ sessionId: r.rootId, deviceId: w.deviceId, messageId: r.messageId, body })).rejects.toThrow()
  })

  it(`refuses a keyless tree, a clear tree, a later message and a first message the agent already answered`, async () => {
    const w = await world()
    const api = caller(w.owner)
    const keyless = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `k` } })
    expect(await api.resealTarget({ sessionId: keyless.sessionId, deviceId: w.deviceId })).toBeNull()
    const clear = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `c` }, viaMcp: true })
    expect(await api.resealTarget({ sessionId: clear.sessionId, deviceId: w.deviceId })).toBeNull()

    const r = await w.keyedServerRoot()
    const later = await sendSession({ userId: w.owner, input: { sessionId: r.rootId, text: sealBody(r.tree.publicKey, 1, `later`) } })
    await expect(
      api.reseal({ sessionId: r.rootId, deviceId: w.deviceId, messageId: later.messageId!, body: sealBody(r.tree.publicKey, 1, `later`) })
    ).rejects.toThrow()
    await api.reply({ sessionId: r.rootId, body: sealBody(r.tree.publicKey, 1, `done`) })
    expect(await api.resealTarget({ sessionId: r.rootId, deviceId: w.deviceId })).toBeNull()
    await expect(
      api.reseal({ sessionId: r.rootId, deviceId: w.deviceId, messageId: r.messageId, body: sealBody(r.tree.publicKey, 1, `x`) })
    ).rejects.toThrow()
  })
})

describe.skipIf(!ready.ok)(`cards (cardCases)`, () => {
  it(`a plaintext plan into a keyed tree is refused, a sealed one lands, a keyless tree takes plaintext`, async () => {
    const w = await world()
    const r = await w.keyedServerRoot()
    const plan = buildPlanCard({ plan: `Do it` })
    await expect(postShowCard({ userId: w.owner, sessionId: r.rootId, card: plan, replace: false })).rejects.toThrow(
      /end-to-end encrypted/
    )
    const sealed = buildSealedShowCard(sealCard(r.tree.publicKey, 1, plan as never), `plan`)
    await expect(postShowCard({ userId: w.owner, sessionId: r.rootId, card: sealed, replace: false })).resolves.toBeTruthy()
    const keyless = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `k` } })
    await expect(postShowCard({ userId: w.owner, sessionId: keyless.sessionId, card: plan, replace: false })).resolves.toBeTruthy()
  })

  it(`#EXP-2118: plaintext suggestions and replies into a keyed tree are refused and nothing of them is stored`, async () => {
    const w = await world()
    const r = await w.keyedServerRoot()
    const tasks = [{ title: `CLEAR-2118`, text: `CLEAR-2118` }]
    const replies = [`CLEAR-2118`]
    const refused = /end-to-end encrypted/
    await expect(postSuggestions({ userId: w.owner, callerSessionId: r.rootId, tasks })).rejects.toThrow(refused)
    await expect(postReplies({ userId: w.owner, callerSessionId: r.rootId, replies })).rejects.toThrow(refused)
    await expect(
      postShowCard({ userId: w.owner, sessionId: r.rootId, card: buildPlanCard({ plan: `CLEAR-2118` }), replace: false })
    ).rejects.toThrow(refused)
    const rows = await db.select().from(messages).where(eq(messages.sessionId, r.rootId))
    expect(JSON.stringify(rows)).not.toContain(`CLEAR-2118`)
    const keyless = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `k` } })
    await expect(postReplies({ userId: w.owner, callerSessionId: keyless.sessionId, replies })).resolves.toBeTruthy()
  })
})
