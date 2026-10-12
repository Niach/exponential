import { useEffect, useMemo, useRef } from "react"
import { useNavigate, useParams, useRouterState } from "@tanstack/react-router"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import type { Board, CodingSession, Issue, Thread } from "@/db/schema"
import {
  codingSessionCollection,
  issueCollection,
  threadItemsCollection,
  threadsCollection,
} from "@/lib/collections"
import {
  closeMergedRunTabs,
  pruneTabs,
  reconcileLive,
  routePathFromLocation,
  upsertFromRoute,
  type EndedRun,
  type LiveRun,
  type RouteTab,
} from "@/lib/work-tabs"
import { useMyLiveRuns } from "@/hooks/use-my-live-runs"
import {
  openNewRunPage,
  updateWorkTabs,
  useWorkTabs,
} from "@/hooks/use-work-tabs"

// EXP-870: the ONE writer of the work tabs, mounted once in the team layout
// (md+ only — the strip is hidden on phones). Three jobs:
//
//   1. URL → tab: the open issue / run / support conversation, resolved
//      against the collections (an issue's id, a run's issue), is upserted
//      (`upsertFromRoute`). The active tab is never stored: the strip derives
//      it from the same URL.
//   2. Live runs → tab BINDINGS: every live run of mine in the team is
//      reconciled against the open tabs (`reconcileLive`), on every change and
//      on a 30s clock so a stale heartbeat drops out. EXP-923: this no longer
//      ADDS anything — a live run is a sidebar row, not a chip; the reconcile
//      only binds a run under a tab that is already open and marks it live.
//   3. Merge closes (EXP-923): a run that drops out of the live set because
//      its PR MERGED takes its tab (and any other tab of that issue) with it.
//      Only the TRANSITION closes, so reopening the issue afterwards works.
//   4. Prune: a tab whose issue or run is gone from its (READY) collection is
//      dropped — never while a collection is still syncing, so a cold load
//      cannot wipe the strip. The New run tab (EXP-1255) has nothing to lose.
//   5. EXP-1255: Alt/Option+T opens the New run tab (the strip's "+"), even
//      with no strip up yet. Browsers keep Cmd/Ctrl+T (and Cmd/Ctrl+W) for
//      their own tabs and never hand them to a page, so the web's chord is
//      Alt/Option+T.
//   6. EXP-1284: a THREAD route is a tab of its own; a live run binds its
//      thread's tab through the run's item (`LiveRun.threadId`).

export function WorkTabsSync({
  teamId,
  userId,
  boards,
}: {
  teamId: string
  userId: string | undefined
  boards: Board[] | undefined
}) {
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const from = useRouterState({
    select: (s) => {
      const value = (s.location.search as { from?: unknown }).from
      return typeof value === `string` ? value : null
    },
  })
  const path = useMemo(() => routePathFromLocation(pathname, from), [pathname, from])

  const issueBoard =
    path?.kind === `issue`
      ? (boards?.find((board) => board.slug === path.boardSlug) ?? null)
      : null
  const issueIdentifier = path?.kind === `issue` ? path.identifier : null
  const { data: issueRows } = useLiveQuery(
    (query) =>
      issueBoard && issueIdentifier
        ? query
            .from({ i: issueCollection })
            .where(({ i }) =>
              and(eq(i.boardId, issueBoard.id), eq(i.identifier, issueIdentifier))
            )
        : undefined,
    [issueBoard?.id, issueIdentifier]
  )
  const runId = path?.kind === `run` ? path.runId : null
  const { data: runRows } = useLiveQuery(
    (query) =>
      runId
        ? query
            .from({ s: codingSessionCollection })
            .where(({ s }) => eq(s.id, runId))
        : undefined,
    [runId]
  )

  const threadId = path?.kind === `thread` ? path.threadId : null
  const { data: threadRows } = useLiveQuery(
    (query) =>
      threadId
        ? query
            .from({ t: threadsCollection })
            .where(({ t }) => eq(t.id, threadId))
        : undefined,
    [threadId]
  )

  const route = useMemo<RouteTab | null>(() => {
    if (!path) return null
    if (path.kind === `thread`) {
      const thread = ((threadRows ?? []) as Thread[])[0]
      if (!thread || thread.id !== path.threadId || thread.teamId !== teamId) return null
      return { kind: `thread`, threadId: thread.id, from: path.from }
    }
    if (path.kind === `issue`) {
      const issue = ((issueRows ?? []) as Issue[])[0]
      return issue ? { kind: `issue`, issueId: issue.id, from: path.from } : null
    }
    const run = ((runRows ?? []) as CodingSession[])[0]
    if (!run || run.id !== path.runId || run.teamId !== teamId) return null
    return { kind: `run`, runId: run.id, issueId: run.issueId, from: path.from }
  }, [path, issueRows, runRows, threadRows, teamId])
  const routeKey = route ? JSON.stringify(route) : null

  useEffect(() => {
    if (!route) return
    updateWorkTabs(teamId, (state) => upsertFromRoute(state, route))
    // `routeKey` is the route's value identity.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [teamId, routeKey])

  const viewedRunId = path?.kind === `run` ? path.runId : null
  // EXP-902: the issue whose Issue face is up — a run binding under its tab
  // must not flip the page being read (`reconcileLive`).
  const viewedIssueId = route?.kind === `issue` ? route.issueId : null
  const { runs, isReady, now } = useMyLiveRuns(teamId, userId, 30_000)
  // EXP-1284: each live run's thread, off its synced run item.
  const liveRunIds = useMemo(() => runs.map((run) => run.id).sort(), [runs])
  const { data: liveItemRows } = useLiveQuery(
    (query) =>
      liveRunIds.length > 0
        ? query
            .from({ it: threadItemsCollection })
            .where(({ it }) => inArray(it.sessionId, liveRunIds))
        : undefined,
    [liveRunIds.join(`,`)]
  )
  const live = useMemo<LiveRun[]>(() => {
    const threadOf = new Map(
      ((liveItemRows ?? []) as { sessionId: string | null; threadId: string }[]).map(
        (item) => [item.sessionId, item.threadId]
      )
    )
    return runs.map((run) => ({
      runId: run.id,
      issueId: run.issueId,
      threadId: threadOf.get(run.id) ?? null,
    }))
  }, [runs, liveItemRows])
  const liveKey = JSON.stringify(live)
  const { tabs } = useWorkTabs(teamId)

  useEffect(() => {
    if (!isReady) return
    updateWorkTabs(teamId, (state) =>
      reconcileLive(state, live, viewedRunId, viewedIssueId)
    )
    // `liveKey` is the runs' value identity.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [teamId, liveKey, isReady, viewedRunId, viewedIssueId])

  // EXP-923: the merge close. Only a run that WAS live here and is not any
  // more is looked up — a run already ended when this mounted never closes a
  // tab you opened afterwards.
  const wasLiveRef = useRef<Set<string> | null>(null)
  useEffect(() => {
    if (!isReady) return
    const now = new Set(live.map((run) => run.runId))
    const before = wasLiveRef.current
    wasLiveRef.current = now
    if (!before) return
    const ended: EndedRun[] = []
    for (const runId of before) {
      if (now.has(runId)) continue
      const row = codingSessionCollection.get(runId)
      if (!row || row.teamId !== teamId) continue
      ended.push({
        runId,
        issueId: row.issueId,
        endedBy: row.endedBy,
      })
    }
    if (ended.length === 0) return
    updateWorkTabs(teamId, (state) => closeMergedRunTabs(state, ended))
    // `liveKey` is the runs' value identity.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [teamId, liveKey, isReady])

  // Prune against the collections themselves (not a live query whose deps
  // lag the tab list by a render — that would drop a tab the instant it is
  // added). Re-checked whenever the tabs change and on the 30s clock.
  // The issues shape carries no `team_id` (a scoping column), so an issue
  // belongs to the team through its board — and nothing is pruned before the
  // boards themselves have synced.
  const boardIds = useMemo(
    () => new Set((boards ?? []).map((board) => board.id)),
    [boards]
  )
  const navigate = useNavigate()
  const { teamSlug } = useParams({ strict: false })
  useEffect(() => {
    if (!teamSlug) return
    const onKeyDown = (event: KeyboardEvent) => {
      // `code`, not `key`: Option+T types "†" on a Mac keyboard.
      if (event.code !== `KeyT` || !event.altKey) return
      if (event.metaKey || event.ctrlKey || event.shiftKey) return
      event.preventDefault()
      openNewRunPage(teamId, teamSlug, navigate)
    }
    window.addEventListener(`keydown`, onKeyDown)
    return () => window.removeEventListener(`keydown`, onKeyDown)
  }, [teamId, teamSlug, navigate])

  useEffect(() => {
    if (!issueCollection.isReady() || !codingSessionCollection.isReady()) return
    if (boardIds.size === 0) return
    updateWorkTabs(teamId, (state) =>
      pruneTabs(state, (tab) => {
        if (tab.kind === `new`) return true
        if (tab.kind === `issue`) {
          const issue = issueCollection.get(tab.issueId)
          return issue !== undefined && boardIds.has(issue.boardId)
        }
        if (tab.kind === `thread`) {
          // Nothing is pruned before the threads snapshot itself landed.
          if (!threadsCollection.isReady()) return true
          return threadsCollection.get(tab.threadId)?.teamId === teamId
        }
        return codingSessionCollection.get(tab.runId)?.teamId === teamId
      })
    )
  }, [teamId, tabs, now, boardIds])

  return null
}
