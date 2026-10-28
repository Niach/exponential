// EXP-1146/1291: yolo mode (`teams.yolo_mode`) merges a THREAD's PRs once
// the thread is complete, root first. A thread's sessions may still open
// follow-up PRs on each other's branches, so nothing merges while any
// session of the tree works; every lifecycle edge (a turn ending, a session
// ending, `pr_open`) calls the ONE idempotent `maybeMergeYoloThread(rootId)`
// through `yoloEdge` (`lib/yolo-edge.ts`), after its write committed (EXP-1413:
// `sessions.report` on a real busy → idle edge, not a Stopped one, or when a
// child's process goes; `endSession` / `endHostedSessions`; MCP `pr open`).
//
// Complete = no live session of the tree is queued, busy, waiting on a
// person (an open question/plan card = needs_input, a wall = blocked),
// holding a Stopped turn (`stopped_at`), or has unread input (a child's
// report, a person's message, an answered card: the device's pickup set).
// Drafts are skipped silently (their descendants wait). Merge order = by
// stack parent (`base_pull_request_id`): a PR merges only after the PR it
// sits on, so each merge is a plain squash onto the default branch once the
// EXP-324 heal moved it there (`mergePullRequestOrThrow` awaits that heal).
// A refused PR stays open and only its own descendants wait on it; EXP-1391:
// the refusal is posted into the thread's root (it wakes and deals with it,
// once per refusal of that PR at that head commit). Merging never ends a
// session. One pass per thread at a time across web instances: a session
// advisory lock per root.
import { and, eq, isNotNull, isNull, ne, or } from "drizzle-orm"
import { db, pool } from "@/db/connection"
import { messages, sessions, teams } from "@/db/schema"
import { livePullRequestsOfRoot, type PullRequestRow } from "@/lib/integrations/pr-sync"
import { answeredOwnCard, pendingInput } from "@/lib/sessions/messages"
import { postMergeRefusal } from "@/lib/merge-refusal"
import { mergePullRequestOrThrow, pullRequestHeadSha } from "@/lib/pull-requests"
import { isMergeRefusal } from "@/lib/trpc/pr-merge-error"

export type YoloOutcome =
  | { kind: `merged` }
  | { kind: `queued` }
  | { kind: `failed`; error: string }
  | { kind: `blocked`; on: string }
  | { kind: `draft` }

export type YoloResult =
  | { status: `not_yolo` }
  | { status: `waiting` }
  | { status: `merged`; outcomes: Record<string, YoloOutcome> }

/** Pure: the merge order, parents before children (ties: oldest first). A
 *  PR whose parent is not in the set (the default branch, an already merged
 *  one) is a root of the order. */
export function yoloMergeOrder(
  prs: ReadonlyArray<Pick<PullRequestRow, `id` | `basePullRequestId` | `createdAt`>>
): string[] {
  const ids = new Set(prs.map((pr) => pr.id))
  const byAge = [...prs].sort((a, b) => a.createdAt.getTime() - b.createdAt.getTime())
  const order: string[] = []
  const placed = new Set<string>()
  let progressed = true
  while (progressed && placed.size < prs.length) {
    progressed = false
    for (const pr of byAge) {
      if (placed.has(pr.id)) continue
      const parent = pr.basePullRequestId
      if (parent && ids.has(parent) && !placed.has(parent)) continue
      order.push(pr.id)
      placed.add(pr.id)
      progressed = true
    }
  }
  // A cycle (never written, but cheap to survive): the rest, oldest first.
  for (const pr of byAge) if (!placed.has(pr.id)) order.push(pr.id)
  return order
}

/** EXP-1418: one pass per thread in process; a trigger arriving while it
 *  runs marks it `dirty` and the pass runs ONCE more at its end (the late
 *  edge's write may have landed after the running pass read the tree). */
const inFlight = new Map<string, { run: Promise<YoloResult>; dirty: boolean }>()
/** EXP-1418: another instance held the thread's lock: one retry later (its
 *  pass may have read the tree before this edge's write committed). */
const lockRetries = new Map<string, ReturnType<typeof setTimeout>>()
export const YOLO_LOCK_RETRY_MS = 5_000

/** Merge the thread's live PRs when its team is in yolo mode and the tree is
 *  complete. Single-flight per thread (in process, and across instances by
 *  the advisory lock); never throws. */
export function maybeMergeYoloThread(rootId: string): Promise<YoloResult> {
  const running = inFlight.get(rootId)
  if (running) {
    running.dirty = true
    return running.run
  }
  const entry = { dirty: false, run: Promise.resolve<YoloResult>({ status: `waiting` }) }
  entry.run = (async (): Promise<YoloResult> => {
    let result: YoloResult
    do {
      entry.dirty = false
      result = await mergeLocked(rootId).catch((err): YoloResult => {
        console.error(`[yolo] thread ${rootId} merge failed`, err)
        return { status: `merged`, outcomes: {} }
      })
    } while (entry.dirty)
    return result
  })().finally(() => inFlight.delete(rootId))
  inFlight.set(rootId, entry)
  return entry.run
}

function retryAfterLock(rootId: string): void {
  if (lockRetries.has(rootId)) return
  const timer = setTimeout(() => {
    lockRetries.delete(rootId)
    void maybeMergeYoloThread(rootId)
  }, YOLO_LOCK_RETRY_MS)
  timer.unref?.()
  lockRetries.set(rootId, timer)
}

/** A SESSION-level advisory lock on one pooled connection (no transaction
 *  held open across GitHub calls). Another instance's pass = `waiting` (and
 *  one retry). EXP-1418: an unlock that throws DISCARDS the connection
 *  (`release(err)`): back in the pool it would hold the thread's lock and
 *  every later pass would read `waiting` until the process restarts. */
async function mergeLocked(rootId: string): Promise<YoloResult> {
  const client = await pool.connect()
  const key = `yolo_thread:${rootId}`
  let broken: Error | undefined
  try {
    const { rows } = await client.query<{ locked: boolean }>(
      `SELECT pg_try_advisory_lock(hashtext($1)) AS locked`,
      [key]
    )
    if (!rows[0]?.locked) {
      retryAfterLock(rootId)
      return { status: `waiting` }
    }
    try {
      return await mergeNow(rootId)
    } finally {
      try {
        await client.query(`SELECT pg_advisory_unlock(hashtext($1))`, [key])
      } catch (err) {
        broken = err instanceof Error ? err : new Error(String(err))
        console.error(`[yolo] thread ${rootId} unlock failed: connection discarded`, err)
      }
    }
  } catch (err) {
    // A failed lock query leaves the lock state unknown: discard it too.
    broken ??= err instanceof Error ? err : new Error(String(err))
    throw err
  } finally {
    client.release(broken)
  }
}

/** Whether a live session of the tree still works or waits on someone. */
async function treeBusy(rootId: string): Promise<boolean> {
  const live = and(eq(sessions.rootId, rootId), ne(sessions.status, `ended`))
  const [working] = await db
    .select({ id: sessions.id })
    .from(sessions)
    .where(
      and(
        live,
        or(
          eq(sessions.status, `queued`),
          eq(sessions.busy, true),
          eq(sessions.needsInput, true),
          isNotNull(sessions.blocked),
          isNotNull(sessions.stoppedAt)
        )
      )
    )
    .limit(1)
  if (working) return true
  const [unread] = await db
    .select({ id: messages.id })
    .from(messages)
    .innerJoin(sessions, eq(sessions.id, messages.sessionId))
    .where(and(eq(messages.rootId, rootId), live, isNull(messages.consumedAt), or(pendingInput(), answeredOwnCard())))
    .limit(1)
  return !!unread
}

async function mergeNow(rootId: string): Promise<YoloResult> {
  const [root] = await db
    .select({ userId: sessions.userId, yoloMode: teams.yoloMode })
    .from(sessions)
    .innerJoin(teams, eq(teams.id, sessions.teamId))
    .where(and(eq(sessions.id, rootId), eq(sessions.rootId, rootId)))
    .limit(1)
  if (!root?.yoloMode) return { status: `not_yolo` }
  if (await treeBusy(rootId)) return { status: `waiting` }
  const prs = await livePullRequestsOfRoot(rootId)
  const byId = new Map(prs.map((pr) => [pr.id, pr]))
  const outcomes: Record<string, YoloOutcome> = {}
  for (const id of yoloMergeOrder(prs)) {
    const pr = byId.get(id)!
    const parent = pr.basePullRequestId ? outcomes[pr.basePullRequestId] : undefined
    if (parent && parent.kind !== `merged`) {
      outcomes[id] = { kind: `blocked`, on: pr.basePullRequestId! }
      continue
    }
    // A draft is not ready: skipped silently (no refusal, no wake).
    if (pr.state === `draft`) {
      outcomes[id] = { kind: `draft` }
      continue
    }
    try {
      const result = await mergePullRequestOrThrow({ userId: root.userId, pullRequestId: id })
      outcomes[id] = result.queued ? { kind: `queued` } : { kind: `merged` }
    } catch (err) {
      outcomes[id] = { kind: `failed`, error: err instanceof Error ? err.message : String(err) }
      if (isMergeRefusal(err)) {
        await postMergeRefusal({
          pullRequestId: id,
          actorUserId: root.userId,
          error: err,
          automerge: true,
          headSha: await pullRequestHeadSha(id),
        })
      }
    }
  }
  return { status: `merged`, outcomes }
}
