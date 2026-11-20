// @vitest-environment node
// #EXP-1986 (agent-settings.json `assistant.move`, #EXP-1984): the move
// (lib/agents/assistant-move.sql, behind its switch) against the REAL migrated Postgres, replayed in a rolled-back
// transaction over rows shaped like before it (per-team Assistants), scoped
// to this file's members so parallel suites never see it. Locks:
//   * the pick replays `keepCases` (the TS reference `assistantMoveOf`);
//   * the kept conversation moves into the personal team with its ids, its
//     tree key and its sealed history, and stays readable by its member;
//   * THE PRIVACY HOLE: an Assistant chat started on a device shared with
//     the team was readable and steerable by teammates (`shared`); after the
//     move it is private, and the teammates' wraps of its key are gone;
//   * the other conversation ends, detached, readable in its team;
//   * references to the dropped Assistant point to the kept one;
//   * a NEW Assistant conversation on a team-shared device is private;
//   * #EXP-1988: every per-team Assistant's skill and MCP picks carry into
//     the kept one (and its conversation), and each team's work launches
//     with that team's picks before and after the move.
import { afterAll, describe, expect, it, onTestFinished, vi } from "vitest"
import { and, eq, inArray, sql } from "drizzle-orm"
import { ASSISTANT_MOVE, assistantMoveOf } from "@exp/domain-contract/agent-settings"
import {
  b64urlEncode,
  generateTreeKey,
  openBody,
  pushPublicKey,
  sealBody,
  unwrapTreeKey,
  wrapTreeKey,
} from "@exp/domain-contract/sealed-content"
import { db } from "@/db/connection"
import {
  agents,
  boards,
  devices,
  installs,
  issues,
  mcpServers,
  messages,
  sessionKeys,
  sessions,
  skills,
  teamMembers,
  teams,
  userVoiceSettings,
} from "@/db/schema"
import { inRollback, probeRealDb, type RealDbTx } from "@/test/real-db"

vi.mock(`@/lib/steer`, async (importOriginal) => {
  const real = await importOriginal<typeof import("@/lib/steer")>()
  return {
    ...real,
    getSteerRelayConfig: () => null,
    relayPostNudge: async () => {},
    relayPostKill: async () => {},
    relayPostInterrupt: async () => ({ delivered: true }),
  }
})

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'uniq_teams_personal_owner') AS ok
`)
if (!ready.ok) console.info(`[assistant-move.real-db] skipped: ${ready.reason}`)

const { treeReaders } = await import(`@/lib/sessions/tree-keys`)
const { sessionVisibleTo, loadSessionForUser } = await import(`@/lib/sessions/access`)
const { sendSession } = await import(`@/lib/sessions/send`)
const { ensureAssistant, personalTeamSlug } = await import(`@/lib/agents/assistant`)
const { conversationInTx } = await import(`@/lib/agents/conversation`)
const { assistantMoveStatements } = await import(`@/lib/agents/assistant-move`)
const { sessionLaunchPick, sessionLaunchTeams, mergedAgentPick } = await import(`@/lib/agents/team-picks`)

const MIGRATION = assistantMoveStatements()

/** The migration, scoped to `owners` (the movers' set is its first table). */
async function runMove(tx: RealDbTx, owners: string[]) {
  const scope = owners.map((owner) => `'${owner}'`).join(`, `)
  for (const [index, statement] of MIGRATION.entries()) {
    const scoped =
      index === 0
        ? statement.replace(
            `WHERE a."role" = 'assistant' AND t."kind" <> 'personal';`,
            `WHERE a."role" = 'assistant' AND t."kind" <> 'personal' AND a."owner_id" IN (${scope});`
          )
        : statement
    if (index === 0) expect(scoped).not.toBe(statement)
    await tx.execute(sql.raw(scoped))
  }
}

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `move-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@move.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function team(members: Array<[string, `owner` | `member`]>): Promise<string> {
  const [row] = await db.insert(teams).values({ name: `Move`, slug: `move-${id8()}` }).returning({ id: teams.id })
  made.teams.push(row!.id)
  await db.insert(teamMembers).values(members.map(([userId, role]) => ({ teamId: row!.id, userId, role })))
  return row!.id
}

async function device(userId: string, sharedTeamIds: string[] = []) {
  const deviceId = `dev-${id8()}`
  const [row] = await db
    .insert(devices)
    .values({ userId, deviceId, label: `desk`, kind: `desktop`, lastSeenAt: new Date(), sharedTeamIds })
    .returning({ id: devices.id })
  return { deviceId, rowId: row!.id }
}

async function install(userId: string, deviceRowId: string | null = null) {
  const secret = crypto.getRandomValues(new Uint8Array(32))
  const x25519Public = b64urlEncode(pushPublicKey(secret))
  const [row] = await db
    .insert(installs)
    .values({ userId, kind: deviceRowId ? `desktop` : `phone`, deviceId: deviceRowId, endpointId: `ep-${id8()}${id8()}`, x25519Public } as never)
    .returning({ id: installs.id })
  return { id: row!.id, secret, x25519Public }
}

const assistant = async (tx: RealDbTx, teamId: string, ownerId: string, deviceId: string, createdAt: string) => {
  const [row] = await tx
    .insert(agents)
    .values({ teamId, ownerId, deviceId, name: `Assistant`, role: `assistant`, createdAt: new Date(createdAt), instructionsUpdatedAt: new Date(createdAt) } as never)
    .returning()
  return row!
}

const root = async (
  tx: RealDbTx,
  input: { teamId: string; agentId: string; userId: string; deviceId: string; shared?: boolean; lastActivityAt: string; key?: { publicKey: string } }
) => {
  const id = crypto.randomUUID()
  await tx.insert(sessions).values({
    id,
    rootId: id,
    teamId: input.teamId,
    agentId: input.agentId,
    userId: input.userId,
    deviceId: input.deviceId,
    shared: input.shared ?? false,
    status: `running`,
    lastActivityAt: new Date(input.lastActivityAt),
    createdAt: new Date(`2026-11-01T09:00:00Z`),
    ...(input.key ? { sealPublicKey: input.key.publicKey, sealEpoch: 1 } : {}),
  } as never)
  return id
}

const say = (tx: RealDbTx, sessionId: string, teamId: string, authorId: string, body: string) =>
  tx.insert(messages).values({
    conversationId: sql`(SELECT id FROM conversations WHERE session_id = ${sessionId})`,
    sessionId,
    rootId: sessionId,
    teamId,
    authorKind: `user`,
    authorId,
    body,
  } as never)

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.delete(teams).where(inArray(teams.id, made.teams))
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`the Assistant move (#EXP-1986)`, () => {
  it(`keeps the most recently active conversation, moved private with its key and history; ends the other`, async () => {
    const me = await user()
    const mate = await user()
    const teamA = await team([[me, `owner`], [mate, `member`]])
    const teamB = await team([[me, `owner`]])
    // My desktop is shared with team A: the chat there is `shared` (the hole).
    const desk = await device(me, [teamA])
    const myPhone = await install(me)
    const myDesk = await install(me, desk.rowId)
    const matePhone = await install(mate)
    await inRollback(async (tx) => {
      const asA = await assistant(tx, teamA, me, desk.deviceId, `2026-10-01T09:00:00Z`)
      const asB = await assistant(tx, teamB, me, desk.deviceId, `2026-10-02T09:00:00Z`)
      const tree = generateTreeKey()
      const publicKey = tree.publicKey
      const rA = await root(tx, { teamId: teamA, agentId: asA.id, userId: me, deviceId: desk.deviceId, shared: true, lastActivityAt: `2026-11-18T09:00:00Z`, key: { publicKey } })
      const rB = await root(tx, { teamId: teamB, agentId: asB.id, userId: me, deviceId: desk.deviceId, lastActivityAt: `2026-11-10T09:00:00Z` })
      for (const reader of [myPhone, myDesk, matePhone]) {
        await tx.insert(sessionKeys).values({ rootId: rA, epoch: 1, installId: reader.id, wrapped: wrapTreeKey(reader.x25519Public, tree.secret) })
      }
      const sealed = sealBody(publicKey, 1, `the plan for Friday`)
      await say(tx, rA, teamA, me, sealed)
      // Before: the teammate reads and steers it (shared on a team device).
      const [before] = await tx.select().from(sessions).where(eq(sessions.id, rA))
      expect(sessionVisibleTo(before!, mate)).toBe(true)
      expect([...(await treeReaders(tx, before!)).keys()]).toContain(matePhone.id)
      // References to the Assistant that goes.
      const [board] = await tx.insert(boards).values({ teamId: teamB, name: `B`, slug: `b-${id8()}`, prefix: `MV` }).returning({ id: boards.id })
      const [issue] = await tx
        .insert(issues)
        .values({ boardId: board!.id, teamId: teamB, title: `Mine`, creatorId: me, assigneeAgentId: asB.id, creatorAgentId: asB.id } as never)
        .returning({ id: issues.id })
      await tx.update(teams).set({ leadAssistantId: asB.id }).where(eq(teams.id, teamB))
      await tx.insert(userVoiceSettings).values({ userId: me, agentId: asB.id } as never).onConflictDoNothing()

      // The pick, as the TS reference reads it.
      const pick = assistantMoveOf(
        [asA, asB].map((a) => ({ id: a.id, ownerId: me, createdAt: a.createdAt.toISOString() })),
        [
          { id: rA, agentId: asA.id, lastActivityAt: `2026-11-18T09:00:00Z`, createdAt: `2026-11-01T09:00:00Z` },
          { id: rB, agentId: asB.id, lastActivityAt: `2026-11-10T09:00:00Z`, createdAt: `2026-11-01T09:00:00Z` },
        ]
      )
      expect(pick).toEqual([{ ownerId: me, keepAgentId: asA.id, keepRootId: rA, endedRootIds: [rB], dropAgentIds: [asB.id] }])

      await runMove(tx, [me])

      const [personal] = await tx.select().from(teams).where(and(eq(teams.ownerId, me), eq(teams.kind, `personal`)))
      expect(personal).toMatchObject({ slug: personalTeamSlug(me) })
      const mine = await tx.select().from(agents).where(and(eq(agents.ownerId, me), eq(agents.role, `assistant`)))
      expect(mine.map((a) => [a.id, a.teamId])).toEqual([[asA.id, personal!.id]])
      expect(mine[0]!.instructionsUpdatedAt!.getTime()).toBeGreaterThan(asA.instructionsUpdatedAt!.getTime())

      // The kept conversation: same id, personal team, private, same key.
      const [kept] = await tx.select().from(sessions).where(eq(sessions.id, rA))
      expect(kept).toMatchObject({ teamId: personal!.id, shared: false, agentId: asA.id, status: `running`, sealPublicKey: publicKey, sealEpoch: 1, sealRotateEpoch: 1 })
      expect(sessionVisibleTo(kept!, mate)).toBe(false)
      expect(sessionVisibleTo(kept!, me)).toBe(true)
      const readers = [...(await treeReaders(tx, kept!)).keys()].sort()
      expect(readers).toEqual([myPhone.id, myDesk.id].sort())
      const wraps = await tx.select().from(sessionKeys).where(eq(sessionKeys.rootId, rA))
      expect(wraps.map((w) => w.installId).sort()).toEqual([myPhone.id, myDesk.id].sort())
      // The stored history still opens with the member's own wrap.
      const [line] = await tx.select().from(messages).where(eq(messages.sessionId, rA))
      expect(line).toMatchObject({ teamId: personal!.id, shared: false, body: sealed })
      const secret = unwrapTreeKey(myPhone.secret, wraps.find((w) => w.installId === myPhone.id)!.wrapped)
      expect(openBody(line!.body, new Map([[1, secret]]))).toMatchObject({ status: `opened`, text: `the plan for Friday` })

      // The other conversation ended, detached, still in its team.
      const [other] = await tx.select().from(sessions).where(eq(sessions.id, rB))
      expect(other).toMatchObject({ teamId: teamB, status: `ended`, agentId: null })
      // References moved to the kept one.
      const [moved] = await tx.select().from(issues).where(eq(issues.id, issue!.id))
      expect(moved).toMatchObject({ assigneeAgentId: asA.id, creatorAgentId: asA.id })
      const [lead] = await tx.select({ lead: teams.leadAssistantId }).from(teams).where(eq(teams.id, teamB))
      expect(lead!.lead).toBe(asA.id)
      const [voice] = await tx.select().from(userVoiceSettings).where(eq(userVoiceSettings.userId, me))
      expect(voice!.agentId).toBe(asA.id)
      // No scratch table survives; a second run changes nothing.
      const scratch = await tx.execute(sql`SELECT count(*)::int AS n FROM pg_tables WHERE tablename LIKE 'exp1986_%' OR tablename LIKE 'exp1988_%'`)
      expect((scratch as unknown as { rows: Array<{ n: number }> }).rows?.[0]?.n ?? (scratch as unknown as Array<{ n: number }>)[0]?.n).toBe(0)
      await runMove(tx, [me])
      const [again] = await tx.select().from(sessions).where(eq(sessions.id, rA))
      expect(again).toMatchObject({ teamId: personal!.id, status: `running` })
    })
  }, 60_000)

  it(`replays keepCases on SQL rows (no conversation = the newest row; ties to the lower id)`, async () => {
    for (const c of ASSISTANT_MOVE.keepCases) {
      const owners = new Map<string, string>()
      for (const a of c.assistants) if (!owners.has(a.ownerId)) owners.set(a.ownerId, await user())
      const deskOf = new Map<string, string>()
      for (const [, real] of owners) deskOf.set(real, (await device(real)).deviceId)
      const teamOf = new Map<string, string>()
      for (const a of c.assistants) teamOf.set(a.id, await team([[owners.get(a.ownerId)!, `owner`]]))
      await inRollback(async (tx) => {
        // Fixture ids sort like real uuids: map each to a uuid of the same order.
        const ids = [...new Set([...c.assistants.map((a) => a.id), ...c.roots.map((r) => r.id)])].sort()
        const uuidOf = new Map(ids.map((fid, i) => [fid, `00000000-0000-4000-8${String(i).padStart(3, `0`)}-${id8()}${id8().slice(0, 4)}`]))
        for (const a of c.assistants) {
          const real = owners.get(a.ownerId)!
          const t = teamOf.get(a.id)!
          await tx.insert(agents).values({
            id: uuidOf.get(a.id)!,
            teamId: t,
            ownerId: real,
            deviceId: deskOf.get(real)!,
            name: `Assistant`,
            role: `assistant`,
            createdAt: new Date(a.createdAt),
          } as never)
        }
        for (const r of c.roots) {
          const owner = owners.get(c.assistants.find((a) => a.id === r.agentId)!.ownerId)!
          const sid = uuidOf.get(r.id)!
          await tx.insert(sessions).values({
            id: sid,
            rootId: sid,
            teamId: teamOf.get(r.agentId)!,
            agentId: uuidOf.get(r.agentId)!,
            userId: owner,
            deviceId: deskOf.get(owner)!,
            status: `running`,
            lastActivityAt: r.lastActivityAt ? new Date(r.lastActivityAt) : null,
            createdAt: new Date(r.createdAt),
          } as never)
        }
        await runMove(tx, [...owners.values()])
        for (const expected of c.expected) {
          const real = owners.get(expected.ownerId)!
          const kept = await tx.select({ id: agents.id }).from(agents).where(and(eq(agents.ownerId, real), eq(agents.role, `assistant`)))
          expect(kept.map((a) => a.id), c.name).toEqual([uuidOf.get(expected.keepAgentId)])
          if (expected.keepRootId) {
            const [r] = await tx.select().from(sessions).where(eq(sessions.id, uuidOf.get(expected.keepRootId)!))
            expect(r!.status, c.name).toBe(`running`)
          }
          for (const ended of expected.endedRootIds) {
            const [r] = await tx.select().from(sessions).where(eq(sessions.id, uuidOf.get(ended)!))
            expect(r, c.name).toMatchObject({ status: `ended`, agentId: null })
          }
        }
      })
    }
  }, 120_000)

  it(`a NEW Assistant conversation on a device shared with the team is private (the hole stays closed)`, async () => {
    process.env.ASSISTANT_MOVE = `on`
    onTestFinished(() => {
      delete process.env.ASSISTANT_MOVE
    })
    const me = await user()
    const mate = await user()
    const teamA = await team([[me, `owner`], [mate, `member`]])
    await device(me, [teamA])
    const agent = (await ensureAssistant(me, teamA))!
    const [personal] = await db.select().from(teams).where(eq(teams.id, agent.teamId))
    expect(personal).toMatchObject({ kind: `personal`, ownerId: me })
    await inRollback(async (tx) => {
      await conversationInTx(tx as never, agent, me)
      const [convo] = await tx.select().from(sessions).where(and(eq(sessions.agentId, agent.id), eq(sessions.userId, me)))
      expect(convo).toMatchObject({ teamId: personal!.id, shared: false })
      expect(sessionVisibleTo(convo!, mate)).toBe(false)
    })
    // A message from inside team A (what every client sends) lands in that
    // private conversation; the teammate can neither read nor write it.
    const sent = await sendSession({ userId: me, input: { agentId: agent.id, teamId: teamA, text: `What is running today?` } as never })
    const [convo] = await db.select().from(sessions).where(eq(sessions.id, sent.sessionId))
    expect(convo).toMatchObject({ agentId: agent.id, teamId: personal!.id, shared: false, userId: me })
    const lines = await db.select().from(messages).where(eq(messages.sessionId, convo!.id))
    expect(lines.map((line) => [line.body, line.teamId, line.shared])).toEqual([[`What is running today?`, personal!.id, false]])
    await expect(loadSessionForUser(mate, convo!.id)).rejects.toMatchObject({ code: `NOT_FOUND` })
    await expect(
      sendSession({ userId: mate, input: { sessionId: convo!.id, text: `steer` } as never })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
    await expect(
      sendSession({ userId: mate, input: { agentId: agent.id, teamId: teamA, text: `hi` } as never })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
  }, 60_000)
  it(`#EXP-1988: every team's skill and MCP picks survive the move, and each team's work launches with its own`, async () => {
    const me = await user()
    const teamA = await team([[me, `owner`]])
    const teamB = await team([[me, `owner`]])
    const desk = await device(me)
    await inRollback(async (tx) => {
      const skill = async (teamId: string, name: string) =>
        (await tx.insert(skills).values({ teamId, name, description: name, body: `# ${name}` }).returning({ id: skills.id }))[0]!.id
      const server = async (teamId: string, name: string) =>
        (await tx.insert(mcpServers).values({ teamId, name, url: `https://${name.toLowerCase()}.example/mcp` }).returning({ id: mcpServers.id }))[0]!.id
      const [skA, skB, skB2] = [await skill(teamA, `alpha`), await skill(teamB, `beta`), await skill(teamB, `gamma`)]
      const [mcpA, mcpB] = [await server(teamA, `Linear`), await server(teamB, `Linear`)]
      const asA = await assistant(tx, teamA, me, desk.deviceId, `2026-10-01T09:00:00Z`)
      const asB = await assistant(tx, teamB, me, desk.deviceId, `2026-10-02T09:00:00Z`)
      await tx.update(agents).set({ skillIds: [skA], mcpServerIds: [mcpA] }).where(eq(agents.id, asA.id))
      await tx.update(agents).set({ skillIds: [skB, skB2], mcpServerIds: [mcpB] }).where(eq(agents.id, asB.id))
      const rA = await root(tx, { teamId: teamA, agentId: asA.id, userId: me, deviceId: desk.deviceId, lastActivityAt: `2026-11-18T09:00:00Z` })
      const rB = await root(tx, { teamId: teamB, agentId: asB.id, userId: me, deviceId: desk.deviceId, lastActivityAt: `2026-11-10T09:00:00Z` })
      await tx.update(sessions).set({ skillIds: [skA], mcpServerIds: [mcpA] }).where(eq(sessions.id, rA))
      await tx.update(sessions).set({ skillIds: [skB, skB2], mcpServerIds: [mcpB] }).where(eq(sessions.id, rB))

      // (a) switch off: each team's conversation launches with its team's picks.
      const before = await tx.select().from(sessions).where(inArray(sessions.id, [rA, rB]))
      const byId = new Map(before.map((row) => [row.id, row]))
      expect(await sessionLaunchPick(tx, `skills`, byId.get(rA)!, byId.get(rA)!.skillIds)).toEqual([skA])
      expect(await sessionLaunchPick(tx, `mcp`, byId.get(rB)!, byId.get(rB)!.mcpServerIds)).toEqual([mcpB])

      await runMove(tx, [me])

      // (b) the kept Assistant holds both teams' picks, kept list first.
      const [kept] = await tx.select().from(agents).where(and(eq(agents.ownerId, me), eq(agents.role, `assistant`)))
      expect(kept).toMatchObject({ id: asA.id, skillIds: [skA, skB, skB2], mcpServerIds: [mcpA, mcpB] })
      const [keptRoot] = await tx.select().from(sessions).where(eq(sessions.id, rA))
      expect(keptRoot).toMatchObject({ skillIds: [skA, skB, skB2], mcpServerIds: [mcpA, mcpB] })
      // (c) the conversation's turn (nothing picked = every team) launches
      // with both teams' picks; a pick of team B only with B's.
      expect((await sessionLaunchTeams(tx, keptRoot!)).sort()).toEqual([kept!.teamId, teamA, teamB].sort())
      expect(await sessionLaunchPick(tx, `skills`, keptRoot!, keptRoot!.skillIds)).toEqual([skA, skB, skB2])
      expect(await sessionLaunchPick(tx, `mcp`, keptRoot!, keptRoot!.mcpServerIds)).toEqual([mcpA, mcpB])
      await tx.update(sessions).set({ scopeTeamIds: [teamB] }).where(eq(sessions.id, rA))
      const [scoped] = await tx.select().from(sessions).where(eq(sessions.id, rA))
      expect(await sessionLaunchPick(tx, `skills`, scoped!, scoped!.skillIds)).toEqual([skB, skB2])
      expect(await sessionLaunchPick(tx, `mcp`, scoped!, scoped!.mcpServerIds)).toEqual([mcpB])
      // A child of the tree in team A: A's picks only.
      const child = { teamId: teamA, rootId: rA }
      expect(await sessionLaunchPick(tx, `mcp`, child, [mcpA, mcpB])).toEqual([mcpA])
      // A section's write replaces only its team's picks.
      expect(await mergedAgentPick(tx, `skills`, { existing: kept!.skillIds, written: [skB2], pickTeamId: teamB, agentTeamId: kept!.teamId })).toEqual([skA, skB2])
      // Nothing left over; a second run changes nothing.
      await runMove(tx, [me])
      const [again] = await tx.select().from(agents).where(eq(agents.id, asA.id))
      expect(again).toMatchObject({ skillIds: [skA, skB, skB2], mcpServerIds: [mcpA, mcpB] })
    })
  }, 60_000)
})
