// @vitest-environment node
import { readFileSync } from "node:fs"
import { describe, expect, it } from "vitest"
import { eq, inArray, sql } from "drizzle-orm"
import { boards, codingSessions, issues, teams, threadItems, threads } from "@/db/schema"
import { inRollback, probeRealDb, type RealDbTx } from "@/test/real-db"

// EXP-1284 (review T16): the data halves of migrations 0166 (every resume
// chain becomes ONE thread with its issues and its newest Guide) and 0168
// (run items take their session's board context, emptied threads go, the
// hidden flag is computed), replayed against the REAL migrated Postgres
// inside a rolled-back transaction. 0166's backfill runs over EVERY session
// in the database, so the case first clears the existing threads (rolled
// back with the rest) and asserts on its own seeded rows only.

const ready = await probeRealDb(`
  SELECT (
    EXISTS (SELECT 1 FROM information_schema.columns
      WHERE table_name = 'threads' AND column_name = 'has_visible_items')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'refresh_thread_visibility')
    AND current_setting('is_superuser') = 'on'
  ) AS ok
`)
if (!ready.ok) console.info(`[threads-backfill.real-db] skipped: ${ready.reason}`)

const migration = (name: string) =>
  readFileSync(new URL(`../db/out/${name}`, import.meta.url), `utf8`)
    .split(`--> statement-breakpoint`)
    .map((statement) => statement.trim())
    .filter(Boolean)

/** 0166's backfill = everything from the temp chain table on (the DDL above
 *  it is already applied). */
const backfill0166 = () => {
  const statements = migration(`0166_threads.sql`)
  const from = statements.findIndex((statement) => statement.includes(`CREATE TEMP TABLE exp1284_chain`))
  return statements.slice(from)
}
/** 0168's backfill = everything after its ADD COLUMN. */
const backfill0168 = () =>
  migration(`0168_threads_visibility.sql`).filter((statement) => !statement.startsWith(`ALTER TABLE`))

const id8 = () => crypto.randomUUID().slice(0, 8)

async function seedTeam(tx: RealDbTx, userId: string) {
  const [team] = await tx
    .insert(teams)
    .values({ name: `Backfill`, slug: `backfill-${id8()}` })
    .returning({ id: teams.id })
  const [board] = await tx
    .insert(boards)
    .values({ teamId: team!.id, name: `B`, slug: `b-${id8()}`, prefix: `BF` })
    .returning({ id: boards.id })
  const issue = async () =>
    (
      await tx
        .insert(issues)
        .values({ boardId: board!.id, teamId: team!.id, title: `Issue ${id8()}` })
        .returning({ id: issues.id })
    )[0]!.id
  const session = async (over: Partial<typeof codingSessions.$inferInsert> = {}) =>
    (
      await tx
        .insert(codingSessions)
        .values({ teamId: team!.id, userId, status: `ended`, ...over })
        .returning({ id: codingSessions.id })
    )[0]!.id
  return { teamId: team!.id, boardId: board!.id, issue, session }
}

async function seedUser(tx: RealDbTx) {
  const userId = `backfill-user-${id8()}`
  await tx.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${userId}, 'Backfill', ${`${userId}@threads.test`}, true, false, now(), now())
  `)
  return userId
}

describe.skipIf(!ready.ok)(`migration 0166 backfill (EXP-1284)`, () => {
  it(`one thread per resume chain, its covered issues, the newest member's Guide`, () =>
    inRollback(async (tx) => {
      const userId = await seedUser(tx)
      const team = await seedTeam(tx, userId)
      const other = await seedTeam(tx, userId)
      const [i1, i2, foreign] = [await team.issue(), await team.issue(), await other.issue()]
      const guide = [{ topic: `Summary`, label: null, attachmentId: null, width: null, height: null, text: `newest` }]
      const root = await team.session({ issueId: i1, createdAt: new Date(`2026-09-01T00:00:00Z`) })
      const resumed = await team.session({
        issueId: i1,
        resumedFromId: root,
        results: guide,
        createdAt: new Date(`2026-09-02T00:00:00Z`),
      })
      const batch = await team.session({
        // Valid, duplicated, missing, another team's, malformed.
        batchIssueIds: [i2, i2, crypto.randomUUID(), foreign, `not-a-uuid`],
        createdAt: new Date(`2026-09-03T00:00:00Z`),
      })

      await tx.delete(threads)
      for (const statement of backfill0166()) await tx.execute(sql.raw(statement))

      const chainThread = await tx.select().from(threads).where(eq(threads.id, root))
      expect(chainThread).toHaveLength(1)
      expect(chainThread[0]).toMatchObject({ teamId: team.teamId, createdBy: userId, results: guide })
      const chainItems = await tx.select().from(threadItems).where(eq(threadItems.threadId, root))
      expect(chainItems.filter((item) => item.kind === `run`).map((item) => item.sessionId).sort()).toEqual(
        [root, resumed].sort()
      )
      expect(chainItems.filter((item) => item.kind === `issue`).map((item) => item.issueId)).toEqual([i1])

      const batchItems = await tx.select().from(threadItems).where(eq(threadItems.threadId, batch))
      expect(batchItems.filter((item) => item.kind === `issue`).map((item) => item.issueId)).toEqual([i2])
      expect((await tx.select().from(threads).where(eq(threads.id, batch)))[0]!.results).toBeNull()

      const runItems = await tx
        .select()
        .from(threadItems)
        .where(inArray(threadItems.sessionId, [root, resumed, batch]))
      expect(runItems).toHaveLength(3)
    }))
})

describe.skipIf(!ready.ok)(`migration 0168 backfill (EXP-1284 review T3)`, () => {
  it(`run items take their session's board, emptied threads go, hidden ones are flagged`, () =>
    inRollback(async (tx) => {
      const userId = await seedUser(tx)
      const team = await seedTeam(tx, userId)
      const issueId = await team.issue()
      const onBoard = await team.session({ issueId })
      const thread = async (results: unknown = null) =>
        (
          await tx
            .insert(threads)
            .values({ teamId: team.teamId, results: results as never })
            .returning({ id: threads.id })
        )[0]!.id

      const live = await thread()
      const emptyNoGuide = await thread()
      const emptyWithGuide = await thread([{ topic: `Summary`, text: `kept` }])
      const trashedOnly = await thread()

      // The pre-0168 state, written with triggers off (0166 left run items
      // without a board and nothing kept the flag).
      await tx.execute(sql`SET LOCAL session_replication_role = replica`)
      await tx.insert(threadItems).values([
        { threadId: live, kind: `run`, sessionId: onBoard, teamId: team.teamId },
        {
          threadId: trashedOnly,
          kind: `issue`,
          issueId,
          teamId: team.teamId,
          boardId: team.boardId,
          boardDeletedAt: new Date(),
        },
      ])
      await tx.execute(sql`SET LOCAL session_replication_role = origin`)

      for (const statement of backfill0168()) await tx.execute(sql.raw(statement))

      const [item] = await tx.select().from(threadItems).where(eq(threadItems.sessionId, onBoard))
      expect(item!.boardId).toBe(team.boardId)
      const rows = await tx
        .select({ id: threads.id, hasVisibleItems: threads.hasVisibleItems })
        .from(threads)
        .where(inArray(threads.id, [live, emptyNoGuide, emptyWithGuide, trashedOnly]))
      const byId = new Map(rows.map((row) => [row.id, row.hasVisibleItems]))
      expect(byId.get(live)).toBe(true)
      expect(byId.has(emptyNoGuide)).toBe(false)
      expect(byId.get(emptyWithGuide)).toBe(false)
      expect(byId.get(trashedOnly)).toBe(false)
    }))
})
