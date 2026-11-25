import { useEffect, useMemo, useRef } from "react"
import { useNavigate, useRouterState } from "@tanstack/react-router"
import { and, eq, isNull, or, useLiveQuery } from "@tanstack/react-db"
import type { App, Board, Device, Issue } from "@/db/schema"
import { isAssistant } from "@exp/domain-contract/agent-settings"
import {
  agentCollection,
  appCollection,
  deviceCollection,
  issueCollection,
  sessionCollection,
  teamCollection,
} from "@/lib/collections"
import {
  isNewRunPath,
  openNewRunTab,
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

/** An agent's tab belongs to the team: the team's own agent, or (#EXP-1984)
 *  the viewer's Assistant, whose personal team is never a synced team
 *  (`agentsInTeam`'s rule). */
type AgentTeamLike = { teamId: string; role?: string | null }
function agentTabInTeam(agent: AgentTeamLike | undefined | null, teamId: string): boolean {
  if (!agent) return false
  return agent.teamId === teamId || (isAssistant(agent) && teamCollection.get(agent.teamId) === undefined)
}

// #EXP-1987: a session tab belongs to the team it runs in, and a session of a
// team the viewer does not sync (the Assistant's tree in their personal team)
// to every team, like the Assistant's own tab.
function sessionInTeam(session: { teamId: string } | undefined | null, teamId: string): boolean {
  if (!session) return false
  return session.teamId === teamId || teamCollection.get(session.teamId) === undefined
}

// EXP-870: the ONE writer of the work tabs, mounted once in the app layout
// (md+ only — the strip is hidden on phones). EXP-1293: ROUTE-DRIVEN, so no
// other screen calls into the store. #EXP-2027: the tabs span every member
// team; a tab's team is the route's `/t/<slug>`, its entity's team:
//
//   1. URL → tab: the open issue, chat, agent channel or raw session,
//      resolved against the collections (an issue's id through the board of
//      the route's team; a session row of the route's team), is upserted
//      with that team (`upsertFromRoute`). #EXP-2124: pages are tabs too:
//      Needs you, Chats, Drafts (no lookup), a device page (its row in
//      `devices`), a board page (the route team's board) and an app's run
//      view (the route team's live app). Settings and creation pages name
//      no tab. The active tab is never stored: the strip derives it from
//      the same URL.
//   2. The New session tab = Home's tab (#EXP-2124): visiting Home opens or
//      activates it. A navigation that LEAVES Home for a work item or a
//      page lands in the New session tab (`landing`) — the composer's
//      start navigates to its chat, and that chat takes the tab's place.
//      The flag survives until the route resolves (a fresh chat's row may
//      sync a moment after the navigation).
//   3. Prune: a tab whose issue, session, board, app or device row is gone
//      from its (READY) collection, or whose team the person left, is
//      dropped — never while a collection is still syncing, so a cold load
//      cannot wipe the strip. Page tabs and Home's never prune.
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

/** A member team, as the tabs need it. */
export type WorkTabsTeam = { id: string; slug: string }

export function WorkTabsSync({
  teams,
  boards,
}: {
  /** Every member team. */
  teams: readonly WorkTabsTeam[]
  /** Every member team's boards. */
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

  // #EXP-2124: Home IS the New session tab: visiting it opens or activates
  // that tab.
  useEffect(() => {
    if (isNewRunPath(pathname)) updateWorkTabs(openNewRunTab)
  }, [pathname])

  // The route's team: the entity's team (a team the person is not in names
  // no tab).
  const routeTeamSlug = path && `teamSlug` in path ? path.teamSlug : null
  const routeTeam =
    routeTeamSlug !== null ? (teams.find((team) => team.slug === routeTeamSlug) ?? null) : null
  const teamId = routeTeam?.id ?? null
  const routeBoard =
    path?.kind === `board` && teamId
      ? (boards?.find((board) => board.teamId === teamId && board.slug === path.boardSlug) ?? null)
      : null
  const routeAppSlug = path?.kind === `app` ? path.appSlug : null
  const { data: appRows } = useLiveQuery(
    (query) =>
      teamId && routeAppSlug
        ? query
            .from({ a: appCollection })
            .where(({ a }) => and(eq(a.teamId, teamId), eq(a.slug, routeAppSlug), isNull(a.deletedAt)))
        : undefined,
    [teamId, routeAppSlug]
  )
  // A device page's param is the LOGICAL id (the row id still resolves).
  const routeDeviceId = path?.kind === `device` ? path.deviceId : null
  const { data: deviceRows } = useLiveQuery(
    (query) =>
      routeDeviceId
        ? query
            .from({ d: deviceCollection })
            .where(({ d }) => or(eq(d.deviceId, routeDeviceId), eq(d.id, routeDeviceId)))
        : undefined,
    [routeDeviceId]
  )
  const issueBoard =
    path?.kind === `issue` && teamId
      ? (boards?.find((board) => board.teamId === teamId && board.slug === path.boardSlug) ?? null)
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
  const routeSessionId = path?.kind === `session` ? path.sessionId : null
  const { row: sessionRow } = useSessionRow(routeSessionId)
  // NEW-2: an agent tab names the `agents` row.
  const { agent: routeAgent } = useAgentRow(path?.kind === `agent` ? path.agentId : null)
  // EXP-1733: `/sessions/<id>` of a ROOT is its chat (the chat's tab); a
  // child only redirects into its root's chat (the session opens in place,
  // the CHAT's tab focuses) unless the tree is an agent's.
  const sessionRootId =
    path?.kind === `session` && sessionRow && sessionRow.rootId && sessionRow.rootId !== sessionRow.id
      ? sessionRow.rootId
      : null
  const { row: sessionRoot } = useSessionRow(sessionRootId)

  const route = useMemo<RouteTab | null>(() => {
    if (!path) return null
    if (path.kind === `page`) return { kind: `page`, page: path.page, from: null }
    if (path.kind === `device`) {
      const rows = (deviceRows ?? []) as Device[]
      const device = rows.find((row) => row.deviceId === path.deviceId) ?? rows[0]
      return device ? { kind: `device`, deviceId: device.deviceId, from: null } : null
    }
    if (!routeTeam) return null
    const team = { teamId: routeTeam.id, teamSlug: routeTeam.slug }
    if (path.kind === `board`) {
      return routeBoard ? { kind: `board`, boardId: routeBoard.id, from: null, ...team } : null
    }
    if (path.kind === `app`) {
      const app = ((appRows ?? []) as App[])[0]
      return app ? { kind: `app`, appId: app.id, from: null, ...team } : null
    }
    if (path.kind === `issue`) {
      const issue = ((issueRows ?? []) as Issue[])[0]
      return issue ? { kind: `issue`, issueId: issue.id, from: path.from, ...team } : null
    }
    if (path.kind === `agent`) {
      return routeAgent && routeAgent.id === path.agentId && agentTabInTeam(routeAgent, team.teamId)
        ? { kind: `agent`, agentId: path.agentId, from: path.from, ...team }
        : null
    }
    if (!sessionRow || sessionRow.id !== routeSessionId || !sessionInTeam(sessionRow, team.teamId)) {
      return null
    }
    const isRoot = !sessionRow.rootId || sessionRow.rootId === sessionRow.id
    if (isRoot) {
      // An agent's conversation root redirects to the agent's page.
      return sessionRow.agentId
        ? null
        : { kind: `chat`, chatId: sessionRow.id, from: path.from, ...team }
    }
    return sessionRoot?.agentId
      ? { kind: `session`, sessionId: path.sessionId, from: path.from, ...team }
      : null
  }, [path, routeTeam, issueRows, sessionRow, sessionRoot, routeSessionId, routeAgent, routeBoard, appRows, deviceRows])
  const routeKey = route ? JSON.stringify(route) : null

  useEffect(() => {
    if (!route) return
    const landing = landingRef.current
    landingRef.current = false
    updateWorkTabs((state) => upsertFromRoute(state, route, { landing }))
    // `routeKey` is the route's value identity.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [routeKey])

  const navigate = useNavigate()
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!isNewTabChord(event)) return
      event.preventDefault()
      openNewRunPage(navigate)
    }
    window.addEventListener(`keydown`, onKeyDown)
    return () => window.removeEventListener(`keydown`, onKeyDown)
  }, [navigate])

  // Prune against the collections themselves (not a live query whose deps
  // lag the tab list by a render — that would drop a tab the instant it is
  // added). The issues shape carries no `team_id`, so an issue belongs to
  // its tab's team through its board — and nothing is pruned before the
  // member teams and their boards themselves have synced.
  const { tabs } = useWorkTabs()
  const boardTeams = useMemo(
    () => new Map((boards ?? []).map((board) => [board.id, board.teamId])),
    [boards]
  )
  const teamIds = useMemo(() => new Set(teams.map((team) => team.id)), [teams])
  useEffect(() => {
    if (!issueCollection.isReady() || !sessionCollection.isReady() || !agentCollection.isReady()) {
      return
    }
    if (teamIds.size === 0 || boardTeams.size === 0) return
    updateWorkTabs((state) =>
      pruneTabs(state, (tab) => {
        if (tab.kind === `new` || tab.kind === `page`) return true
        if (tab.kind === `device`) {
          if (!deviceCollection.isReady()) return true
          return deviceCollection.toArray.some(
            (device) => device.deviceId === tab.deviceId || device.id === tab.deviceId
          )
        }
        if (!teamIds.has(tab.teamId)) return false
        if (tab.kind === `board`) return boardTeams.get(tab.boardId) === tab.teamId
        if (tab.kind === `app`) {
          if (!appCollection.isReady()) return true
          const app = appCollection.get(tab.appId)
          return app !== undefined && !app.deletedAt && app.teamId === tab.teamId
        }
        if (tab.kind === `issue`) {
          const issue = issueCollection.get(tab.issueId)
          return issue !== undefined && boardTeams.get(issue.boardId) === tab.teamId
        }
        if (tab.kind === `agent`) {
          return agentTabInTeam(agentCollection.get(tab.agentId) as AgentTeamLike | undefined, tab.teamId)
        }
        const sessionId = tabSessionId(tab)
        return sessionId !== null && sessionInTeam(sessionCollection.get(sessionId), tab.teamId)
      })
    )
  }, [tabs, boardTeams, teamIds])

  return null
}
