// EXP-1284: the web thread page's own pure rules, around the shared model
// (`lib/thread-view.ts`, fixture-locked ×4 — never forked here). What lives
// here is what only the web decides: how a synced run row becomes the
// model's `ThreadViewRun`, which child `?child=` names, where the reply box
// sends, whether a child is fresh enough to fade in with a ring, and which
// PR the thread's Guide merges.

import { isCodingSessionStale } from "@exp/db-schema/domain"
import { runIsStaleEnd } from "@/lib/past-runs"
import type {
  ThreadChild,
  ThreadView,
  ThreadViewItem,
  ThreadViewRun,
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

/** The PR the thread's Guide merges: the PR of its NEWEST run with one,
 *  else the one URL its issues share (a batch PR links them all), else the
 *  first issue's own. */
export function threadPrUrl(
  runs: readonly { prUrl?: string | null; startedAt: Date | string }[],
  issues: readonly { prUrl?: string | null }[]
): string | null {
  const newest = [...runs]
    .filter((run) => run.prUrl)
    .sort((a, b) => Date.parse(iso(b.startedAt)) - Date.parse(iso(a.startedAt)))[0]
  if (newest?.prUrl) return newest.prUrl
  const urls = issues.map((issue) => issue.prUrl).filter(Boolean) as string[]
  if (urls.length === 0) return null
  const counts = new Map<string, number>()
  for (const url of urls) counts.set(url, (counts.get(url) ?? 0) + 1)
  return [...counts.entries()].sort((a, b) => b[1] - a[1])[0]![0]
}

/** EXP-1284 (rule 8): the Guide a reader shows for a run — its THREAD's
 *  once the thread has one, else the run's own (`guideOfRun`, server). */
export function threadGuideResults(
  run: { results?: unknown },
  thread: { results?: unknown } | null | undefined
): unknown {
  return thread?.results ?? run.results ?? null
}
