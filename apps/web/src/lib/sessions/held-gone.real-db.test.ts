// @vitest-environment node
// #EXP-2138 (held-gone.json): DELETING REMOVES THE TRANSCRIPT, the server's
// half against the REAL migrated Postgres. Locks:
//   * `sessions.heldGone`: deleted chat, agent and team are gone; everything
//     else keeps the holder's copy (ended, archived, board trash, device move,
//     group-chat move, a left team's own chat, a machine's owner); the
//     Assistant's move is in assistant-move.real-db.test.ts;
//   * unknown and forbidden answer the same; an API key is refused (FORBIDDEN);
//   * a delete sends `kill {reason: deleted}` for EVERY session of the tree,
//     live or not, after the rows are gone.
import { afterAll, afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { and, eq, inArray, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, apikeys, boards, devices, issues, sessionIssues, sessions, teamImports, teamMembers, teams, users } from "@/db/schema"
import { inRollback, probeRealDb, retryingConflict } from "@/test/real-db"

const relay = vi.hoisted(() => ({
  kills: [] as Array<{ sessionId: string; userId: string; deviceId: string; reason?: string }>,
  after: null as null | (() => Promise<void>),
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  getSteerRelayConfig: () => ({ url: `ws://relay.test`, secret: `s` }),
  relayPostNudge: async () => {},
  relayPostInterrupt: async () => ({ delivered: true }),
  relayPostKill: async (_c: unknown, sessionId: string, device: { userId: string; deviceId: string }, _f: unknown, reason?: string) => {
    relay.kills.push({ sessionId, ...device, reason })
    await relay.after?.()
    return { delivered: true }
  },
}))
vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
vi.mock(`@/lib/storage`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/storage")>()),
  deleteObject: async () => {},
}))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors') AS ok
`)
if (!ready.ok) console.info(`[held-gone.real-db] skipped: ${ready.reason}`)

const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
const { agentsRouter } = await import(`@/lib/trpc/agents`)
const { teamsRouter } = await import(`@/lib/trpc/teams`)
const { adminRouter } = await import(`@/lib/trpc/admin`)
const { sendSession } = await import(`@/lib/sessions/send`)
const { heldGone } = await import(`@/lib/sessions/held-gone`)
const { runGroupChatMove } = await import(`@/lib/agents/group-chat`)
const { sweepAbandonedTeamImports, TEAM_IMPORT_ABANDONED_MS } = await import(`@/lib/team-import/sweep`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(isAdmin = false): Promise<string> {
  const id = `held-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@held.test`}, true, ${isAdmin}, now(), now())
  `)
  made.users.push(id)
  return id
}
async function team(members: Array<[string, `owner` | `member`]>): Promise<string> {
  const [row] = await db.insert(teams).values({ name: `Held`, slug: `held-${id8()}` }).returning({ id: teams.id })
  made.teams.push(row!.id)
  await db.insert(teamMembers).values(members.map(([userId, role]) => ({ teamId: row!.id, userId, role })))
  return row!.id
}
async function device(userId: string, sharedTeamIds: string[] = []): Promise<string> {
  const deviceId = `dev-${id8()}`
  const agentAccounts = { claude: { signedIn: true, email: `me@held.test`, profiles: [{ id: `p`, signedIn: true, email: `me@held.test`, active: true }] } }
  await db.insert(devices).values({ userId, deviceId, label: `box`, kind: `server`, version: `0.14.180`, lastSeenAt: new Date(), sharedTeamIds, agentAccounts })
  return deviceId
}
async function chat(values: { teamId: string; userId: string; deviceId: string; shared?: boolean; status?: `running` | `ended` | `idle`; parentId?: string; agentId?: string }): Promise<string> {
  const id = crypto.randomUUID()
  await db.insert(sessions).values({ id, rootId: values.parentId ?? id, depth: values.parentId ? 1 : 0, status: `ended`, ...values } as never)
  return id
}
const ctx = (userId: string, sessionId = `s`) =>
  ({ session: { user: { id: userId }, session: { id: sessionId } }, db, request: new Request(`http://x.test`) }) as never
const as = (userId: string) => sessionsRouter.createCaller(ctx(userId))
const gone = async (userId: string, ...ids: string[]) => (await as(userId).heldGone({ ids })).gone
const kills = () => relay.kills.map((kill) => kill.sessionId).sort()

beforeEach(() => {
  relay.kills = []
  relay.after = null
})
afterEach(() => {
  delete process.env.AGENT_GROUP_CHATS
})
afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.delete(teams).where(inArray(teams.id, made.teams))
  if (made.users.length) await db.delete(users).where(inArray(users.id, made.users))
})

describe.skipIf(!ready.ok)(`what the server says is gone (#EXP-2138)`, () => {
  it(`a deleted chat: gone with its tree, every device told after the commit, live or not`, async () => {
    const me = await user()
    const teamId = await team([[me, `owner`]])
    const [boxA, boxB] = [await device(me), await device(me)]
    const root = await chat({ teamId, userId: me, deviceId: boxA })
    const child = await chat({ teamId, userId: me, deviceId: boxB, parentId: root, status: `running` })
    const other = await chat({ teamId, userId: me, deviceId: boxA })
    expect(await gone(me, root, child, other)).toEqual([])
    const left: number[] = []
    relay.after = async () => {
      left.push((await db.select().from(sessions).where(eq(sessions.rootId, root))).length)
    }
    await as(me).delete({ sessionId: root })
    expect(left).toEqual([0, 0])
    expect(relay.kills.map((kill) => [kill.sessionId, kill.userId, kill.deviceId, kill.reason])).toEqual([
      [child, me, boxB, `deleted`],
      [root, me, boxA, `deleted`],
    ])
    expect(await gone(me, other, root, child, root)).toEqual([root, child])
  }, 60_000)

  it(`an ended chat's End sends a plain kill, and keeps it`, async () => {
    const me = await user()
    const teamId = await team([[me, `owner`]])
    const box = await device(me)
    const root = await chat({ teamId, userId: me, deviceId: box, status: `running` })
    await as(me).end({ sessionId: root })
    expect(relay.kills).toEqual([{ sessionId: root, userId: me, deviceId: box, reason: undefined }])
    expect(await gone(me, root)).toEqual([])
  }, 60_000)

  it(`a deleted agent: its conversations' trees are gone and killed; a teammate's kill goes to the box's owner`, async () => {
    const me = await user()
    const mate = await user()
    const teamId = await team([[me, `owner`], [mate, `member`]])
    const box = await device(me, [teamId])
    const [agent] = await db.insert(agents).values({ teamId, ownerId: me, deviceId: box, name: `Triage` } as never).returning()
    const mine = await chat({ teamId, userId: me, deviceId: box, agentId: agent!.id, status: `idle` })
    const theirs = await chat({ teamId, userId: mate, deviceId: box, agentId: agent!.id, status: `idle`, shared: true })
    const child = await chat({ teamId, userId: mate, deviceId: box, parentId: theirs })
    await agentsRouter.createCaller(ctx(me)).delete({ id: agent!.id })
    expect(kills()).toEqual([mine, theirs, child].sort())
    expect(relay.kills.every((kill) => kill.userId === me && kill.deviceId === box && kill.reason === `deleted`)).toBe(true)
    expect(await gone(me, mine, theirs, child)).toEqual([mine, theirs, child])
    expect(await gone(mate, theirs)).toEqual([theirs])
  }, 60_000)

  it(`a deleted team, by its owner, by the admin, by the import sweep`, async () => {
    const me = await user()
    const admin = await user(true)
    const box = await device(me)
    for (const drop of [
      (teamId: string) => teamsRouter.createCaller(ctx(me)).delete({ teamId }),
      (teamId: string) => adminRouter.createCaller(ctx(admin)).deleteTeam({ teamId }),
      async (teamId: string) => {
        await db.insert(teamImports).values({ teamId, createdByUserId: me, status: `failed`, manifest: {}, createdAt: new Date(Date.now() - TEAM_IMPORT_ABANDONED_MS - 60_000) } as never)
        expect(await sweepAbandonedTeamImports()).toBeGreaterThan(0)
      },
    ]) {
      relay.kills = []
      const teamId = await team([[me, `owner`]])
      const root = await chat({ teamId, userId: me, deviceId: box })
      await drop(teamId)
      expect(relay.kills).toEqual([{ sessionId: root, userId: me, deviceId: box, reason: `deleted` }])
      expect(await gone(me, root)).toEqual([root])
    }
  }, 60_000)

  it(`left the team: the own chat stays, a teammate's shared one is gone; rejoined, it stays again`, async () => {
    const owner = await user()
    const me = await user()
    const teamId = await team([[owner, `owner`], [me, `member`]])
    const mine = await chat({ teamId, userId: me, deviceId: await device(me) })
    const theirs = await chat({ teamId, userId: owner, deviceId: await device(owner, [teamId]), shared: true })
    expect(await gone(me, mine, theirs)).toEqual([])
    await db.delete(teamMembers).where(and(eq(teamMembers.teamId, teamId), eq(teamMembers.userId, me)))
    expect(await gone(me, mine, theirs)).toEqual([theirs])
    await db.insert(teamMembers).values({ teamId, userId: me, role: `member` })
    expect(await gone(me, mine, theirs)).toEqual([])
  }, 60_000)

  it(`shared turned off: gone for a teammate, kept for the starter and for the owner of the machine`, async () => {
    const host = await user()
    const mate = await user()
    const starter = await user()
    const teamId = await team([[host, `owner`], [mate, `member`], [starter, `member`]])
    const box = await device(host, [teamId])
    const root = await chat({ teamId, userId: starter, deviceId: box, shared: true })
    await db.update(sessions).set({ shared: false }).where(eq(sessions.id, root))
    await db.update(devices).set({ sharedTeamIds: [] }).where(eq(devices.deviceId, box))
    expect(await gone(mate, root)).toEqual([root])
    expect(await gone(starter, root)).toEqual([])
    expect(await gone(host, root)).toEqual([])
    // Even after the machine's owner left the team.
    await db.delete(teamMembers).where(and(eq(teamMembers.teamId, teamId), eq(teamMembers.userId, host)))
    expect(await gone(host, root)).toEqual([])
  }, 60_000)

  it(`a teammate's private chat that moved to my machine stays; a team owner's role alone keeps nothing`, async () => {
    const me = await user()
    const mate = await user()
    const teamId = await team([[me, `owner`], [mate, `member`]])
    const box = await device(me, [teamId])
    const away = await device(mate)
    const root = await chat({ teamId, userId: mate, deviceId: away })
    expect(await gone(me, root)).toEqual([root])
    await db.update(sessions).set({ deviceId: box }).where(eq(sessions.id, root))
    expect(await gone(me, root)).toEqual([])
  }, 60_000)

  it(`board in the trash, archive, end and a device move keep it`, async () => {
    const me = await user()
    const teamId = await team([[me, `owner`]])
    const [from, to] = [await device(me), await device(me)]
    const root = await chat({ teamId, userId: me, deviceId: from })
    const [board] = await db.insert(boards).values({ teamId, name: `B`, slug: `b-${id8()}`, prefix: `HG` }).returning({ id: boards.id })
    const [issue] = await db.insert(issues).values({ boardId: board!.id, teamId, title: `x`, creatorId: me } as never).returning({ id: issues.id })
    await db.insert(sessionIssues).values({ sessionId: root, issueId: issue!.id, rootId: root, teamId, ownerId: me } as never)
    await db.update(boards).set({ deletedAt: new Date() }).where(eq(boards.id, board!.id))
    expect(await gone(me, root)).toEqual([])
    await db.delete(boards).where(eq(boards.id, board!.id))
    expect(await gone(me, root)).toEqual([])
    const started = await sendSession({ userId: me, input: { teamId, deviceId: from, cli: `claude`, account: `p`, text: `work` } })
    await as(me).claim({ sessionId: started.sessionId, deviceId: from })
    const moved = await sendSession({ userId: me, input: { sessionId: started.sessionId, deviceId: to, text: `go on there` } })
    const [row] = await db.select().from(sessions).where(eq(sessions.id, moved.sessionId))
    expect(row!.deviceId).toBe(to)
    expect(await gone(me, root, started.sessionId, moved.sessionId)).toEqual([])
    await as(me).setArchived({ sessionIds: [root], archived: true })
    expect(await gone(me, root)).toEqual([])
  }, 60_000)

  it(`the group-chat move keeps the private chat for its member and for the machine's owner`, async () => {
    const owner = await user()
    const mate = await user()
    const teamId = await team([[owner, `owner`], [mate, `member`]])
    const box = await device(owner, [teamId])
    const [agent] = await db.insert(agents).values({ teamId, ownerId: owner, deviceId: box, name: `Triage` } as never).returning()
    const theirs = await chat({ teamId, userId: mate, deviceId: box, agentId: agent!.id, status: `idle`, shared: true })
    process.env.AGENT_GROUP_CHATS = `on`
    await retryingConflict(() =>
      inRollback(async (tx) => {
        expect(await runGroupChatMove(tx as never)).toContain(theirs)
        const [row] = await tx.select().from(sessions).where(eq(sessions.id, theirs))
        expect(row).toMatchObject({ agentId: null, shared: false, status: `ended` })
        expect(await heldGone(mate, [theirs], tx)).toEqual([])
        expect(await heldGone(owner, [theirs], tx)).toEqual([])
      })
    )
  }, 60_000)

  it(`a foreign id answers like an unknown one; an API key and an MCP call are refused`, async () => {
    const me = await user()
    const stranger = await user()
    const teamId = await team([[stranger, `owner`]])
    const foreign = await chat({ teamId, userId: stranger, deviceId: await device(stranger), shared: true })
    const unknown = crypto.randomUUID()
    expect(await gone(me, foreign, unknown)).toEqual([foreign, unknown])
    expect(await gone(me)).toEqual([])
    const keyId = `key-${id8()}`
    await db.insert(apikeys).values({ id: keyId, configId: `default`, key: id8(), referenceId: me, createdAt: new Date(), updatedAt: new Date() } as never)
    await expect(sessionsRouter.createCaller(ctx(me, keyId)).heldGone({ ids: [unknown] })).rejects.toMatchObject({ code: `FORBIDDEN` })
    await expect(
      sessionsRouter.createCaller({ ...(ctx(me) as object), viaMcp: true } as never).heldGone({ ids: [unknown] })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
    await expect(as(me).heldGone({ ids: Array.from({ length: 501 }, () => crypto.randomUUID()) })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(sessionsRouter.createCaller({ session: null, db, request: new Request(`http://x.test`) } as never).heldGone({ ids: [] })).rejects.toMatchObject({ code: `UNAUTHORIZED` })
  }, 60_000)
})
