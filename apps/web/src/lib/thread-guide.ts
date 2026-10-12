import { eq, sql } from "drizzle-orm"
import type { CodingSessionResult } from "@exp/db-schema/domain"
import { codingSessions, threads } from "@/db/schema"
import { getThreadOfSession } from "@/lib/threads"

// EXP-1284: ONE Guide per THREAD. `threads.results` is canonical; every Guide
// write (MCP `sessions_guide`/`sessions_show`, the token-gated upload route,
// pr_open's prUrl stamp) is a jsonb read-modify-write under these two locks,
// in this order:
//   1. `FOR UPDATE` on the CALLER's `coding_sessions` row (the lock every
//      report write took before threads, and the one the FEED-77 resume
//      handoff takes on a predecessor: a start locks a session row before it
//      touches a thread, so session → thread is the one order that never
//      deadlocks against it);
//   2. `FOR UPDATE` on the session's thread, which serializes every session
//      of the thread (two sessions writing one Guide would otherwise lose a
//      write).
// The written array goes to the thread AND, as a compat mirror for clients
// that predate threads (EXP-1258), onto the caller's own row. A session with
// no thread (a row the start could not place) keeps writing its own row, as
// before.
//
// Review T1: the thread's Guide holds EVERY session's entries, each stamped
// with the `sessionId` that wrote it. A writer touches only the entries it
// OWNS (`ownsGuideEntry`): its own and its resume chain's (a resume
// continues its predecessor's report), the ones tagged with its own PR (a
// session picking up an open PR writes that PR's report), and the unstamped
// ones filed before the stamp when it continues the thread's FIRST session
// (`sessionGuideEntries` shows those under that session). So a later session
// never overwrites another's topic; it files its own beside it, and the
// per-session caps count its own entries only.

type Tx = Parameters<
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  Parameters<typeof import("@/db/connection").db.transaction>[0]
>[0]
// eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
type Db = typeof import("@/db/connection").db | Tx

/** Whose Guide entries a writing session may replace, remove or tag. */
export interface GuideOwner {
  sessionId: string
  /** The session and its resume predecessors (itself first). */
  chainIds: string[]
  /** The PR stamped on the session's row (its topics are this PR's). */
  prUrl: string | null
  /** Unstamped (pre-stamp) entries are the thread's first session's: true
   *  when that session is in this one's chain, or there is no thread. */
  ownsLegacy: boolean
}

export function ownsGuideEntry(
  owner: GuideOwner,
  row: CodingSessionResult | null | undefined
): boolean {
  if (!row) return false
  const stamped = typeof row.sessionId === `string` && row.sessionId ? row.sessionId : null
  if (stamped && owner.chainIds.includes(stamped)) return true
  const pr = typeof row.prUrl === `string` ? row.prUrl.trim() : ``
  if (pr && owner.prUrl && pr === owner.prUrl) return true
  return !stamped && owner.ownsLegacy
}

/** The entries the owner wrote (what its caps and its answers count). */
export function ownedGuideEntries(
  owner: GuideOwner,
  results: readonly CodingSessionResult[] | null | undefined
): CodingSessionResult[] {
  return (results ?? []).filter((row) => ownsGuideEntry(owner, row))
}

/** Reads who the session is in Guide terms. `threadId` = its thread (null =
 *  none). Never locks; `lockRunGuide` calls it under its locks. */
export async function loadGuideOwner(
  db: Db,
  sessionId: string,
  threadId: string | null,
  prUrl: string | null
): Promise<GuideOwner> {
  // One round trip: the resume chain (bounded: a chain is a handful of
  // resumes) and the thread's first session (its oldest run item).
  const result = await db.execute(sql`
    WITH RECURSIVE chain(id, prev, depth) AS (
      SELECT id, resumed_from_id, 0 FROM coding_sessions WHERE id = ${sessionId}
      UNION ALL
      SELECT c.id, c.resumed_from_id, chain.depth + 1
      FROM coding_sessions c JOIN chain ON c.id = chain.prev
      WHERE chain.depth < 64
    )
    SELECT
      (SELECT array_agg(id::text ORDER BY depth) FROM chain) AS chain_ids,
      (SELECT ti.session_id::text FROM thread_items ti
        WHERE ${threadId}::uuid IS NOT NULL AND ti.thread_id = ${threadId}::uuid AND ti.session_id IS NOT NULL
        ORDER BY ti.sort_order, ti.created_at LIMIT 1) AS first_session_id
  `)
  const row = (result as { rows?: Array<Record<string, unknown>> }).rows?.[0] ?? {}
  const chain = Array.isArray(row.chain_ids)
    ? (row.chain_ids as unknown[]).filter((id): id is string => typeof id === `string`)
    : []
  const chainIds = [sessionId, ...chain.filter((id) => id !== sessionId)]
  const first = typeof row.first_session_id === `string` ? row.first_session_id : null
  return {
    sessionId,
    chainIds,
    prUrl: prUrl?.trim() || null,
    ownsLegacy: !threadId || !first || chainIds.includes(first),
  }
}

export interface RunGuideLock {
  sessionId: string
  /** The thread whose Guide this is; null = the session's own row (no thread). */
  threadId: string | null
  /** The Guide as it stands under the locks (every session's entries). */
  results: CodingSessionResult[] | null
  /** Which of `results` this session may touch. */
  owner: GuideOwner
}

/** Takes both locks and reads the Guide the session writes to. Null = no
 *  such session. Call inside a transaction. */
export async function lockRunGuide(
  tx: Tx,
  sessionId: string
): Promise<RunGuideLock | null> {
  const [run] = await tx
    .select({ results: codingSessions.results, prUrl: codingSessions.prUrl })
    .from(codingSessions)
    .where(eq(codingSessions.id, sessionId))
    .limit(1)
    .for(`update`)
  if (!run) return null
  const own = (run.results ?? null) as CodingSessionResult[] | null
  const prUrl = run.prUrl ?? null
  const ref = await getThreadOfSession(tx, sessionId)
  const thread = ref
    ? (
        await tx
          .select({ results: threads.results })
          .from(threads)
          .where(eq(threads.id, ref.id))
          .limit(1)
          .for(`update`)
      )[0]
    : undefined
  if (!ref || !thread) {
    const owner = await loadGuideOwner(tx, sessionId, null, prUrl)
    return { sessionId, threadId: null, results: own, owner }
  }
  const owner = await loadGuideOwner(tx, sessionId, ref.id, prUrl)
  return {
    sessionId,
    threadId: ref.id,
    // A thread whose Guide was never written starts from the session's own
    // entries (its mirror may still carry another thread's).
    results: (thread.results ??
      (own ? ownedGuideEntries(owner, own) : null)) as CodingSessionResult[] | null,
    owner,
  }
}

/** Stamps the writing session on an entry (every Guide write goes through
 *  this, so `sessionGuideEntries` can slice the thread's Guide per session). */
export function stampGuideEntry(
  lock: Pick<RunGuideLock, `sessionId`>,
  entry: CodingSessionResult
): CodingSessionResult {
  return { ...entry, sessionId: lock.sessionId }
}

/** Writes the Guide under the locks `lockRunGuide` took: the thread's, then
 *  the session's mirror (the same array). */
export async function writeRunGuide(
  tx: Tx,
  lock: RunGuideLock,
  results: CodingSessionResult[]
): Promise<void> {
  const now = new Date()
  if (lock.threadId) {
    await tx
      .update(threads)
      .set({ results, updatedAt: now })
      .where(eq(threads.id, lock.threadId))
  }
  await tx
    .update(codingSessions)
    .set({ results, updatedAt: now })
    .where(eq(codingSessions.id, lock.sessionId))
}

/** The unlocked reader's rule over a row joined to its thread: the thread's
 *  Guide once it has one, else the session's own results. */
export function guideOfRun<T>(row: {
  results?: T | null
  threadResults?: T | null
}): T | null {
  return row.threadResults ?? row.results ?? null
}
