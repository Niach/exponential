// @vitest-environment node
// #EXP-1849 (ONE root per app and member) against the REAL migrated
// Postgres: a member's editor root IS their conversation with the app's
// agent (app_id + agent_id); the agent's paths (a trigger's wake) land in
// that root; it never ends; the app agent's SLOT (its device) is empty or a
// team-shared device and moves every member's root when set; the migration
// merges legacy roots.
import { readFileSync } from "node:fs"
import { resolve } from "node:path"
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, isNull, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, apps, devices, messages, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

process.env.BETTER_AUTH_SECRET ||= `one-root-test-secret-0123456789abcdef`

vi.mock(`@/lib/integrations/notifications`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/integrations/notifications")>()),
  fireAndForgetSessionMessageNotify: () => {},
  notifySessionBlocked: async () => {},
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  getSteerRelayConfig: () => null,
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
}))
vi.mock(`@/lib/conversion/events`, () => ({ recordConversionEvent: async () => {} }))

const ready = await probeRealDb(`SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'apps' AND column_name = 'agent_id') AS ok`)
if (!ready.ok) console.info(`[one-root.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { endSession } = await import(`@/lib/sessions/lifecycle`)
const { postAgentInputInTx } = await import(`@/lib/agents/triggers`)
const { createAppAgentInTx } = await import(`@/lib/apps/app-agent`)
const { agentsRouter } = await import(`@/lib/trpc/agents`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `one-root-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@one-root.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function world() {
  const owner = await user()
  const member = await user()
  const [team] = await db.insert(teams).values({ name: `One root`, slug: `one-root-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner, role: `owner` },
    { teamId, userId: member, role: `member` },
  ])
  const ownerDevice = `own-${id8()}`
  const memberDevice = `mem-${id8()}`
  const server = `srv-${id8()}`
  await db.insert(devices).values([
    { userId: owner, deviceId: ownerDevice, label: `owner box`, kind: `desktop`, isDefault: true, lastSeenAt: new Date() },
    { userId: member, deviceId: memberDevice, label: `member box`, kind: `desktop`, isDefault: true, lastSeenAt: new Date() },
    { userId: owner, deviceId: server, label: `build`, kind: `server`, lastSeenAt: new Date(), sharedTeamIds: [teamId] },
  ])
  const app = await db.transaction(async (tx) => {
    const [row] = await tx.insert(apps).values({ teamId, slug: `notes-${id8()}`, name: `Notes`, createdBy: owner }).returning()
    row!.agentId = await createAppAgentInTx(tx, row!, owner)
    return row!
  })
  return { owner, member, teamId, ownerDevice, memberDevice, server, app, agentId: app.agentId! }
}

const rootsOf = (appId: string) => db.select().from(sessions).where(and(eq(sessions.appId, appId), isNull(sessions.parentId)))
const asUser = (userId: string) => agentsRouter.createCaller({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) } as never)

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`one root per app and member (#EXP-1849)`, () => {
  it(`a new app's agent starts with an empty slot; a member's editor root is their chat with it and never ends`, async () => {
    const w = await world()
    const [agent] = await db.select().from(agents).where(eq(agents.id, w.agentId))
    expect(agent?.deviceId).toBe(``)
    const sent = await sendSession({ userId: w.member, input: { appId: w.app.id, deviceId: w.memberDevice, text: `make it blue` } })
    const [root] = await db.select().from(sessions).where(eq(sessions.id, sent.sessionId))
    expect(root).toMatchObject({ appId: w.app.id, agentId: w.agentId, userId: w.member, deviceId: w.memberDevice, parentId: null })
    // An older client still ends the editor root on leave: a no-op.
    await endSession({ userId: w.member, sessionId: root!.id })
    const [after] = await db.select({ status: sessions.status }).from(sessions).where(eq(sessions.id, root!.id))
    expect(after?.status).not.toBe(`ended`)
    // The agent's own path (a message to the agent) lands in the same root.
    const again = await sendSession({ userId: w.member, input: { agentId: w.agentId, text: `and green` } })
    expect(again.sessionId).toBe(root!.id)
    expect(await rootsOf(w.app.id)).toHaveLength(1)
  })

  it(`a trigger's wake opens the owner's app root on their default device, with the app brief`, async () => {
    const w = await world()
    await db.transaction(async (tx) => {
      const [agent] = await tx.select().from(agents).where(eq(agents.id, w.agentId))
      await postAgentInputInTx(tx, agent!, w.owner, `Schedule fired.`, { triggerId: null, eventId: null, name: null })
    })
    const [root] = await rootsOf(w.app.id)
    expect(root).toMatchObject({ agentId: w.agentId, userId: w.owner, deviceId: w.ownerDevice })
    const [first] = await db.select({ body: messages.body }).from(messages).where(eq(messages.sessionId, root!.id))
    expect(first?.body).toContain(`Notes`)
    expect(first?.body).toContain(`Schedule fired.`)
  })

  it(`the slot takes a team-shared device only, moves every member's root there, and empties again`, async () => {
    const w = await world()
    const sent = await sendSession({ userId: w.member, input: { appId: w.app.id, deviceId: w.memberDevice, text: `hi` } })
    const owner = asUser(w.owner)
    await expect(owner.update({ id: w.agentId, deviceId: w.ownerDevice })).rejects.toThrow(/shared with the team/)
    await owner.update({ id: w.agentId, deviceId: w.server })
    const [moved] = await db.select({ deviceId: sessions.deviceId }).from(sessions).where(eq(sessions.id, sent.sessionId))
    expect(moved?.deviceId).toBe(w.server)
    // A new member root starts on the slot whatever the member picks.
    const ownerSent = await sendSession({ userId: w.owner, input: { appId: w.app.id, deviceId: w.ownerDevice, text: `hi` } })
    const [ownerRoot] = await db.select({ deviceId: sessions.deviceId }).from(sessions).where(eq(sessions.id, ownerSent.sessionId))
    expect(ownerRoot?.deviceId).toBe(w.server)
    await owner.update({ id: w.agentId, deviceId: `` })
    const [agent] = await db.select({ deviceId: agents.deviceId }).from(agents).where(eq(agents.id, w.agentId))
    expect(agent?.deviceId).toBe(``)
    // Members cannot write the slot; a plain agent never empties its device.
    await expect(asUser(w.member).update({ id: w.agentId, deviceId: w.server })).rejects.toThrow()
    const [plain] = await db.insert(agents).values({ teamId: w.teamId, name: `Plain`, deviceId: w.server, ownerId: w.owner }).returning({ id: agents.id })
    await expect(owner.update({ id: plain!.id, deviceId: `` })).rejects.toThrow(/Pick a device/)
  })

  it(`the migration merges a legacy editor root with the agent and keeps an older conversation as a plain chat`, async () => {
    const w = await world()
    // A pre-#EXP-1849 world: the agent on the owner's own device, an editor
    // root without the agent and a separate conversation with it.
    await db.update(agents).set({ deviceId: w.ownerDevice }).where(eq(agents.id, w.agentId))
    const editorId = crypto.randomUUID()
    const convoId = crypto.randomUUID()
    await db.insert(sessions).values([
      { id: editorId, teamId: w.teamId, rootId: editorId, depth: 0, appId: w.app.id, userId: w.owner, deviceId: w.ownerDevice, status: `idle`, shared: false, title: `Notes` },
      { id: convoId, teamId: w.teamId, rootId: convoId, depth: 0, agentId: w.agentId, userId: w.owner, deviceId: w.ownerDevice, status: `idle`, shared: false, title: `Notes` },
    ])
    const migration = readFileSync(resolve(import.meta.dirname, `../../db/out/0283_exp1849_one_app_root.sql`), `utf8`)
    for (const statement of migration.split(`--> statement-breakpoint`)) await db.execute(sql.raw(statement))
    const [editor] = await db.select({ agentId: sessions.agentId }).from(sessions).where(eq(sessions.id, editorId))
    const [convo] = await db.select({ agentId: sessions.agentId }).from(sessions).where(eq(sessions.id, convoId))
    expect(editor?.agentId).toBe(w.agentId)
    expect(convo?.agentId).toBeNull()
    const [agent] = await db.select({ deviceId: agents.deviceId }).from(agents).where(eq(agents.id, w.agentId))
    expect(agent?.deviceId).toBe(``)
  })
})
