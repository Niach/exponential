// @vitest-environment node
// #EXP-1989 (agent-settings.json `assistant.move`, #EXP-1984 phase 5): THE
// TRAP. An install that never set the old `ASSISTANT_MOVE` switch still
// holds one Assistant per (team, member) when it updates, and drizzle runs
// the migrations before anything of the server boots: the migration `exp1989_one_assistant_per_owner` must
// move them itself before it builds the index on owner. So this suite runs
// the journal THE WAY AN INSTALL DOES, through drizzle's migrator, on
// databases of its own (created beside DATABASE_URL's, dropped after):
//   * a database at 0292 with its custom triggers, holding a member with
//     per-team Assistants in two teams (different picks, a keyed shared
//     conversation), a member who already moved and one without an Assistant:
//     the update moves the first, leaves the others, swaps the index;
//   * the migrator again: nothing pending, nothing changes;
//   * an empty database at 0292: only the index changes;
//   * the index refuses a second Assistant row for one owner.
// The move's own cases (`keepCases`, the references, the launch picks):
// assistant-move.real-db.test.ts, on the same file.
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import { afterAll, describe, expect, it } from "vitest"
import { Pool } from "pg"
import { and, eq, sql } from "drizzle-orm"
import { drizzle } from "drizzle-orm/node-postgres"
import { migrate } from "drizzle-orm/node-postgres/migrator"
import {
  b64urlEncode,
  generateTreeKey,
  openBody,
  pushPublicKey,
  sealBody,
  unwrapTreeKey,
  wrapTreeKey,
} from "@exp/domain-contract/sealed-content"
import {
  agents,
  devices,
  installs,
  mcpServers,
  messages,
  sessionKeys,
  sessions,
  skills,
  teamMembers,
  teams,
} from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

const OUT = join(dirname(fileURLToPath(import.meta.url)), `../../db/out`)
// Found by name: its number moves when another migration lands first.
const TAG = (JSON.parse(readFileSync(join(OUT, `meta`, `_journal.json`), `utf8`)) as { entries: Array<{ tag: string }> }).entries
  .map((entry) => entry.tag)
  .find((tag) => tag.endsWith(`_exp1989_one_assistant_per_owner`))!

// Its own databases: the role must be allowed to create them.
const ready = await probeRealDb(`
  SELECT (rolcreatedb OR rolsuper) AS ok FROM pg_roles WHERE rolname = current_user
`)
if (!ready.ok) console.info(`[assistant-move-migration.real-db] skipped: ${ready.reason}`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const run = id8()
const names = { moved: `exp1989_${run}_moved`, empty: `exp1989_${run}_empty` }
const urlOf = (database: string) => {
  const url = new URL(process.env.DATABASE_URL!)
  url.pathname = `/${database}`
  return url.toString()
}
const admin = async (statement: string) => {
  const pool = new Pool({ connectionString: process.env.DATABASE_URL, max: 1 })
  try {
    await pool.query(statement)
  } finally {
    await pool.end()
  }
}
const scratch: string[] = []

/** `fn` on a connection of its own to `database`, closed after (a template
 *  database must have no session left). */
async function on<T>(database: string, fn: (db: ReturnType<typeof drizzle>, pool: Pool) => Promise<T>): Promise<T> {
  const pool = new Pool({ connectionString: urlOf(database), max: 2 })
  try {
    return await fn(drizzle({ client: pool, casing: `snake_case` }), pool)
  } finally {
    await pool.end()
  }
}

/** The journal up to (not including) 0293: the release before this one. */
function folderBefore(): string {
  const folder = mkdtempSync(join(tmpdir(), `exp1989-`))
  scratch.push(folder)
  cpSync(OUT, folder, { recursive: true })
  const journalPath = join(folder, `meta`, `_journal.json`)
  const journal = JSON.parse(readFileSync(journalPath, `utf8`)) as { entries: Array<{ tag: string }> }
  const at = journal.entries.findIndex((entry) => entry.tag === TAG)
  expect(at).toBeGreaterThan(0)
  journal.entries = journal.entries.slice(0, at)
  writeFileSync(journalPath, JSON.stringify(journal))
  return folder
}

const indexDef = async (pool: Pool) =>
  (await pool.query<{ indexdef: string }>(`SELECT indexdef FROM pg_indexes WHERE indexname = 'uniq_agents_assistant'`)).rows[0]
    ?.indexdef ?? ``
const TEAM_OWNER_INDEX = /USING btree \(team_id, owner_id\) WHERE \(role = 'assistant'/
const OWNER_INDEX = /USING btree \(owner_id\) WHERE \(role = 'assistant'/
const leftovers = async (pool: Pool) =>
  (
    await pool.query<{ n: number }>(
      `SELECT count(*)::int AS n FROM pg_tables WHERE tablename LIKE 'exp1986_%' OR tablename LIKE 'exp1988_%'`
    )
  ).rows[0]!.n
const applied = async (pool: Pool) =>
  (await pool.query<{ n: number }>(`SELECT count(*)::int AS n FROM drizzle.__drizzle_migrations`)).rows[0]!.n

afterAll(async () => {
  for (const folder of scratch) rmSync(folder, { recursive: true, force: true })
  if (!ready.ok) return
  for (const database of Object.values(names)) await admin(`DROP DATABASE IF EXISTS "${database}" WITH (FORCE)`)
  // Dropping two whole databases outlasts the default 10 s hook on a busy host.
}, 120_000)

describe.skipIf(!ready.ok)(`the one-Assistant-per-owner migration through drizzle's migrator (#EXP-1989)`, () => {
  it(`moves an install that never set the switch, leaves who already moved, then holds one Assistant per owner`, async () => {
    const journal = (JSON.parse(readFileSync(join(OUT, `meta`, `_journal.json`), `utf8`)) as { entries: Array<{ tag: string }> }).entries
    const total = journal.length
    // The release before holds every migration ahead of this one (later ones may follow it).
    const before = journal.findIndex((entry) => entry.tag === TAG)
    await admin(`CREATE DATABASE "${names.moved}"`)
    // The release before: every migration up to 0292, then its boot's triggers.
    await on(names.moved, async (db, pool) => {
      await migrate(db, { migrationsFolder: folderBefore() })
      await pool.query(readFileSync(join(OUT, `custom`, `0001_triggers.sql`), `utf8`))
      expect(await applied(pool)).toBe(before)
      expect(await indexDef(pool)).toMatch(TEAM_OWNER_INDEX)
    })
    // An empty install of that release, for the second case.
    await admin(`CREATE DATABASE "${names.empty}" TEMPLATE "${names.moved}"`)

    await on(names.moved, async (db, pool) => {
      const user = async (name: string) => {
        const id = `mig-${name}-${id8()}`
        await pool.query(
          `INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at) VALUES ($1, 'Tester', $2, true, false, now(), now())`,
          [id, `${id}@move.test`]
        )
        return id
      }
      const team = async (members: string[]) => {
        const [row] = await db.insert(teams).values({ name: `Move`, slug: `move-${id8()}` }).returning({ id: teams.id })
        await db.insert(teamMembers).values(members.map((userId, i) => ({ teamId: row!.id, userId, role: i ? `member` : `owner` })) as never)
        return row!.id
      }
      const install = async (userId: string) => {
        const secret = crypto.getRandomValues(new Uint8Array(32))
        const x25519Public = b64urlEncode(pushPublicKey(secret))
        const [row] = await db
          .insert(installs)
          .values({ userId, kind: `phone`, endpointId: `ep-${id8()}${id8()}`, x25519Public } as never)
          .returning({ id: installs.id })
        return { id: row!.id, secret, x25519Public }
      }
      const assistant = async (teamId: string, ownerId: string, deviceId: string, createdAt: string, picks: { skillIds: string[]; mcpServerIds: string[] }) =>
        (
          await db
            .insert(agents)
            .values({ teamId, ownerId, deviceId, name: `Assistant`, role: `assistant`, createdAt: new Date(createdAt), instructionsUpdatedAt: new Date(createdAt), ...picks } as never)
            .returning()
        )[0]!
      const root = async (input: { teamId: string; agentId: string; userId: string; deviceId: string; shared?: boolean; lastActivityAt: string; publicKey?: string; skillIds?: string[]; mcpServerIds?: string[] }) => {
        const id = crypto.randomUUID()
        await db.insert(sessions).values({
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
          skillIds: input.skillIds ?? [],
          mcpServerIds: input.mcpServerIds ?? [],
          ...(input.publicKey ? { sealPublicKey: input.publicKey, sealEpoch: 1 } : {}),
        } as never)
        return id
      }
      const say = (sessionId: string, teamId: string, authorId: string, body: string, shared: boolean) =>
        db.insert(messages).values({
          conversationId: sql`(SELECT id FROM conversations WHERE session_id = ${sessionId})`,
          sessionId,
          rootId: sessionId,
          teamId,
          authorKind: `user`,
          authorId,
          body,
          shared,
        } as never)
      const skill = async (teamId: string, name: string) =>
        (await db.insert(skills).values({ teamId, name, description: name, body: `# ${name}` }).returning({ id: skills.id }))[0]!.id
      const server = async (teamId: string, name: string) =>
        (await db.insert(mcpServers).values({ teamId, name, url: `https://${name.toLowerCase()}.example/mcp` }).returning({ id: mcpServers.id }))[0]!.id

      // (1) Never switched on: one Assistant per team, different picks, the
      // more recent conversation keyed and shared with team A (its device is).
      const me = await user(`me`)
      const mate = await user(`mate`)
      const teamA = await team([me, mate])
      const teamB = await team([me])
      const deviceId = `dev-${id8()}`
      await db.insert(devices).values({ userId: me, deviceId, label: `desk`, kind: `desktop`, lastSeenAt: new Date(), sharedTeamIds: [teamA] } as never)
      const myPhone = await install(me)
      const matePhone = await install(mate)
      const [skA, skB, skB2] = [await skill(teamA, `alpha`), await skill(teamB, `beta`), await skill(teamB, `gamma`)]
      const [mcpA, mcpB] = [await server(teamA, `Linear`), await server(teamB, `Linear`)]
      const asA = await assistant(teamA, me, deviceId, `2026-10-01T09:00:00Z`, { skillIds: [skA], mcpServerIds: [mcpA] })
      const asB = await assistant(teamB, me, deviceId, `2026-10-02T09:00:00Z`, { skillIds: [skB, skB2], mcpServerIds: [mcpB] })
      const tree = generateTreeKey()
      const rA = await root({ teamId: teamA, agentId: asA.id, userId: me, deviceId, shared: true, lastActivityAt: `2026-11-18T09:00:00Z`, publicKey: tree.publicKey, skillIds: [skA], mcpServerIds: [mcpA] })
      const rB = await root({ teamId: teamB, agentId: asB.id, userId: me, deviceId, lastActivityAt: `2026-11-10T09:00:00Z`, skillIds: [skB, skB2], mcpServerIds: [mcpB] })
      for (const reader of [myPhone, matePhone]) {
        // Raw: the schema has a column this release did not (0299).
        await pool.query(`INSERT INTO session_keys (root_id, epoch, install_id, wrapped) VALUES ($1, 1, $2, $3)`, [
          rA,
          reader.id,
          wrapTreeKey(reader.x25519Public, tree.secret),
        ])
      }
      const sealed = sealBody(tree.publicKey, 1, `the plan for Friday`)
      await say(rA, teamA, me, sealed, true)
      await say(rB, teamB, me, `the other team's chat`, false)

      // (2) Already moved (the switch was on): the ONE, in their personal team.
      const solo = await user(`solo`)
      const soloTeam = await team([solo])
      const [soloHome] = await db.insert(teams).values({ name: `Personal`, slug: `personal-${id8()}${id8()}`, kind: `personal`, ownerId: solo } as never).returning({ id: teams.id })
      const soloSkill = await skill(soloTeam, `delta`)
      const asSolo = await assistant(soloHome!.id, solo, `dev-${id8()}`, `2026-10-03T09:00:00Z`, { skillIds: [soloSkill], mcpServerIds: [] })
      const rSolo = await root({ teamId: soloHome!.id, agentId: asSolo.id, userId: solo, deviceId: asSolo.deviceId, lastActivityAt: `2026-11-19T09:00:00Z`, skillIds: [soloSkill] })
      await say(rSolo, soloHome!.id, solo, `mine alone`, false)
      // (3) A member without an Assistant, and a team agent of theirs.
      const plain = await user(`plain`)
      const plainTeam = await team([plain])
      await db.insert(agents).values({ teamId: plainTeam, ownerId: plain, deviceId: `dev-${id8()}`, name: `Triage`, role: `agent` } as never)

      const untouched = async () => ({
        agents: await db.select().from(agents).where(eq(agents.ownerId, solo)),
        teams: await db.select().from(teams).where(eq(teams.ownerId, solo)),
        sessions: await db.select().from(sessions).where(eq(sessions.userId, solo)),
        messages: await db.select().from(messages).where(eq(messages.sessionId, rSolo)),
        plain: await db.select().from(agents).where(eq(agents.ownerId, plain)),
        plainHome: await db.select().from(teams).where(and(eq(teams.ownerId, plain), eq(teams.kind, `personal`))),
      })
      const before = await untouched()
      expect(before.plainHome).toEqual([])

      // THE UPDATE: drizzle applies 0293 (the move, then the index).
      await migrate(db, { migrationsFolder: OUT })
      expect(await applied(pool)).toBe(total)
      expect(await indexDef(pool)).toMatch(OWNER_INDEX)
      expect(await leftovers(pool)).toBe(0)

      // (1) One Assistant, in the personal team, with both teams' picks.
      const [home] = await db.select().from(teams).where(and(eq(teams.ownerId, me), eq(teams.kind, `personal`)))
      expect(home).toBeDefined()
      const mine = await db.select().from(agents).where(and(eq(agents.ownerId, me), eq(agents.role, `assistant`)))
      expect(mine).toHaveLength(1)
      expect(mine[0]).toMatchObject({ id: asA.id, teamId: home!.id, skillIds: [skA, skB, skB2], mcpServerIds: [mcpA, mcpB] })
      // The prompt stamp stays: the move puts no Instructions changed card on the moved chat.
      expect(mine[0]!.instructionsUpdatedAt).toEqual(asA.instructionsUpdatedAt)
      // The keyed conversation moved with its ids, private, still readable by
      // its member's own wrap; the teammate's wrap is gone.
      const [kept] = await db.select().from(sessions).where(eq(sessions.id, rA))
      expect(kept).toMatchObject({ teamId: home!.id, shared: false, agentId: asA.id, status: `running`, sealPublicKey: tree.publicKey, sealEpoch: 1, sealRotateEpoch: 1, skillIds: [skA, skB, skB2], mcpServerIds: [mcpA, mcpB] })
      const wraps = await db.select().from(sessionKeys).where(eq(sessionKeys.rootId, rA))
      expect(wraps.map((w) => w.installId)).toEqual([myPhone.id])
      const [line] = await db.select().from(messages).where(eq(messages.sessionId, rA))
      expect(line).toMatchObject({ teamId: home!.id, shared: false, body: sealed })
      expect(openBody(line!.body, new Map([[1, unwrapTreeKey(myPhone.secret, wraps[0]!.wrapped)]]))).toMatchObject({ status: `opened`, text: `the plan for Friday` })
      // The other conversation ended, detached, readable in its team.
      const [other] = await db.select().from(sessions).where(eq(sessions.id, rB))
      expect(other).toMatchObject({ teamId: teamB, status: `ended`, agentId: null })
      expect((await db.select().from(messages).where(eq(messages.sessionId, rB))).map((m) => [m.body, m.teamId])).toEqual([[`the other team's chat`, teamB]])
      // (2) + (3): nothing of theirs changed.
      expect(await untouched()).toEqual(before)

      // The index: no second Assistant for one owner, in any team.
      for (const teamId of [teamA, home!.id, soloTeam]) {
        const owner = teamId === soloTeam ? solo : me
        await expect(assistant(teamId, owner, deviceId, `2026-11-20T09:00:00Z`, { skillIds: [], mcpServerIds: [] })).rejects.toMatchObject({
          cause: { code: `23505`, constraint: `uniq_agents_assistant` },
        })
      }
      // Someone's first Assistant still fits.
      const first = await assistant(plainTeam, plain, `dev-${id8()}`, `2026-11-20T09:00:00Z`, { skillIds: [], mcpServerIds: [] })
      expect(first.role).toBe(`assistant`)

      // The migrator again (the next boot): nothing pending, nothing changes.
      const after = { mine: await db.select().from(agents).where(eq(agents.ownerId, me)), kept: await db.select().from(sessions).where(eq(sessions.userId, me)) }
      await migrate(db, { migrationsFolder: OUT })
      expect(await applied(pool)).toBe(total)
      expect({ mine: await db.select().from(agents).where(eq(agents.ownerId, me)), kept: await db.select().from(sessions).where(eq(sessions.userId, me)) }).toEqual(after)
    })
  }, 300_000)

  it(`an empty install at 0292: only the index changes`, async () => {
    await on(names.empty, async (db, pool) => {
      expect(await indexDef(pool)).toMatch(TEAM_OWNER_INDEX)
      await migrate(db, { migrationsFolder: OUT })
      expect(await indexDef(pool)).toMatch(OWNER_INDEX)
      expect(await leftovers(pool)).toBe(0)
      const counts = await pool.query<{ agents: number; teams: number }>(
        `SELECT (SELECT count(*)::int FROM agents) AS agents, (SELECT count(*)::int FROM teams) AS teams`
      )
      expect(counts.rows[0]).toEqual({ agents: 0, teams: 0 })
    })
  }, 120_000)
})
