import { useMemo, useRef } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import type { CodingSession, Device, Issue, Thread, ThreadItem } from "@/db/schema"
import {
  codingSessionCollection,
  deviceCollection,
  issueCollection,
  threadItemsCollection,
  threadsCollection,
} from "@/lib/collections"
import { batchRunIssues, isBatchRun } from "@/lib/batch-run"
import { sessionIdentity } from "@/lib/session-identity"
import { sessionDeviceRow } from "@/components/session-tree"
import { teamScopeIds } from "@/lib/team-scope"
import { buildThreadView, type ThreadView } from "@/lib/thread-view"
import {
  iso,
  orphanLiveSessions,
  sameThreadInputs,
  splitThreadList,
  threadRunStatus,
  toThreadViewItem,
  toThreadViewRun,
} from "@/lib/thread-page"
import { useNow } from "@/hooks/use-now"
import { useMyLiveRuns } from "@/hooks/use-my-live-runs"

// EXP-1284: the web's thread reads — every surface that shows a thread (the
// thread page, the work-tab chip, the sidebar's Running, Agent home's
// Recent, the Agent › Recent panel, the issue page's Threads row) resolves
// it HERE, through the ONE model (`lib/thread-view.ts`): the synced thread
// shell + its items, the runs and issues they name, each run named by its
// session identity and its host device's (renamed) label.

/** A synced thread item (the shape drops the server-only board mirrors). */
export type SyncedThreadItem = Omit<ThreadItem, `boardDeletedAt` | `boardArchivedAt`>

/** One thread, resolved. */
export interface ThreadBundle {
  thread: Thread
  items: SyncedThreadItem[]
  /** The runs its run items name, start order. */
  runs: CodingSession[]
  /** The issues its issue items name (visible ones), link order. */
  issues: Issue[]
  view: ThreadView
  /** Each run's name (its session identity subject), by run id. */
  runNames: Map<string, string>
  /** The viewer owns a live session in it (`partitionThreadList`). */
  mineLive: boolean
  /** The viewer has a session in it or created it (Recent lists only these). */
  mine: boolean
  /** The newest live run of the viewer's, for marks + bindings. */
  myLiveRun: CodingSession | null
  /** The newest live run of anyone's (its state = the row's mark). */
  liveRun: CodingSession | null
}

function sortedKey(ids: Iterable<string>): string[] {
  return [...new Set(ids)].sort()
}

/**
 * Threads of the given teams (all of them, or only `threadIds`), resolved.
 * `isReady` = the threads AND items snapshots arrived.
 */
export function useThreadBundles(
  teamId: string | readonly string[] | undefined,
  currentUserId: string | undefined,
  options: { threadIds?: readonly string[] } = {}
): { bundles: ThreadBundle[]; isReady: boolean; now: Date } {
  const teamIds = teamScopeIds(teamId)
  const teamKey = teamIds.join(`,`)
  const only = options.threadIds ? sortedKey(options.threadIds) : null
  const onlyKey = only ? only.join(`,`) : null

  const { data: threadRows, isReady: threadsReady } = useLiveQuery(
    (query) =>
      teamIds.length === 0 || (only && only.length === 0)
        ? undefined
        : query
            .from({ t: threadsCollection })
            .where(({ t }) =>
              only
                ? and(inArray(t.teamId, teamIds), inArray(t.id, only))
                : inArray(t.teamId, teamIds)
            ),
    [teamKey, onlyKey]
  )
  const { data: itemRows, isReady: itemsReady } = useLiveQuery(
    (query) =>
      teamIds.length === 0 || (only && only.length === 0)
        ? undefined
        : query
            .from({ it: threadItemsCollection })
            .where(({ it }) =>
              only
                ? and(inArray(it.teamId, teamIds), inArray(it.threadId, only))
                : inArray(it.teamId, teamIds)
            ),
    [teamKey, onlyKey]
  )
  const items = useMemo(() => (itemRows ?? []) as SyncedThreadItem[], [itemRows])

  const sessionIds = useMemo(
    () => sortedKey(items.flatMap((item) => (item.sessionId ? [item.sessionId] : []))),
    [items]
  )
  const { data: runRows } = useLiveQuery(
    (query) =>
      sessionIds.length > 0
        ? query
            .from({ s: codingSessionCollection })
            .where(({ s }) => inArray(s.id, sessionIds))
        : undefined,
    [sessionIds.join(`,`)]
  )
  const runs = useMemo(() => (runRows ?? []) as CodingSession[], [runRows])

  // The issues the items name, plus those the runs name themselves after (a
  // run's subject issue, a batch's covered issues).
  const issueIds = useMemo(
    () =>
      sortedKey([
        ...items.flatMap((item) => (item.issueId ? [item.issueId] : [])),
        ...runs.flatMap((run) => [
          ...(run.issueId ? [run.issueId] : []),
          ...(isBatchRun(run) ? (run.batchIssueIds ?? []) : []),
        ]),
      ]),
    [items, runs]
  )
  const { data: issueRows } = useLiveQuery(
    (query) =>
      issueIds.length > 0
        ? query
            .from({ i: issueCollection })
            .where(({ i }) => inArray(i.id, issueIds))
        : undefined,
    [issueIds.join(`,`)]
  )
  const { data: deviceRows } = useLiveQuery(
    (query) => (teamIds.length > 0 ? query.from({ d: deviceCollection }) : undefined),
    [teamKey]
  )
  const now = useNow(30_000)

  // EXP-1284 (review T16): ONE view per thread, rebuilt only when ITS
  // inputs change — a heartbeat on an unrelated session (every live row
  // ticks) never rebuilds every other thread's view. The cache compares the
  // inputs by identity (the collections hand back the same row object until
  // that row changes) plus the derived strings (device label, liveness).
  const cacheRef = useRef(new Map<string, { deps: unknown[]; bundle: ThreadBundle }>())
  const bundles = useMemo(() => {
    const issuesById = new Map(((issueRows ?? []) as Issue[]).map((row) => [row.id, row]))
    const runsById = new Map(runs.map((row) => [row.id, row]))
    const devices = (deviceRows ?? []) as Device[]
    const itemsByThread = new Map<string, SyncedThreadItem[]>()
    for (const item of items) {
      const list = itemsByThread.get(item.threadId) ?? []
      list.push(item)
      itemsByThread.set(item.threadId, list)
    }
    const pool = [...issuesById.values()]
    const cache = cacheRef.current
    const next = new Map<string, { deps: unknown[]; bundle: ThreadBundle }>()
    const out = ((threadRows ?? []) as Thread[]).map((thread): ThreadBundle => {
      const own = itemsByThread.get(thread.id) ?? []
      const threadRuns = own
        .flatMap((item) => (item.sessionId ? [runsById.get(item.sessionId)] : []))
        .filter((run): run is CodingSession => run !== undefined)
        .sort(
          (a, b) =>
            Date.parse(iso(a.startedAt)) - Date.parse(iso(b.startedAt)) || a.id.localeCompare(b.id)
        )
      const threadIssues = own
        .flatMap((item) => (item.issueId ? [issuesById.get(item.issueId)] : []))
        .filter((issue): issue is Issue => issue !== undefined)
      const namingIssues = threadRuns.flatMap((run) =>
        run.issueId
          ? [issuesById.get(run.issueId)]
          : isBatchRun(run)
            ? batchRunIssues(run, pool)
            : []
      )
      const deviceLabels = threadRuns.map(
        (run) => sessionDeviceRow(run, devices)?.label || run.deviceLabel || null
      )
      const statuses = threadRuns.map((run) => threadRunStatus(run, now))
      const deps: unknown[] = [
        thread,
        currentUserId,
        ...own,
        `|`,
        ...threadRuns,
        `|`,
        ...threadIssues,
        `|`,
        ...namingIssues,
        `|`,
        ...deviceLabels,
        `|`,
        ...statuses,
      ]
      const cached = cache.get(thread.id)
      if (cached && sameThreadInputs(cached.deps, deps)) {
        next.set(thread.id, cached)
        return cached.bundle
      }
      const runNames = new Map(
        threadRuns.map((run) => [
          run.id,
          sessionIdentity({
            session: run,
            issue: run.issueId ? issuesById.get(run.issueId) : undefined,
            batchIssues: isBatchRun(run) ? batchRunIssues(run, pool) : [],
          }).subject,
        ])
      )
      const view = buildThreadView({
        thread: {
          id: thread.id,
          title: thread.title,
          results: thread.results,
          updatedAt: iso(thread.updatedAt),
        },
        items: own.map(toThreadViewItem),
        issues: threadIssues.map((issue) => ({
          id: issue.id,
          identifier: issue.identifier,
          title: issue.title,
          createdAt: iso(issue.createdAt),
        })),
        runs: threadRuns.map((run, index) =>
          toThreadViewRun(run, {
            name: runNames.get(run.id) ?? ``,
            deviceLabel: deviceLabels[index] ?? null,
            now,
          })
        ),
      })
      const liveRuns = threadRuns.filter((_, index) => statuses[index] !== `ended`)
      const myLive = liveRuns.filter((run) => run.userId === currentUserId)
      const bundle: ThreadBundle = {
        thread,
        items: own,
        runs: threadRuns,
        issues: threadIssues,
        view,
        runNames,
        mineLive: myLive.length > 0,
        // Review T6: a session of mine or a thread I created.
        mine:
          (currentUserId != null && thread.createdBy === currentUserId) ||
          threadRuns.some((run) => run.userId === currentUserId),
        myLiveRun: myLive[myLive.length - 1] ?? null,
        liveRun: liveRuns[liveRuns.length - 1] ?? null,
      }
      next.set(thread.id, { deps, bundle })
      return bundle
    })
    cacheRef.current = next
    return out
  }, [threadRows, items, runs, issueRows, deviceRows, now, currentUserId])

  return {
    bundles,
    isReady: teamIds.length > 0 && threadsReady && itemsReady,
    now,
  }
}

/** One thread by id (null until it synced, or when it is not the team's). */
export function useThreadBundle(
  teamId: string | undefined,
  currentUserId: string | undefined,
  threadId: string | undefined
): { bundle: ThreadBundle | null; isReady: boolean; now: Date } {
  const ids = useMemo(() => (threadId ? [threadId] : []), [threadId])
  const { bundles, isReady, now } = useThreadBundles(teamId, currentUserId, {
    threadIds: ids,
  })
  return { bundle: bundles[0] ?? null, isReady, now }
}

/** The list surfaces' model (review T6, `partitionThreadList`, fixture-
 *  locked): Running = a thread holding a live session of mine; Recent = my
 *  other threads (a session of mine, or one I created), newest activity
 *  first. Scope: ONE team = that team (web md+, the active team); several =
 *  every one of them (the phone's cross-team lists, banded per team).
 *  Review T4: `orphans` = my live sessions no listed thread holds (its item
 *  failed to place, or the items shape lags the sessions shape) — they still
 *  show, as plain session rows, so a live session never vanishes. */
export function useThreadList(
  teamId: string | readonly string[] | undefined,
  currentUserId: string | undefined
): {
  running: ThreadBundle[]
  recent: ThreadBundle[]
  orphans: CodingSession[]
  isReady: boolean
  now: Date
} {
  const { bundles, isReady, now } = useThreadBundles(teamId, currentUserId)
  const { runs: myLive } = useMyLiveRuns(teamId, currentUserId, 30_000)
  const scopeTeam =
    typeof teamId === `string` ? teamId : teamId && teamId.length === 1 ? teamId[0]! : null
  const parts = useMemo(() => splitThreadList(bundles, scopeTeam), [bundles, scopeTeam])
  const orphans = useMemo(
    // Only once the thread rows have settled: before that every live
    // session would flash in as an orphan.
    () => (isReady ? orphanLiveSessions(bundles, myLive) : []),
    [bundles, myLive, isReady]
  )
  return { ...parts, orphans, isReady, now }
}

/** The thread a run sits in (its run item), or null while it syncs. */
export function useThreadOfRun(runId: string | null | undefined): Thread | null {
  const { data: itemRows } = useLiveQuery(
    (query) =>
      runId
        ? query
            .from({ it: threadItemsCollection })
            .where(({ it }) => eq(it.sessionId, runId))
        : undefined,
    [runId]
  )
  const threadId = ((itemRows ?? []) as SyncedThreadItem[])[0]?.threadId ?? null
  const { data: threadRows } = useLiveQuery(
    (query) =>
      threadId
        ? query.from({ t: threadsCollection }).where(({ t }) => eq(t.id, threadId))
        : undefined,
    [threadId]
  )
  return ((threadRows ?? []) as Thread[])[0] ?? null
}

/** The thread of EVERY run among `runIds` (run id → thread), for the Guide
 *  readers that pick among several runs (rule 8: `thread.results ??
 *  run.results`). */
export function useThreadsOfRuns(runIds: readonly string[]): Map<string, Thread> {
  const key = sortedKey(runIds)
  const { data: itemRows } = useLiveQuery(
    (query) =>
      key.length > 0
        ? query
            .from({ it: threadItemsCollection })
            .where(({ it }) => inArray(it.sessionId, key))
        : undefined,
    [key.join(`,`)]
  )
  const items = useMemo(() => (itemRows ?? []) as SyncedThreadItem[], [itemRows])
  const threadIds = useMemo(() => sortedKey(items.map((item) => item.threadId)), [items])
  const { data: threadRows } = useLiveQuery(
    (query) =>
      threadIds.length > 0
        ? query.from({ t: threadsCollection }).where(({ t }) => inArray(t.id, threadIds))
        : undefined,
    [threadIds.join(`,`)]
  )
  return useMemo(() => {
    const byId = new Map(((threadRows ?? []) as Thread[]).map((row) => [row.id, row]))
    const out = new Map<string, Thread>()
    for (const item of items) {
      const thread = item.sessionId ? byId.get(item.threadId) : undefined
      if (thread && item.sessionId) out.set(item.sessionId, thread)
    }
    return out
  }, [items, threadRows])
}

/** The threads holding an issue (the issue page's `Threads` row), newest
 *  activity first. */
export function useThreadsOfIssue(
  teamId: string | undefined,
  currentUserId: string | undefined,
  issueId: string | undefined
): ThreadBundle[] {
  const { data: itemRows } = useLiveQuery(
    (query) =>
      issueId
        ? query
            .from({ it: threadItemsCollection })
            .where(({ it }) => eq(it.issueId, issueId))
        : undefined,
    [issueId]
  )
  const threadIds = useMemo(
    () => sortedKey(((itemRows ?? []) as SyncedThreadItem[]).map((item) => item.threadId)),
    [itemRows]
  )
  const { bundles } = useThreadBundles(teamId, currentUserId, { threadIds })
  return useMemo(
    () =>
      [...bundles].sort(
        (a, b) => Date.parse(iso(b.thread.updatedAt)) - Date.parse(iso(a.thread.updatedAt))
      ),
    [bundles]
  )
}

/** The thread a run sits in, read straight off the synced collection (no
 *  hook) — the New run tab's landing, which runs inside a callback. */
export function threadIdOfRun(runId: string): string | null {
  for (const item of threadItemsCollection.state.values()) {
    if ((item as SyncedThreadItem).sessionId === runId) {
      return (item as SyncedThreadItem).threadId
    }
  }
  return null
}

/** Waits (polling the synced collection) for a just-started run's thread
 *  item to land; null after `timeoutMs`. */
export function waitForThreadOfRun(runId: string, timeoutMs = 6_000): Promise<string | null> {
  const found = threadIdOfRun(runId)
  if (found) return Promise.resolve(found)
  return new Promise((resolve) => {
    const started = Date.now()
    const timer = setInterval(() => {
      const threadId = threadIdOfRun(runId)
      if (threadId || Date.now() - started > timeoutMs) {
        clearInterval(timer)
        resolve(threadId)
      }
    }, 150)
  })
}

/** EXP-1284: first-render times per child id, so a child the viewer has not
 *  seen before can fade in with a ring exactly once per page mount. */
export function useFirstSeen(ids: readonly string[]): Map<string, number> {
  const seen = useRef(new Map<string, number>())
  const now = Date.now()
  for (const id of ids) if (!seen.current.has(id)) seen.current.set(id, now)
  return seen.current
}
