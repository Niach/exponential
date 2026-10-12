import { eq } from "drizzle-orm"
import type { CodingSessionResult } from "@exp/db-schema/domain"
import { codingSessions, threads } from "@/db/schema"
import { getThreadOfSession } from "@/lib/threads"

// EXP-1284: ONE Guide per THREAD. `threads.results` is canonical; every Guide
// write (MCP `sessions_guide`/`sessions_show`, the token-gated upload route,
// pr_open's prUrl stamp) is a jsonb read-modify-write under these two locks,
// in this order:
//   1. `FOR UPDATE` on the CALLER's `coding_sessions` row (the lock every
//      report write took before threads, and the one the FEED-77 resume
//      handoff takes on a predecessor: a start locks a run row before it
//      touches a thread, so run → thread is the one order that never
//      deadlocks against it);
//   2. `FOR UPDATE` on the run's thread, which serializes every run of the
//      thread (two runs writing one Guide would otherwise lose a write).
// The written array goes to the thread AND, as a compat mirror for clients
// that predate threads (EXP-1258), onto the caller's own row. A run with no
// thread (a row the start could not place) keeps writing its own row, as
// before. A thread whose Guide was never written (NULL) starts from the
// run's own results, so a run moved into a fresh thread keeps its sections.

type Tx = Parameters<
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  Parameters<typeof import("@/db/connection").db.transaction>[0]
>[0]

export interface RunGuideLock {
  sessionId: string
  /** The thread whose Guide this is; null = the run's own row (no thread). */
  threadId: string | null
  /** The Guide as it stands under the locks. */
  results: CodingSessionResult[] | null
}

/** Takes both locks and reads the Guide the run writes to. Null = no such
 *  run. Call inside a transaction. */
export async function lockRunGuide(
  tx: Tx,
  sessionId: string
): Promise<RunGuideLock | null> {
  const [run] = await tx
    .select({ results: codingSessions.results })
    .from(codingSessions)
    .where(eq(codingSessions.id, sessionId))
    .limit(1)
    .for(`update`)
  if (!run) return null
  const own = (run.results ?? null) as CodingSessionResult[] | null
  const ref = await getThreadOfSession(tx, sessionId)
  if (!ref) return { sessionId, threadId: null, results: own }
  const [thread] = await tx
    .select({ results: threads.results })
    .from(threads)
    .where(eq(threads.id, ref.id))
    .limit(1)
    .for(`update`)
  if (!thread) return { sessionId, threadId: null, results: own }
  return {
    sessionId,
    threadId: ref.id,
    results: (thread.results ?? own) as CodingSessionResult[] | null,
  }
}

/** Writes the Guide under the locks `lockRunGuide` took: the thread's, then
 *  the run's mirror (the same array). */
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
 *  Guide once it has one, else the run's own results. */
export function guideOfRun<T>(row: {
  results?: T | null
  threadResults?: T | null
}): T | null {
  return row.threadResults ?? row.results ?? null
}
