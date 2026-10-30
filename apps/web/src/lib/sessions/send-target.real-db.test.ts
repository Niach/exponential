// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, apps, boards, devices, installs, messages, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { editorBrief } from "@exp/domain-contract/apps"
import { generateTreeKey, openBody, sealBody, wrapTreeKey } from "@exp/domain-contract/sealed-content"

// EXP-1458: a send to an AGENT or an APP sealed by the writer: the target
// query names the conversation root (or none yet) and its key, the first
// sealed send carries `seal`, later ones seal to the root's key, and an
// app's brief is the writer's to compose. Against the REAL migrated Postgres.

process.env.BETTER_AUTH_SECRET ||= `send-target-test-secret-0123456789abcdef`

vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
  notifyAssistantDecision: async () => {},
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: false }),
}))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'session_keys') AS ok
`)
if (!ready.ok) console.info(`[send-target.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

const caller = (userId: string) =>
  sessionsRouter.createCaller({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) } as never)

async function world() {
  const owner = `send-target-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${owner}, 'Tester', ${`${owner}@sealed.test`}, true, false, now(), now())
  `)
  made.users.push(owner)
  const [team] = await db.insert(teams).values({ name: `Target`, slug: `target-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `ST` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const installKey = generateTreeKey()
  const [install] = await db
    .insert(installs)
    .values({ userId: owner, kind: `desktop`, endpointId: `ep-${crypto.randomUUID()}`, x25519Public: installKey.publicKey })
    .returning({ id: installs.id })
  const newSeal = () => {
    const tree = generateTreeKey()
    const seal = {
      installId: install!.id,
      publicKey: tree.publicKey,
      epoch: 1 as const,
      wraps: [{ installId: install!.id, wrapped: wrapTreeKey(installKey.publicKey, tree.secret) }],
    }
    return { tree, seal }
  }
  return { owner, teamId, deviceId, newSeal }
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

const bodyOf = async (id: string | null | undefined) =>
  (await db.select({ body: messages.body }).from(messages).where(eq(messages.id, id!)).limit(1))[0]!.body

describe.skipIf(!ready.ok)(`sessions.sendTarget`, () => {
  it(`an agent: no conversation yet, then the sealed first send keys it, later sends seal to that key`, async () => {
    const w = await world()
    const [agent] = await db.insert(agents).values({ teamId: w.teamId, name: `Helper`, deviceId: w.deviceId, ownerId: w.owner }).returning()
    const api = caller(w.owner)
    expect(await api.sendTarget({ agentId: agent!.id })).toEqual({ rootId: null, epoch: null, publicKey: null, brief: null })

    const { tree, seal } = w.newSeal()
    const first = sealBody(tree.publicKey, 1, `hello @someone`)
    const sent = await sendSession({ userId: w.owner, input: { agentId: agent!.id, text: first, seal } })
    expect(await bodyOf(sent.messageId)).toBe(first)
    const target = await api.sendTarget({ agentId: agent!.id })
    expect(target).toEqual({ rootId: sent.sessionId, epoch: 1, publicKey: tree.publicKey, brief: null })

    const second = sealBody(target.publicKey!, target.epoch!, `again`)
    const again = await sendSession({ userId: w.owner, input: { agentId: agent!.id, text: second } })
    expect(again.sessionId).toBe(sent.sessionId)
    expect(openBody(await bodyOf(again.messageId), new Map([[1, tree.secret]]))).toMatchObject({ status: `opened`, text: `again` })
    // A second key for a keyed conversation is refused (the target says it has one).
    await expect(
      sendSession({ userId: w.owner, input: { agentId: agent!.id, text: sealBody(tree.publicKey, 1, `x`), seal: w.newSeal().seal } })
    ).rejects.toThrow()
  })

  it(`an app: the brief rides only while the send would brief (new or ended editor)`, async () => {
    const w = await world()
    const [app] = await db.insert(apps).values({ teamId: w.teamId, slug: `notes-${id8()}`, name: `Notes` }).returning()
    const api = caller(w.owner)
    const fresh = await api.sendTarget({ appId: app!.id })
    expect(fresh).toEqual({ rootId: null, epoch: null, publicKey: null, brief: editorBrief(app!) })

    const { tree, seal } = w.newSeal()
    const text = sealBody(tree.publicKey, 1, `${fresh.brief}\n\nmake it blue`)
    const sent = await sendSession({ userId: w.owner, input: { appId: app!.id, deviceId: w.deviceId, text, seal } })
    // The sealed text is stored as the writer composed it (no second brief).
    expect(await bodyOf(sent.messageId)).toBe(text)
    expect(await api.sendTarget({ appId: app!.id })).toEqual({ rootId: sent.sessionId, epoch: 1, publicKey: tree.publicKey, brief: null })

    await db.update(sessions).set({ status: `ended`, endedAt: new Date() }).where(eq(sessions.id, sent.sessionId))
    expect((await api.sendTarget({ appId: app!.id })).brief).toBe(editorBrief(app!))
  })

  it(`names one target and hides another member's assistant`, async () => {
    const w = await world()
    const api = caller(w.owner)
    await expect(api.sendTarget({})).rejects.toThrow()
    const other = `send-target-${id8()}`
    await db.execute(sql`
      INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
      VALUES (${other}, 'Other', ${`${other}@sealed.test`}, true, false, now(), now())
    `)
    made.users.push(other)
    await db.insert(teamMembers).values({ teamId: w.teamId, userId: other, role: `member` })
    const [assistant] = await db
      .insert(agents)
      .values({ teamId: w.teamId, name: `Mine`, deviceId: w.deviceId, ownerId: w.owner, role: `assistant` })
      .returning()
    await expect(caller(other).sendTarget({ agentId: assistant!.id })).rejects.toThrow(/not found/i)
  })
})
