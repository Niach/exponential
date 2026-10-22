import { useEffect, useMemo, useRef } from "react"
import { useNavigate, useParams, useRouterState } from "@tanstack/react-router"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import type { Board, Issue } from "@/db/schema"
import { agentCollection, issueCollection, sessionCollection } from "@/lib/collections"
import {
  isNewRunPath,
  pruneTabs,
  routePathFromLocation,
  tabSessionId,
  upsertFromRoute,
  type RouteTab,
} from "@/lib/work-tabs"
import { useSessionRow } from "@/hooks/use-session-tree"
import { useAgentRow } from "@/hooks/use-agent-row"
import {
  openNewRunPage,
  updateWorkTabs,
  useWorkTabs,
} from "@/hooks/use-work-tabs"

// EXP-870: the ONE writer of the work tabs, mounted once in the team layout
// (md+ only — the strip is hidden on phones). EXP-1293: ROUTE-DRIVEN, so no
// other screen calls into the store:
//
//   1. URL → tab: the open issue, thread, agent channel or raw session,
//      resolved against the collections (an issue's id; a session row of
//      THIS team), is upserted (`upsertFromRoute`). The active tab is never
//      stored: the strip derives it from the same URL.
//   2. The New session tab: a navigation that LEAVES the Agent page for a
//      work item lands in the New session tab (`landing`) — the composer's
//      start navigates to its thread, and that thread takes the tab's place.
//      The flag survives until the route resolves (a fresh thread's row may
//      sync a moment after the navigation).
//   3. Prune: a tab whose issue or session row is gone from its (READY)
//      collection is dropped — never while a collection is still syncing, so
//      a cold load cannot wipe the strip.
//   4. EXP-1255: Alt/Option+T opens the New session tab (the strip's "+").
//      Browsers keep Cmd/Ctrl+T for their own tabs, so the web's chord is
//      Alt/Option+T. Never while typing (a field, an IME composition) or
//      behind a dialog: Option+T there is the person's keystroke.

/** Alt/Option+T outside a field, a composition and any open dialog. */
export function isNewTabChord(event: KeyboardEvent): boolean {
  // `code`, not `key`: Option+T types "†" on a Mac keyboard.
  if (event.code !== `KeyT` || !event.altKey) return false
  if (event.metaKey || event.ctrlKey || event.shiftKey) return false
  if (event.isComposing) return false
  const target = event.target
  if (
    target instanceof HTMLElement &&
    (target.isContentEditable ||
      target.closest(`input, textarea, select, [contenteditable=""], [contenteditable="true"]`) !==
        null)
  ) {
    return false
  }
  return document.querySelector(`[role="dialog"], [role="alertdialog"]`) === null
}

export function WorkTabsSync({
  teamId,
  boards,
}: {
  teamId: string
  boards: Board[] | undefined
}) {
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const from = useRouterState({
    select: (s) => {
      const value = (s.location.search as { from?: unknown }).from
      return typeof value === `string` ? value : null
    },
  })
  const view = useRouterState({
    select: (s) => {
      const value = (s.location.search as { view?: unknown }).view
      return typeof value === `string` ? value : null
    },
  })
  const path = useMemo(
    () => routePathFromLocation(pathname, from, view),
    [pathname, from, view]
  )

  // (2) Did this navigation leave the New session tab's page? Declared
  // BEFORE the upsert so both see the same commit.
  const previousPathRef = useRef(pathname)
  const landingRef = useRef(false)
  useEffect(() => {
    const previous = previousPathRef.current
    if (previous === pathname) return
    previousPathRef.current = pathname
    landingRef.current = isNewRunPath(previous) && path !== null
    // `path` is derived from `pathname`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pathname])

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
  const routeSessionId =
    path?.kind === `thread` ? path.threadId : path?.kind === `session` ? path.sessionId : null
  const { row: sessionRow } = useSessionRow(routeSessionId)
  // NEW-2: an agent tab names the `agents` row.
  const { agent: routeAgent } = useAgentRow(path?.kind === `agent` ? path.agentId : null)
  // EXP-1316: `/sessions/<id>` only redirects into its thread (the session
  // opens in place, the THREAD's tab focuses) unless the tree is an agent's.
  const sessionRootId =
    path?.kind === `session` && sessionRow && sessionRow.rootId && sessionRow.rootId !== sessionRow.id
      ? sessionRow.rootId
      : null
  const { row: sessionRoot } = useSessionRow(sessionRootId)

  const route = useMemo<RouteTab | null>(() => {
    if (!path) return null
    if (path.kind === `issue`) {
      const issue = ((issueRows ?? []) as Issue[])[0]
      return issue ? { kind: `issue`, issueId: issue.id, from: path.from } : null
    }
    if (path.kind === `agent`) {
      return routeAgent && routeAgent.id === path.agentId && routeAgent.teamId === teamId
        ? path
        : null
    }
    if (!sessionRow || sessionRow.id !== routeSessionId || sessionRow.teamId !== teamId) {
      return null
    }
    if (path.kind === `session`) {
      const root = sessionRow.rootId && sessionRow.rootId !== sessionRow.id ? sessionRoot : sessionRow
      if (!root?.agentId) return null
    }
    return path
  }, [path, issueRows, sessionRow, sessionRoot, routeSessionId, routeAgent, teamId])
  const routeKey = route ? JSON.stringify(route) : null

  useEffect(() => {
    if (!route) return
    const landing = landingRef.current
    landingRef.current = false
    updateWorkTabs(teamId, (state) => upsertFromRoute(state, route, { landing }))
    // `routeKey` is the route's value identity.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [teamId, routeKey])

  const navigate = useNavigate()
  const { teamSlug } = useParams({ strict: false })
  useEffect(() => {
    if (!teamSlug) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (!isNewTabChord(event)) return
      event.preventDefault()
      openNewRunPage(teamId, teamSlug, navigate)
    }
    window.addEventListener(`keydown`, onKeyDown)
    return () => window.removeEventListener(`keydown`, onKeyDown)
  }, [teamId, teamSlug, navigate])

  // Prune against the collections themselves (not a live query whose deps
  // lag the tab list by a render — that would drop a tab the instant it is
  // added). The issues shape carries no `team_id`, so an issue belongs to
  // the team through its board — and nothing is pruned before the boards
  // themselves have synced.
  const { tabs } = useWorkTabs(teamId)
  const boardIds = useMemo(
    () => new Set((boards ?? []).map((board) => board.id)),
    [boards]
  )
  useEffect(() => {
    if (!issueCollection.isReady() || !sessionCollection.isReady() || !agentCollection.isReady()) {
      return
    }
    if (boardIds.size === 0) return
    updateWorkTabs(teamId, (state) =>
      pruneTabs(state, (tab) => {
        if (tab.kind === `new`) return true
        if (tab.kind === `issue`) {
          const issue = issueCollection.get(tab.issueId)
          return issue !== undefined && boardIds.has(issue.boardId)
        }
        if (tab.kind === `agent`) return agentCollection.get(tab.agentId)?.teamId === teamId
        const sessionId = tabSessionId(tab)
        return sessionId !== null && sessionCollection.get(sessionId)?.teamId === teamId
      })
    )
  }, [teamId, tabs, boardIds])

  return null
}
