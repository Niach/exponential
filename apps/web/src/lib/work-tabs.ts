// EXP-870: the web's WORK TABS — browser-like tabs above the content card on
// md+, the twin of the desktop title-band strip (apps/desktop/crates/ui/src/
// screens.rs `TabKey`/`partition_tabs`).
//
// EXP-1293: the strip is ROUTE-DRIVEN and nothing else. A tab is
// a WORK ITEM the URL opened — an issue, a chat (a root session at `/sessions/$id`), a
// team agent's channel (`/agents/$id`) or one raw session of an agent's tree
// (`/sessions/$id`) — and
// no other screen calls into the store:
//
//   * opening a work item focuses its tab, creating it at the end when absent
//     (`upsertFromRoute`); the ACTIVE tab is derived from the URL, never
//     stored;
//   * the strip's "+" (and Alt/Option+T) opens ONE "New session" tab
//     (`kind: 'new'`) hosting the home composer (the Agent page). A
//     navigation that LEAVES that page for a work item lands IN that tab,
//     browser-like: the composer's start goes to its chat, and the chat
//     takes the New session tab's place (`landing`);
//   * the sidebar's Running rows navigate with the `running` origin
//     (`TABLESS_ORIGIN`), which creates NO tab — the row IS the affordance;
//     it only focuses a tab that is already open;
//   * every tab closes; closing never stops a session.
//
// #EXP-2027: md+ has NO active team, so the tabs are the PERSON's, one
// list across every member team: each work-item tab carries the team of its
// entity (`/t/<slug>` of the route that opened it: an issue's board's team,
// a session's, an agent's) and links with that team's slug. The New session
// tab is Home's (`/home`), team-less.
//
// #EXP-2124 (sidebar-menus.json `tabs`): tabs hold ANY page, by the same
// rules: Needs you, Chats and Drafts (`page`, team-less), a device page
// (`device`, team-less), a board page and an app (`board`, `app`, each with
// its team). Home and the New session tab are ONE tab (key `new`): visiting
// Home opens or activates it, and its chip is Home's. A row's "Open in new
// tab" adds a tab in the BACKGROUND (`openTabInBackground`), never twice.
//
// Pure: no React, no storage, no router — every rule is a test
// (`work-tabs.test.ts`). The store is `hooks/use-work-tabs.ts`, the URL→tab
// wiring `components/team/work-tabs-sync.tsx`.

/** #EXP-2027: the team a work-item tab belongs to. `teamSlug` is the slug
 *  it was opened under (`""` for a tab migrated from a per-team store);
 *  links prefer the team's current slug by id. */
export interface TabTeam {
  teamId: string
  teamSlug: string
}

export type WorkTab =
  | ({
      kind: `issue`
      issueId: string
      /** The `?from=` token the tab reopens with. */
      from: string | null
    } & TabTeam)
  | ({
      /** A chat = a root session (`/sessions/$id`, no agent). */
      kind: `chat`
      chatId: string
      from: string | null
    } & TabTeam)
  | ({
      /** A team agent's page (NEW-2: an `agents` row id; the page shows
       *  the caller's conversation session with it). */
      kind: `agent`
      agentId: string
      from: string | null
    } & TabTeam)
  | ({
      /** One raw session (any node of a tree). */
      kind: `session`
      sessionId: string
      from: string | null
    } & TabTeam)
  | {
      /** EXP-1255: the New session tab — the home composer, no work item.
       *  #EXP-2124: Home's tab (Home and the New session tab are one). */
      kind: `new`
      from: null
    }
  | {
      /** #EXP-2124: a team-less list page (`/needs-you`, `/chats`, `/drafts`). */
      kind: `page`
      page: WorkTabPage
      from: null
    }
  | {
      /** #EXP-2124: a device's page (`/devices/$deviceId`, the LOGICAL id). */
      kind: `device`
      deviceId: string
      from: null
    }
  | ({
      /** #EXP-2124: a board's page (its issues stay issue tabs). */
      kind: `board`
      boardId: string
      from: null
    } & TabTeam)
  | ({
      /** #EXP-2124: an app's run view (any of its screens; not the editor). */
      kind: `app`
      appId: string
      from: null
    } & TabTeam)

/** #EXP-2124: the team-less list pages a tab holds. */
export const WORK_TAB_PAGES = [`needs-you`, `chats`, `drafts`] as const
export type WorkTabPage = (typeof WORK_TAB_PAGES)[number]

function isWorkTabPage(value: unknown): value is WorkTabPage {
  return (WORK_TAB_PAGES as readonly unknown[]).includes(value)
}

/** The team a tab belongs to; null for Home's and the team-less pages'. */
export function tabTeam(tab: WorkTab): TabTeam | null {
  return `teamId` in tab ? { teamId: tab.teamId, teamSlug: tab.teamSlug } : null
}

/** EXP-1255: the one New session tab. */
export const NEW_RUN_TAB: Extract<WorkTab, { kind: `new` }> = {
  kind: `new`,
  from: null,
}
export const NEW_RUN_TAB_KEY = `new`

export interface WorkTabsState {
  tabs: WorkTab[]
}

export const EMPTY_WORK_TABS: WorkTabsState = { tabs: [] }

/** A tab's identity — one tab per work item. */
export function tabKey(tab: WorkTab): string {
  switch (tab.kind) {
    case `issue`:
      return `issue:${tab.issueId}`
    case `chat`:
      return `chat:${tab.chatId}`
    case `agent`:
      return `agent:${tab.agentId}`
    case `session`:
      return `session:${tab.sessionId}`
    case `new`:
      return NEW_RUN_TAB_KEY
    case `page`:
      return `page:${tab.page}`
    case `device`:
      return `device:${tab.deviceId}`
    case `board`:
      return `board:${tab.boardId}`
    case `app`:
      return `app:${tab.appId}`
  }
}

/** The session row a tab names (a chat or a session), if any. NEW-2: an
 *  agent tab names an `agents` row, never a session. */
export function tabSessionId(tab: WorkTab): string | null {
  switch (tab.kind) {
    case `chat`:
      return tab.chatId
    case `session`:
      return tab.sessionId
    default:
      return null
  }
}

// ── The route side ───────────────────────────────────────────────────────────

/** What the current URL names, before any collection lookup. `teamSlug` =
 *  the route's `/t/<slug>`, the entity's team (#EXP-2027). */
export type RoutePath =
  | { kind: `issue`; teamSlug: string; boardSlug: string; identifier: string; from: string | null }
  | { kind: `agent`; teamSlug: string; agentId: string; from: string | null }
  | { kind: `session`; teamSlug: string; sessionId: string; from: string | null }
  // #EXP-2124: the pages a tab holds.
  | { kind: `page`; page: WorkTabPage; from: null }
  | { kind: `device`; deviceId: string; from: null }
  | { kind: `board`; teamSlug: string; boardSlug: string; from: null }
  | { kind: `app`; teamSlug: string; appSlug: string; from: null }

/** A route path that names a team (`/t/<slug>/…`). */
export type TeamRoutePath = Extract<RoutePath, { teamSlug: string }>

/** The work item or page a pathname shows — `null` for every other screen
 * (settings, creation pages, the app editor, Home: `isNewRunPath`).
 * #EXP-2124: Needs you, Chats, Drafts, a device's page, a board's page and
 * an app's run view (any screen) are tabs too. `view` = the `?view=` search value:
 * `/sessions/<id>?view=guide` only redirects to the chat's conversation, so
 * it names no tab. A `/sessions/<id>` path is resolved against the row
 * (`WorkTabsSync`): a root = its chat tab, an agent's child = a session tab. */
export function routePathFromLocation(
  pathname: string,
  from: string | null | undefined,
  view?: string | null
): RoutePath | null {
  // A hand-typed path with a broken `%` escape is simply not a work item —
  // never an exception in the team layout.
  const decode = (value: string) => {
    try {
      return decodeURIComponent(value)
    } catch {
      return null
    }
  }
  const page = pathname.match(/^\/([^/]+)\/?$/)
  if (page && isWorkTabPage(page[1])) return { kind: `page`, page: page[1], from: null }
  const device = pathname.match(/^\/devices\/([^/]+)\/?$/)
  if (device) {
    const deviceId = decode(device[1])
    return deviceId === null ? null : { kind: `device`, deviceId, from: null }
  }
  const match = pathname.match(/^\/t\/([^/]+)(\/.*)?$/)
  const rest = match?.[2]?.replace(/\/$/, ``)
  if (!match || !rest) return null
  const token = from ? from : null
  const teamSlug = decode(match[1])
  if (teamSlug === null) return null
  const issue = rest.match(/^\/boards\/([^/]+)\/issues\/([^/]+)$/)
  if (issue) {
    const boardSlug = decode(issue[1])
    const identifier = decode(issue[2])
    if (boardSlug === null || identifier === null) return null
    return { kind: `issue`, teamSlug, boardSlug, identifier, from: token }
  }
  const board = rest.match(/^\/boards\/([^/]+)$/)
  if (board) {
    const boardSlug = decode(board[1])
    return boardSlug === null ? null : { kind: `board`, teamSlug, boardSlug, from: null }
  }
  const app = rest.match(/^\/apps\/([^/]+)(?:\/[^/]+)?$/)
  if (app) {
    const appSlug = decode(app[1])
    return appSlug === null ? null : { kind: `app`, teamSlug, appSlug, from: null }
  }
  const single = rest.match(/^\/(agents|sessions)\/([^/]+)$/)
  if (single) {
    const id = decode(single[2])
    if (id === null) return null
    if (single[1] === `agents`) return { kind: `agent`, teamSlug, agentId: id, from: token }
    if (view === `guide`) return null
    return { kind: `session`, teamSlug, sessionId: id, from: token }
  }
  return null
}

/** A route resolved against the collections — an issue's id; a session
 *  route's row exists in the route's team, which the tab carries. */
export type RouteTab = Exclude<WorkTab, { kind: `new` }>

/** The tab a resolved route shows. */
export function routeTab(route: RouteTab): Exclude<WorkTab, { kind: `new` }> {
  return { ...route } as Exclude<WorkTab, { kind: `new` }>
}

/** The key of the tab a resolved route shows — the active tab. */
export function routeTabKey(route: RouteTab): string {
  return tabKey(routeTab(route))
}

/** Whether the URL is the New session tab's page: Home (#EXP-2027), or
 *  the old team-scoped Agent page that forwards there. */
export function isNewRunPath(pathname: string): boolean {
  return /^\/home\/?$/.test(pathname) || /^\/t\/[^/]+\/agent\/?$/.test(pathname)
}

/** The New session tab's page. */
export const NEW_RUN_PATH = `/home`

/** EXP-923: the `?from=` token the sidebar's Running rows navigate with. A
 * navigation carrying it creates NO tab — the sidebar row IS the affordance.
 * It still FOCUSES a tab that is already open. */
export const TABLESS_ORIGIN = `running`

/** Whether a navigation's origin may create a tab. */
export function originCreatesTab(from: string | null | undefined): boolean {
  return from !== TABLESS_ORIGIN
}

/**
 * The URL opened a work item: focus its tab, creating it when absent. An
 * EXPLICIT `from` on the route overwrites the stored one (a tab reopened from
 * another list follows that list); a route without one keeps what the tab had.
 *
 * `landing` = the navigation LEFT the New session tab's page (the Agent page
 * with the New session tab open): a work item with no tab yet takes that
 * tab's place — browser-like, the composer's start lands IN the tab it was
 * typed in. One that has a tab already just focuses it, and a `running`
 * origin lands nothing: the New session tab stays.
 *
 * EXP-923: a `running`-origin route creates nothing and never restamps a
 * tab's stored origin.
 */
export function upsertFromRoute(
  state: WorkTabsState,
  route: RouteTab,
  options: { landing?: boolean } = {}
): WorkTabsState {
  const tab = routeTab(route)
  const key = tabKey(tab)
  const tabs = [...state.tabs]
  const at = tabs.findIndex((candidate) => tabKey(candidate) === key)
  const creates = originCreatesTab(route.from)
  // Only an item with no tab lands: switching to an open tab (or a Running
  // row, which creates none) leaves the New session tab where it is.
  const newAt =
    options.landing && at < 0 && creates
      ? tabs.findIndex((candidate) => candidate.kind === `new`)
      : -1
  if (newAt >= 0) {
    tabs[newAt] = tab
    return { ...state, tabs }
  }

  if (at >= 0) {
    const current = tabs[at] as RouteTab
    const restamp = creates && route.from !== null && current.from !== route.from
    // #EXP-2027: a tab migrated without its slug (or whose team was renamed)
    // takes the route's.
    const routeTeam = tabTeam(route)
    const currentTeam = tabTeam(current)
    const reteam =
      routeTeam !== null &&
      (currentTeam?.teamId !== routeTeam.teamId || currentTeam?.teamSlug !== routeTeam.teamSlug)
    if (!restamp && !reteam) return state
    tabs[at] = {
      ...current,
      ...(restamp ? { from: route.from } : {}),
      ...(reteam ? routeTeam : {}),
    } as WorkTab
    return { ...state, tabs }
  }
  if (!creates) return state
  tabs.push(tab)
  return { ...state, tabs }
}

// ── The New session tab (EXP-1255) ───────────────────────────────────────────

/** "+" / Alt+T: open the New session tab at the end — or keep the one that
 * is already open (there is only ever one). */
export function openNewRunTab(state: WorkTabsState): WorkTabsState {
  if (state.tabs.some((tab) => tab.kind === `new`)) return state
  return { ...state, tabs: [...state.tabs, NEW_RUN_TAB] }
}

/** #EXP-2124: "Open in new tab": the tab at the end, in the BACKGROUND (no
 * navigation; the current tab stays). A tab that is already open is not
 * opened twice (the same state); Home's is the one New session tab. */
export function openTabInBackground(state: WorkTabsState, tab: WorkTab): WorkTabsState {
  if (tab.kind === `new`) return openNewRunTab(state)
  const key = tabKey(tab)
  if (state.tabs.some((candidate) => tabKey(candidate) === key)) return state
  return { ...state, tabs: [...state.tabs, tab] }
}

/** Close tabs. Closing never stops a session. */
export function closeTabs(
  state: WorkTabsState,
  keys: readonly string[]
): WorkTabsState {
  const closing = new Set(keys)
  if (!state.tabs.some((tab) => closing.has(tabKey(tab)))) return state
  return { tabs: state.tabs.filter((tab) => !closing.has(tabKey(tab))) }
}

/** Drop tabs whose target no longer resolves (a deleted issue, a session of a
 * team the user left). Only call once the collections are READY — a cold
 * load must not wipe tabs whose rows simply have not synced yet. */
export function pruneTabs(
  state: WorkTabsState,
  resolves: (tab: WorkTab) => boolean
): WorkTabsState {
  const tabs = state.tabs.filter(resolves)
  return tabs.length === state.tabs.length ? state : { ...state, tabs }
}

/** Where a tab click goes — with the TAB's own team (#EXP-2027).
 * `resolveIssue` resolves an issue id to its board slug and identifier (null
 * while it syncs — the tab then waits); `teamSlugOf` a team id to its current
 * slug (null = unknown: the slug the tab was opened under, if any);
 * `boardSlugOf` / `appSlugOf` a board's / an app's id to its slug (null =
 * unknown: the tab waits). */
export function tabHref(
  tab: WorkTab,
  resolveIssue: (issueId: string) => { boardSlug: string; identifier: string } | null,
  teamSlugOf: (teamId: string) => string | null = () => null,
  boardSlugOf: (boardId: string) => string | null = () => null,
  appSlugOf: (appId: string) => string | null = () => null
): { to: string; params: Record<string, string>; search: { from?: string } } | null {
  if (tab.kind === `new`) return { to: NEW_RUN_PATH, params: {}, search: {} }
  if (tab.kind === `page`) return { to: `/${tab.page}`, params: {}, search: {} }
  if (tab.kind === `device`) {
    return { to: `/devices/$deviceId`, params: { deviceId: tab.deviceId }, search: {} }
  }
  const search = tab.from ? { from: tab.from } : {}
  const teamSlug = teamSlugOf(tab.teamId) ?? (tab.teamSlug || null)
  if (!teamSlug) return null
  switch (tab.kind) {
    case `chat`:
      return {
        to: `/t/$teamSlug/sessions/$sessionId`,
        params: { teamSlug, sessionId: tab.chatId },
        search,
      }
    case `agent`:
      return {
        to: `/t/$teamSlug/agents/$agentId`,
        params: { teamSlug, agentId: tab.agentId },
        search,
      }
    case `session`:
      return {
        to: `/t/$teamSlug/sessions/$sessionId`,
        params: { teamSlug, sessionId: tab.sessionId },
        search,
      }
    case `board`: {
      const boardSlug = boardSlugOf(tab.boardId)
      if (!boardSlug) return null
      return { to: `/t/$teamSlug/boards/$boardSlug`, params: { teamSlug, boardSlug }, search: {} }
    }
    case `app`: {
      const appSlug = appSlugOf(tab.appId)
      if (!appSlug) return null
      return { to: `/t/$teamSlug/apps/$appSlug`, params: { teamSlug, appSlug }, search: {} }
    }
    case `issue`: {
      const issue = resolveIssue(tab.issueId)
      if (!issue) return null
      return {
        to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
        params: {
          teamSlug,
          boardSlug: issue.boardSlug,
          issueIdentifier: issue.identifier,
        },
        search,
      }
    }
  }
}

// ── The strip's overflow ─────────────────────────────────────────────────────

/**
 * Which tabs get a chip, in strip order — the rest collapse into the "+N"
 * dropdown. EXP-1334: ONE rule, the contract's (`partitionTabs`, the
 * desktop's `partition_tabs` vectors).
 */
export { partitionTabs } from "@exp/domain-contract/session-tree"

// ── Persistence ──────────────────────────────────────────────────────────────

/** EXP-1293: v2 — the v1 blobs named coding-session runs, which are gone.
 *  EXP-1733: v3 — chat tabs are `chat` tabs (the old kind is dropped).
 *  #EXP-2027: v4 — ONE store for the person, every tab carrying its team.
 *  #EXP-2124: v5 — page, device, board and app tabs. */
export const WORK_TABS_STORAGE_KEY = `exp:work-tabs:v5`

/** The v4 blob `migrateLegacyWorkTabs` merges (its tabs parse unchanged). */
export const PREVIOUS_WORK_TABS_STORAGE_KEY = `exp:work-tabs:v4`

/** The v3 per-team keys `migrateLegacyWorkTabs` merges. */
export const LEGACY_WORK_TABS_PREFIX = `exp:work-tabs:v3:`
export function legacyWorkTabsStorageKey(teamId: string): string {
  return `${LEGACY_WORK_TABS_PREFIX}${teamId}`
}

function str(value: unknown): value is string {
  return typeof value === `string` && value !== ``
}

function optStr(value: unknown): string | null {
  return str(value) ? value : null
}

/** The tab kinds that carry no team. */
const TEAMLESS_KINDS: readonly unknown[] = [`new`, `page`, `device`]

/** One stored tab. `team` = the team a v3 per-team blob was keyed on (its
 *  tabs carried none); a v4/v5 team tab without its team is dropped. */
function parseTab(value: unknown, team?: string): WorkTab | null {
  if (typeof value !== `object` || value === null) return null
  const raw = value as Record<string, unknown>
  const teamId = team ?? (str(raw.teamId) ? raw.teamId : null)
  if (!TEAMLESS_KINDS.includes(raw.kind) && !teamId) return null
  const owner: TabTeam = {
    teamId: teamId ?? ``,
    teamSlug: team === undefined && str(raw.teamSlug) ? raw.teamSlug : ``,
  }
  switch (raw.kind) {
    case `issue`:
      return str(raw.issueId)
        ? { kind: `issue`, issueId: raw.issueId, from: optStr(raw.from), ...owner }
        : null
    case `chat`:
      return str(raw.chatId)
        ? { kind: `chat`, chatId: raw.chatId, from: optStr(raw.from), ...owner }
        : null
    case `agent`:
      return str(raw.agentId)
        ? { kind: `agent`, agentId: raw.agentId, from: optStr(raw.from), ...owner }
        : null
    case `session`:
      return str(raw.sessionId)
        ? { kind: `session`, sessionId: raw.sessionId, from: optStr(raw.from), ...owner }
        : null
    case `new`:
      return NEW_RUN_TAB
    case `page`:
      return isWorkTabPage(raw.page) ? { kind: `page`, page: raw.page, from: null } : null
    case `device`:
      return str(raw.deviceId) ? { kind: `device`, deviceId: raw.deviceId, from: null } : null
    case `board`:
      return str(raw.boardId) ? { kind: `board`, boardId: raw.boardId, from: null, ...owner } : null
    case `app`:
      return str(raw.appId) ? { kind: `app`, appId: raw.appId, from: null, ...owner } : null
    default:
      return null
  }
}

/** Read a stored state back. Anything malformed is dropped — a corrupt or
 * foreign blob is an empty strip, never a crash. Duplicate keys keep the
 * first. */
export function parseWorkTabsState(raw: string | null | undefined): WorkTabsState {
  return mergeTabs([{ raw }])
}

function rawTabs(raw: string | null | undefined): unknown[] {
  if (!raw) return []
  let json: unknown
  try {
    json = JSON.parse(raw)
  } catch {
    return []
  }
  if (typeof json !== `object` || json === null) return []
  const tabs = (json as { tabs?: unknown }).tabs
  return Array.isArray(tabs) ? tabs : []
}

function mergeTabs(sources: readonly { raw: string | null | undefined; teamId?: string }[]): WorkTabsState {
  const seen = new Set<string>()
  const tabs: WorkTab[] = []
  for (const source of sources) {
    for (const entry of rawTabs(source.raw)) {
      const tab = parseTab(entry, source.teamId)
      if (!tab || seen.has(tabKey(tab))) continue
      seen.add(tabKey(tab))
      tabs.push(tab)
    }
  }
  return tabs.length === 0 ? EMPTY_WORK_TABS : { tabs }
}

/** #EXP-2027: the one store from the current blob plus every v3 per-team
 *  blob (`legacy`, each tagged with the team it was keyed on). #EXP-2124:
 *  the current (v5) tabs first, then the v4 blob's (`previous`), then each
 *  team's in the given order; duplicates keep the first, so there is still
 *  one New session tab. */
export function migrateLegacyWorkTabs(
  current: string | null | undefined,
  legacy: readonly { teamId: string; raw: string | null | undefined }[],
  previous: string | null | undefined = null
): WorkTabsState {
  return mergeTabs([
    { raw: current },
    { raw: previous },
    ...legacy.filter((entry) => entry.teamId !== ``),
  ])
}
