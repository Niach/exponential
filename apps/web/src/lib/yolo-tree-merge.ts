// EXP-1146/1291: yolo mode (`teams.yolo_mode`) merges a THREAD's PRs once
// the thread is complete, root first. A thread's sessions may still open
// follow-up PRs on each other's branches, so nothing merges while any
// session of the tree works; every lifecycle edge (a turn ending, a session
// ending, `pr_open`) calls the ONE idempotent `maybeMergeYoloThread(rootId)`
// through `yoloEdge` (`lib/yolo-edge.ts`), after its write committed (EXP-1413: `sessions.report`
// when a turn ends or a child's process goes, `endSession` /
// `endHostedSessions` / Stop of a never-started row, MCP `pr open`).
//
// Complete = no session of the tree is queued or busy. Merge order = by
// stack parent (`base_pull_request_id`): a PR merges only after the PR it
// sits on, so each merge is a plain squash onto the default branch once the
// EXP-324 heal moved it there (`mergePullRequestOrThrow` awaits that heal).
// A refused PR stays open and only its own descendants wait on it; EXP-1391:
// the refusal is posted into the thread's root (it wakes and deals with it,
// once per refusal). Merging never ends a session.
import { and, eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { sessions, teams } from "@/db/schema"
import { livePullRequestsOfRoot, type PullRequestRow } from "@/lib/integrations/pr-sync"
import { postMergeRefusal } from "@/lib/merge-refusal"
import { mergePullRequestOrThrow } from "@/lib/pull-requests"

export type YoloOutcome =
  | { kind: `merged` }
  | { kind: `queued` }
  | { kind: `failed`; error: string }
  | { kind: `blocked`; on: string }

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

const inFlight = new Map<string, Promise<YoloResult>>()

/** Merge the thread's live PRs when its team is in yolo mode and no session
 *  of the tree is working. Single-flight per thread; never throws. */
export function maybeMergeYoloThread(rootId: string): Promise<YoloResult> {
  const running = inFlight.get(rootId)
  if (running) return running
  const run = mergeNow(rootId)
    .catch((err): YoloResult => {
      console.error(`[yolo] thread ${rootId} merge failed`, err)
      return { status: `merged`, outcomes: {} }
    })
    .finally(() => inFlight.delete(rootId))
  inFlight.set(rootId, run)
  return run
}

async function mergeNow(rootId: string): Promise<YoloResult> {
  const [root] = await db
    .select({ userId: sessions.userId, yoloMode: teams.yoloMode })
    .from(sessions)
    .innerJoin(teams, eq(teams.id, sessions.teamId))
    .where(and(eq(sessions.id, rootId), eq(sessions.rootId, rootId)))
    .limit(1)
  if (!root?.yoloMode) return { status: `not_yolo` }
  const tree = await db
    .select({ status: sessions.status, busy: sessions.busy })
    .from(sessions)
    .where(eq(sessions.rootId, rootId))
  if (tree.some((session) => session.status === `queued` || session.busy)) {
    return { status: `waiting` }
  }
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
    try {
      const result = await mergePullRequestOrThrow({ userId: root.userId, pullRequestId: id })
      outcomes[id] = result.queued ? { kind: `queued` } : { kind: `merged` }
    } catch (err) {
      outcomes[id] = { kind: `failed`, error: err instanceof Error ? err.message : String(err) }
      await postMergeRefusal({ pullRequestId: id, actorUserId: root.userId, error: err, automerge: true })
    }
  }
  return { status: `merged`, outcomes }
}
