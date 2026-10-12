// EXP-1284: the web thread page's own pure rules, around the shared model
// (`lib/thread-view.ts`, fixture-locked ×4 — never forked here). What lives
// here is what only the web decides: how a synced run row becomes the
// model's `ThreadViewRun`, which child `?child=` names, where the reply box
// sends, whether a child is fresh enough to fade in with a ring, and which
// PR the thread's Guide merges.

import { isCodingSessionStale } from "@exp/db-schema/domain"
import { runIsStaleEnd } from "@/lib/past-runs"
import {
  partitionThreadList,
  threadPrTarget,
  type ThreadChild,
  type ThreadPrCandidate,
  type ThreadView,
  type ThreadViewItem,
  type ThreadViewRun,
} from "@/lib/thread-view"

/** A timestamp as the model reads it (Electric hands strings, the zod
 *  schema may hand Dates). */
export function iso(value: Date | string | null | undefined): string {
  if (value == null) return ``
  return typeof value === `string` ? value : value.toISOString()
}

/** The columns of a synced run the model needs. */
export interface ThreadRunRow {
  id: string
  userId: string
  status: string
  endedBy?: string | null
  needsInput: boolean
  agentBusy?: boolean | null
  issueId: string | null
  parentSessionId: string | null
  resumedFromId: string | null
  startedAt: Date | string
  endedAt: Date | string | null
  updatedAt: Date | string
}

/** A run's state as the thread shows it: a heartbeat-dead row reads ended
 *  (the lists' rule, `useMyLiveRuns`), a sweep end (EXP-888) still live. */
export function threadRunStatus(run: ThreadRunRow, now: Date): string {
  const liveStatus =
    run.status === `running` || run.status === `in_review` || runIsStaleEnd(run)
  if (!liveStatus) return `ended`
  if (isCodingSessionStale(new Date(iso(run.updatedAt)), now)) return `ended`
  return run.status === `ended` ? `running` : run.status
}

/** A synced run row → the model's run. `name` = the platform's session
 *  identity subject (`sessionIdentity(...).subject`). */
export function toThreadViewRun(
  run: ThreadRunRow,
  extra: { name: string; deviceLabel: string | null; now: Date }
): ThreadViewRun {
  return {
    id: run.id,
    name: extra.name,
    userId: run.userId,
    deviceLabel: extra.deviceLabel,
    status: threadRunStatus(run, extra.now),
    needsInput: run.needsInput,
    agentBusy: run.agentBusy === true,
    issueId: run.issueId,
    parentSessionId: run.parentSessionId,
    resumedFromId: run.resumedFromId,
    startedAt: iso(run.startedAt),
    endedAt: run.endedAt ? iso(run.endedAt) : null,
  }
}

/** A synced thread item → the model's item. */
export function toThreadViewItem(item: {
  id: string
  kind: string
  issueId: string | null
  sessionId: string | null
  source: string
  addedBySessionId: string | null
  sortOrder: number
  createdAt: Date | string
}): ThreadViewItem {
  return {
    id: item.id,
    kind: item.kind === `run` ? `run` : `issue`,
    issueId: item.issueId,
    sessionId: item.sessionId,
    source: item.source === `agent` ? `agent` : `user`,
    addedBySessionId: item.addedBySessionId,
    sortOrder: item.sortOrder,
    createdAt: iso(item.createdAt),
  }
}

// ── The child on show (`?child=`) ────────────────────────────────────────────

/** `?child=<IDENTIFIER>|guide`; absent = the conversation. */
export type ThreadChildKey =
  | { kind: `conversation` }
  | { kind: `issue`; identifier: string }
  | { kind: `guide` }

export const THREAD_GUIDE_CHILD = `guide`

export function parseThreadChild(raw: unknown): ThreadChildKey {
  if (typeof raw !== `string` || !raw.trim()) return { kind: `conversation` }
  const value = raw.trim()
  if (value.toLowerCase() === THREAD_GUIDE_CHILD) return { kind: `guide` }
  return { kind: `issue`, identifier: value.toUpperCase() }
}

/** The search value naming a child (undefined = the conversation). */
export function threadChildSearch(child: ThreadChild): string | undefined {
  switch (child.kind) {
    case `issue`:
      return child.identifier
    case `guide`:
      return THREAD_GUIDE_CHILD
    default:
      return undefined
  }
}

/** The strip entry `?child=` selects; an issue no longer in the thread (or
 *  not synced yet) falls back to the conversation. */
export function selectedThreadChild(
  view: Pick<ThreadView, `strip`>,
  key: ThreadChildKey
): ThreadChild {
  if (key.kind === `guide`) {
    return view.strip.find((child) => child.kind === `guide`) ?? { kind: `conversation` }
  }
  if (key.kind === `issue`) {
    const match = view.strip.find(
      (child) => child.kind === `issue` && child.identifier.toUpperCase() === key.identifier
    )
    if (match) return match
  }
  return { kind: `conversation` }
}

// ── The reply box ────────────────────────────────────────────────────────────

/** Where `Reply in this thread` goes:
 *   send   the latest LIVE run the viewer owns (the existing steer send);
 *   resume no live run of theirs, but the thread's latest run IS theirs:
 *          resume it (the resume stays in the thread), then send;
 *   start  otherwise a new run in this thread.
 *  null = the viewer cannot steer at all (not a member, no relay). */
export type ThreadReplyTarget =
  | { kind: `send`; runId: string }
  | { kind: `resume`; runId: string }
  | { kind: `start` }

export function threadReplyTarget(input: {
  runs: readonly Pick<ThreadViewRun, `id` | `userId` | `status` | `startedAt`>[]
  viewerId: string | null | undefined
  canSteer: boolean
}): ThreadReplyTarget | null {
  const { viewerId } = input
  if (!input.canSteer || !viewerId) return null
  const ordered = [...input.runs].sort(
    (a, b) => Date.parse(b.startedAt) - Date.parse(a.startedAt) || b.id.localeCompare(a.id)
  )
  const live = ordered.find((run) => run.userId === viewerId && run.status !== `ended`)
  if (live) return { kind: `send`, runId: live.id }
  const latest = ordered[0]
  if (latest && latest.userId === viewerId) return { kind: `resume`, runId: latest.id }
  return { kind: `start` }
}

// ── A child the viewer has not seen yet ──────────────────────────────────────

/** A child created at most this long before it FIRST renders fades in with a
 *  ring (the agent just linked it). */
export const THREAD_FRESH_CHILD_MS = 10_000

export function isFreshThreadChild(
  createdAt: Date | string | null | undefined,
  firstSeenAt: number
): boolean {
  const created = Date.parse(iso(createdAt))
  if (!Number.isFinite(created)) return false
  return firstSeenAt - created <= THREAD_FRESH_CHILD_MS && firstSeenAt >= created - 1_000
}

// ── The thread's PR ──────────────────────────────────────────────────────────

/** Review T8: the candidates `threadPrTarget` (lib/thread-view.ts, fixture-
 *  locked ×4) picks from — the PR on each of the thread's sessions and
 *  issues, with its state and the row's activity time. */
export function threadPrCandidates(
  sessions: readonly { prUrl?: string | null; prState?: string | null; updatedAt: Date | string }[],
  issues: readonly { prUrl?: string | null; prState?: string | null; updatedAt: Date | string }[]
): ThreadPrCandidate[] {
  return [...sessions, ...issues].flatMap((row) =>
    row.prUrl ? [{ url: row.prUrl, state: row.prState ?? null, at: iso(row.updatedAt) }] : []
  )
}

/** The PR the thread's Guide and Merge capsule target: the newest OPEN PR,
 *  else the newest MERGED one (`threadPrTarget`). */
export function threadPrUrl(
  sessions: Parameters<typeof threadPrCandidates>[0],
  issues: Parameters<typeof threadPrCandidates>[1]
): string | null {
  return threadPrTarget(threadPrCandidates(sessions, issues))
}

/** EXP-1284 (rule 8): the Guide a reader shows for a run — its THREAD's
 *  once the thread has one, else the run's own (`guideOfRun`, server). */
export function threadGuideResults(
  run: { results?: unknown },
  thread: { results?: unknown } | null | undefined
): unknown {
  return thread?.results ?? run.results ?? null
}

// ── The lists ────────────────────────────────────────────────────────────────

/** What the list helpers read off a resolved thread (`ThreadBundle`). */
export interface ThreadListBundle {
  thread: { id: string; teamId: string; updatedAt: Date | string }
  runs: readonly { id: string }[]
  issues: readonly unknown[]
  mineLive: boolean
  mine: boolean
}

/** Review T6: Running vs Recent over the resolved threads, scoped to the
 *  ACTIVE team (`scopeTeam`; null = every team, the phone's banded lists).
 *  A thread whose every child is gone has nothing to show. */
export function splitThreadList<B extends ThreadListBundle>(
  bundles: readonly B[],
  scopeTeam: string | null
): { running: B[]; recent: B[] } {
  const entries = bundles
    .filter((bundle) => bundle.runs.length > 0 || bundle.issues.length > 0)
    .map((bundle) => ({
      threadId: bundle.thread.id,
      teamId: bundle.thread.teamId,
      updatedAt: iso(bundle.thread.updatedAt),
      mineLive: bundle.mineLive,
      mine: bundle.mine,
      bundle,
    }))
  const { running, recent } = partitionThreadList(entries, { teamId: scopeTeam })
  return {
    running: running.map((entry) => entry.bundle),
    recent: recent.map((entry) => entry.bundle),
  }
}

/** Review T4: my live sessions that no listed thread holds (placement
 *  failed, or the items shape lags) — they still list, as session rows. */
export function orphanLiveSessions<S extends { id: string }>(
  bundles: readonly ThreadListBundle[],
  live: readonly S[]
): S[] {
  const listed = new Set(bundles.flatMap((bundle) => bundle.runs.map((run) => run.id)))
  return live.filter((run) => !listed.has(run.id))
}

/** Review T16: a cached thread view is reused while every input is the very
 *  same value (rows by identity, derived strings by value). */
export function sameThreadInputs(previous: readonly unknown[] | undefined, next: readonly unknown[]): boolean {
  return (
    previous !== undefined &&
    previous.length === next.length &&
    previous.every((value, index) => Object.is(value, next[index]))
  )
}

// ── Missing vs still syncing (review T14) ────────────────────────────────────

/** What the server said when asked once (`threads.get`). */
export type ThreadFetchState = `idle` | `pending` | `found` | `missing`

/** How long a thread the server confirmed may take to reach the shape
 *  before the page concludes it is gone after all. */
export const THREAD_SYNC_GRACE_MS = 15_000

/** The thread page's state when the synced row is not (yet) there: keep the
 *  skeleton while the sync has not settled, while a `thread_items` row
 *  names the id (the shell is a separate shape), or while the ONE
 *  `threads.get` is out or answered "found"; only a server "missing" (or a
 *  found thread that never synced within the grace) is "Thread not found". */
export function threadPageState(input: {
  hasBundle: boolean
  syncSettled: boolean
  itemNamesIt: boolean
  fetch: ThreadFetchState
  graceOver: boolean
}): `ready` | `skeleton` | `missing` {
  if (input.hasBundle) return `ready`
  if (!input.syncSettled) return `skeleton`
  if (input.fetch === `missing`) return `missing`
  if (input.fetch === `found`) return input.graceOver ? `missing` : `skeleton`
  if (input.itemNamesIt && !input.graceOver) return `skeleton`
  return input.fetch === `idle` || input.fetch === `pending` ? `skeleton` : `missing`
}
