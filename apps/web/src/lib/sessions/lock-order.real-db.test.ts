// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { Pool } from "pg"
import { db } from "@/db/connection"
import { boards, devices, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb, retryingConflict } from "@/test/real-db"

// #ERR-5 (prod 2026-11-11, `deadlock detected` on `insert into messages`
// from `sessions.reply`): a child's reply writes its OWN rows (its message,
// its conversation's digest, its `sessions` row) and then its PARENT's (the
// bus copy, the parent's digest), while the parent's End with a summary
// wrote the PARENT's rows first and then each child's (`markEnded`).
// Row locks taken in opposite orders: Postgres killed one of them.
// The rule now (`lockSessionRows`): a write that touches several sessions
// row-locks them UP FRONT, ancestors first (depth, then id).
//
// The interleaving is forced, not hoped for: a test-only trigger holds the
// reply's bus copy (the row it writes into the parent) on an advisory lock
// until the parent's End is waiting on a row lock; then it lets go.

vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  notifySessionAsk: async () => {},
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: false }),
}))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'mirror_conversation_digest') AS ok
`)
if (!ready.ok) console.info(`[lock-order.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { endSession } = await import(`@/lib/sessions/lifecycle`)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }
const ROUNDS = Number(process.env.LOCK_ORDER_ROUNDS ?? 3)

// The gate's own connection: it holds the advisory lock across statements.
const gatePool = ready.ok ? new Pool({ connectionString: process.env.DATABASE_URL, max: 3 }) : null

afterAll(async () => {
  if (!ready.ok) return
  await gatePool?.end()
  if (made.teams.length) {
    await retryingConflict(() => db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`))
  }
  if (made.users.length) {
    await retryingConflict(() => db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`))
  }
})

const caller = (userId: string) =>
  sessionsRouter.createCaller({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) } as never)

async function world() {
  const owner = `lock-order-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${owner}, 'Tester', ${`${owner}@lock.test`}, true, false, now(), now())
  `)
  made.users.push(owner)
  const [team] = await db.insert(teams).values({ name: `Lock`, slug: `lock-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `LO` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const host = caller(owner)
  const rootId = (await sendSession({ userId: owner, input: { teamId, deviceId, text: `root` } })).sessionId
  const parentId = (await sendSession({ userId: owner, input: { parentId: rootId, text: `parent` } })).sessionId
  const childId = (await sendSession({ userId: owner, input: { parentId, text: `child` } })).sessionId
  for (const id of [rootId, parentId, childId]) await host.claim({ sessionId: id, deviceId })
  return { owner, host, rootId, parentId, childId }
}

const deadlocked = (e: unknown) => {
  const err = e as { code?: string; cause?: { code?: string; cause?: { code?: string } } }
  return (err.code ?? err.cause?.code ?? err.cause?.cause?.code) === `40P01` || /deadlock detected/.test(String(e))
}

async function until(check: () => Promise<boolean>, what: string) {
  for (let i = 0; i < 400; i++) {
    if (await check()) return
    await new Promise((resolve) => setTimeout(resolve, 25))
  }
  throw new Error(`timed out waiting for ${what}`)
}

/** One forced interleaving: the child's reply holds its own rows and is held
 *  before its bus copy into the parent; the parent's End (with a summary)
 *  runs until it waits on a row lock; then the reply goes on. → which of
 *  the two Postgres killed as a deadlock victim (none = no deadlock). */
async function race(): Promise<{ deadlocks: number; failures: unknown[] }> {
  const w = await world()
  const key = Math.floor(Math.random() * 2 ** 31)
  const fn = `lock_order_gate_${id8()}`
  const gate = await gatePool!.connect()
  try {
    await gate.query(`
      CREATE FUNCTION ${fn}() RETURNS trigger AS $$
      BEGIN PERFORM pg_advisory_xact_lock_shared(${key}); RETURN NEW; END $$ LANGUAGE plpgsql`)
    await gate.query(`
      CREATE TRIGGER ${fn} BEFORE INSERT ON messages FOR EACH ROW
      WHEN (NEW.author_session_id = '${w.childId}'::uuid) EXECUTE FUNCTION ${fn}()`)
    await gate.query(`SELECT pg_advisory_lock(${key})`)
    const reply = w.host.reply({ sessionId: w.childId, body: `Found the bug.` })
    reply.catch(() => {})
    // The reply waits on the gate (its bus copy into the parent).
    let replyPid = 0
    await until(async () => {
      const { rows } = await gate.query(
        `SELECT pid FROM pg_locks WHERE locktype = 'advisory' AND NOT granted AND objid = $1::bigint::oid`,
        [key]
      )
      replyPid = rows[0]?.pid ?? 0
      return replyPid !== 0
    }, `the reply to reach the gate`)
    const end = endSession({ userId: w.owner, sessionId: w.parentId, summary: `Done here.` })
    end.catch(() => {})
    // The End waits on a row lock (the child's before the fix, the
    // parent's after it: the reply took both up front).
    await until(async () => {
      const { rows } = await gate.query(
        `SELECT 1 FROM pg_stat_activity WHERE wait_event_type = 'Lock' AND pid <> $1 AND $1 = ANY(pg_blocking_pids(pid))`,
        [replyPid]
      )
      return rows.length > 0
    }, `the End to wait on the reply`)
    await gate.query(`SELECT pg_advisory_unlock(${key})`)
    const settled = await Promise.allSettled([reply, end])
    const failures = settled.flatMap((s) => (s.status === `rejected` ? [s.reason] : []))
    return { deadlocks: failures.filter(deadlocked).length, failures }
  } finally {
    await gate.query(`SELECT pg_advisory_unlock_all()`).catch(() => {})
    await gate.query(`DROP TRIGGER IF EXISTS ${fn} ON messages`).catch(() => {})
    await gate.query(`DROP FUNCTION IF EXISTS ${fn}()`).catch(() => {})
    gate.release()
  }
}

describe.skipIf(!ready.ok)(`#ERR-5: session writes take their row locks ancestors first`, () => {
  it(`a child's reply racing its parent's End never deadlocks; both land`, async () => {
    let deadlocks = 0
    for (let round = 0; round < ROUNDS; round++) {
      const result = await race()
      deadlocks += result.deadlocks
      expect(result.failures.filter((f) => !deadlocked(f))).toEqual([])
    }
    console.info(`[lock-order.real-db] reply × End: ${deadlocks} deadlock(s) in ${ROUNDS} round(s)`)
    expect(deadlocks).toBe(0)
  }, 60_000)

  it(`a message write locks its session before its conversation (a session write then a message never deadlocks with another message)`, async () => {
    const w = await world()
    const a = await gatePool!.connect()
    const b = await gatePool!.connect()
    try {
      const { rows } = await a.query(`SELECT id FROM conversations WHERE session_id = $1`, [w.childId])
      const conv = rows[0]!.id as string
      // A: a session write first (what `lockSessionRows` / a send's picks do).
      await a.query(`BEGIN`)
      await a.query(`UPDATE sessions SET last_activity_at = now() WHERE id = $1`, [w.childId])
      const pidA = (await a.query(`SELECT pg_backend_pid() AS pid`)).rows[0]!.pid as number
      // B: another message into it (a person's line: its digest trigger).
      await b.query(`BEGIN`)
      const other = b.query(`INSERT INTO messages (conversation_id, root_id, team_id, shared, owner_id, author_kind, body)
        SELECT $1, s.root_id, s.team_id, s.shared, s.user_id, 'user', 'more' FROM sessions s WHERE s.id = $2`, [conv, w.childId])
      other.catch(() => {})
      await until(async () => {
        const { rows: waiting } = await gatePool!.query(
          `SELECT 1 FROM pg_stat_activity WHERE wait_event_type = 'Lock' AND $1 = ANY(pg_blocking_pids(pid))`,
          [pidA]
        )
        return waiting.length > 0
      }, `the other message to wait on the session write`)
      // A: then a message into the same session.
      const insert = a.query(`INSERT INTO messages (conversation_id, root_id, team_id, shared, owner_id, author_kind, body)
        SELECT $1, s.root_id, s.team_id, s.shared, s.user_id, 'agent', 'reply' FROM sessions s WHERE s.id = $2`, [conv, w.childId])
      const first = await Promise.allSettled([insert])
      await a.query(first[0]!.status === `fulfilled` ? `COMMIT` : `ROLLBACK`)
      const second = await Promise.allSettled([other])
      await b.query(second[0]!.status === `fulfilled` ? `COMMIT` : `ROLLBACK`)
      const failures = [...first, ...second].flatMap((s) => (s.status === `rejected` ? [s.reason] : []))
      expect(failures).toEqual([])
    } finally {
      await a.query(`ROLLBACK`).catch(() => {})
      await b.query(`ROLLBACK`).catch(() => {})
      a.release()
      b.release()
    }
  }, 30_000)

  it(`after the race the reply reached the ended parent and the parent ended`, async () => {
    const w = await world()
    await Promise.all([
      w.host.reply({ sessionId: w.childId, body: `Found it.` }),
      endSession({ userId: w.owner, sessionId: w.parentId, summary: `Done.` }),
    ])
    const [parent] = await db.select({ status: sessions.status }).from(sessions).where(eq(sessions.id, w.parentId))
    const [child] = await db.select({ status: sessions.status }).from(sessions).where(eq(sessions.id, w.childId))
    expect(parent!.status).toBe(`ended`)
    expect(child!.status).toBe(`ended`)
  }, 30_000)
})
