// EXP-870: the web's WORK TABS — browser-like tabs above the content card on
// md+, the twin of the desktop title-band strip (apps/desktop/crates/ui/src/
// screens.rs `TabKey`/`partition_tabs`).
//
// EXP-1293 (Threads v2): the strip is ROUTE-DRIVEN and nothing else. A tab is
// a WORK ITEM the URL opened — an issue, a thread (`/threads/$id`), a team
// agent's channel (`/agents/$id`) or one raw session (`/sessions/$id`) — and
// no other screen calls into the store:
//
//   * opening a work item focuses its tab, creating it at the end when absent
//     (`upsertFromRoute`); the ACTIVE tab is derived from the URL, never
//     stored;
//   * the strip's "+" (and Alt/Option+T) opens ONE "New session" tab
//     (`kind: 'new'`) hosting the home composer (the Agent page). A
//     navigation that LEAVES that page for a work item lands IN that tab,
//     browser-like: the composer's start goes to its thread, and the thread
//     takes the New session tab's place (`landing`);
//   * the sidebar's Running rows navigate with the `running` origin
//     (`TABLESS_ORIGIN`), which creates NO tab — the row IS the affordance;
//     it only focuses a tab that is already open;
//   * the Inbox's list-detail opens share ONE preview slot (`PREVIEW_ORIGINS`):
//     stepping the list replaces that tab in place;
//   * every tab closes; closing never stops a session.
//
// Pure: no React, no storage, no router — every rule is a test
// (`work-tabs.test.ts`). The store is `hooks/use-work-tabs.ts`, the URL→tab
// wiring `components/team/work-tabs-sync.tsx`.

export type WorkTab =
  | {
      kind: `issue`
      issueId: string
      /** The `?from=` token the tab reopens with. */
      from: string | null
    }
  | {
      /** A thread = a root session of kind `thread`. */
      kind: `thread`
      threadId: string
      from: string | null
    }
  | {
      /** A team agent's page (NEW-2: an `agents` row id; the page shows
       *  the caller's conversation session with it). */
      kind: `agent`
      agentId: string
      from: string | null
    }
  | {
      /** One raw session (any node of a tree). */
      kind: `session`
      sessionId: string
      from: string | null
    }
  | {
      /** EXP-1255: the New session tab — the home composer, no work item. */
      kind: `new`
      from: null
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
    case `thread`:
      return `thread:${tab.threadId}`
    case `agent`:
      return `agent:${tab.agentId}`
    case `session`:
      return `session:${tab.sessionId}`
    case `new`:
      return NEW_RUN_TAB_KEY
  }
}

/** The session row a tab names (a thread or a session), if any. NEW-2: an
 *  agent tab names an `agents` row, never a session. */
export function tabSessionId(tab: WorkTab): string | null {
  switch (tab.kind) {
    case `thread`:
      return tab.threadId
    case `session`:
      return tab.sessionId
    default:
      return null
  }
}

// ── The route side ───────────────────────────────────────────────────────────

/** What the current URL names, before any collection lookup. */
export type RoutePath =
  | { kind: `issue`; boardSlug: string; identifier: string; from: string | null }
  | { kind: `thread`; threadId: string; from: string | null }
  | { kind: `agent`; agentId: string; from: string | null }
  | { kind: `session`; sessionId: string; from: string | null }

/** The work item a pathname shows — `null` for every other screen (lists,
 * full pages, settings, the Agent page). `view` = the `?view=` search value:
 * `/sessions/<id>?view=guide` only redirects to the thread's Guide, so it
 * names no tab. */
export function routePathFromLocation(
  pathname: string,
  from: string | null | undefined,
  view?: string | null
): RoutePath | null {
  const rest = pathname.match(/^\/t\/[^/]+(\/.*)?$/)?.[1]?.replace(/\/$/, ``)
  if (!rest) return null
  const token = from ? from : null
  // A hand-typed path with a broken `%` escape is simply not a work item —
  // never an exception in the team layout.
  const decode = (value: string) => {
    try {
      return decodeURIComponent(value)
    } catch {
      return null
    }
  }
  const issue = rest.match(/^\/boards\/([^/]+)\/issues\/([^/]+)$/)
  if (issue) {
    const boardSlug = decode(issue[1])
    const identifier = decode(issue[2])
    if (boardSlug === null || identifier === null) return null
    return { kind: `issue`, boardSlug, identifier, from: token }
  }
  const single = rest.match(/^\/(threads|agents|sessions)\/([^/]+)$/)
  if (single) {
    const id = decode(single[2])
    if (id === null) return null
    if (single[1] === `threads`) return { kind: `thread`, threadId: id, from: token }
    if (single[1] === `agents`) return { kind: `agent`, agentId: id, from: token }
    if (view === `guide`) return null
    return { kind: `session`, sessionId: id, from: token }
  }
  return null
}

/** A route resolved against the collections — an issue's id; a session
 *  route's row exists in this team. */
export type RouteTab =
  | { kind: `issue`; issueId: string; from: string | null }
  | { kind: `thread`; threadId: string; from: string | null }
  | { kind: `agent`; agentId: string; from: string | null }
  | { kind: `session`; sessionId: string; from: string | null }

/** The tab a resolved route shows. */
export function routeTab(route: RouteTab): Exclude<WorkTab, { kind: `new` }> {
  return { ...route } as Exclude<WorkTab, { kind: `new` }>
}

/** The key of the tab a resolved route shows — the active tab. */
export function routeTabKey(route: RouteTab): string {
  return tabKey(routeTab(route))
}

/** Whether the URL is the New session tab's page (the Agent page). */
export function isNewRunPath(pathname: string): boolean {
  return /^\/t\/[^/]+\/agent\/?$/.test(pathname)
}

/** EXP-923: the `?from=` token the sidebar's Running rows navigate with. A
 * navigation carrying it creates NO tab — the sidebar row IS the affordance.
 * It still FOCUSES a tab that is already open. */
export const TABLESS_ORIGIN = `running`

/** Whether a navigation's origin may create a tab. */
export function originCreatesTab(from: string | null | undefined): boolean {
  return from !== TABLESS_ORIGIN
}

/** EXP-1250: the list-detail origins (`detail-origin.ts` `originHasListNav`)
 * whose opens share ONE preview slot — stepping the Inbox item by item
 * REPLACES that list's tab in place instead of piling a chip per item onto
 * the strip. */
export const PREVIEW_ORIGINS: readonly string[] = [`inbox`, `inbox:my-issues`]

/** The preview SLOT an origin opens into — both Inbox lists share ONE slot
 * (the desktop's `inbox_slot`). `null` = not a preview. */
const PREVIEW_SLOTS: Readonly<Record<string, string>> = {
  inbox: `inbox`,
  'inbox:my-issues': `inbox`,
}

export function previewSlotKey(from: string | null | undefined): string | null {
  return from != null && Object.hasOwn(PREVIEW_SLOTS, from)
    ? PREVIEW_SLOTS[from]
    : null
}

export function isPreviewOrigin(from: string | null | undefined): boolean {
  return previewSlotKey(from) !== null
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
 * tab's stored origin. EXP-1250: a PREVIEW origin replaces the tab its list
 * opened last, in place; an item that already has a tab just focuses it.
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

  const preview = isPreviewOrigin(route.from)
  if (at >= 0) {
    const current = tabs[at]!
    if (preview || !creates || route.from === null || current.from === route.from) {
      return state
    }
    tabs[at] = { ...current, from: route.from } as WorkTab
    return { ...state, tabs }
  }
  if (!creates) return state
  const slotKey = previewSlotKey(route.from)
  if (slotKey) {
    for (let i = tabs.length - 1; i >= 0; i--) {
      if (previewSlotKey(tabs[i]!.from) === slotKey) {
        tabs[i] = tab
        return { ...state, tabs }
      }
    }
  }
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

/** Where a tab click goes. `resolveIssue` resolves an issue id to its board
 * slug and identifier (null while it syncs — the tab then waits). */
export function tabHref(
  teamSlug: string,
  tab: WorkTab,
  resolveIssue: (issueId: string) => { boardSlug: string; identifier: string } | null
): { to: string; params: Record<string, string>; search: { from?: string } } | null {
  const search = tab.from ? { from: tab.from } : {}
  switch (tab.kind) {
    case `new`:
      return { to: `/t/$teamSlug/agent`, params: { teamSlug }, search: {} }
    case `thread`:
      return {
        to: `/t/$teamSlug/threads/$threadId`,
        params: { teamSlug, threadId: tab.threadId },
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
 * desktop's `partition_tabs` vectors), shared with the thread header's
 * strip (`partitionThreadStrip`).
 */
export { partitionTabs } from "@exp/domain-contract/session-tree"

// ── Persistence ──────────────────────────────────────────────────────────────

/** EXP-1293: v2 — the v1 blobs named coding-session runs, which are gone. */
export function workTabsStorageKey(teamId: string): string {
  return `exp:work-tabs:v2:${teamId}`
}

function str(value: unknown): value is string {
  return typeof value === `string` && value !== ``
}

function optStr(value: unknown): string | null {
  return str(value) ? value : null
}

function parseTab(value: unknown): WorkTab | null {
  if (typeof value !== `object` || value === null) return null
  const raw = value as Record<string, unknown>
  switch (raw.kind) {
    case `issue`:
      return str(raw.issueId)
        ? { kind: `issue`, issueId: raw.issueId, from: optStr(raw.from) }
        : null
    case `thread`:
      return str(raw.threadId)
        ? { kind: `thread`, threadId: raw.threadId, from: optStr(raw.from) }
        : null
    case `agent`:
      return str(raw.agentId)
        ? { kind: `agent`, agentId: raw.agentId, from: optStr(raw.from) }
        : null
    case `session`:
      return str(raw.sessionId)
        ? { kind: `session`, sessionId: raw.sessionId, from: optStr(raw.from) }
        : null
    case `new`:
      return NEW_RUN_TAB
    default:
      return null
  }
}

/** Read a stored state back. Anything malformed is dropped — a corrupt or
 * foreign blob is an empty strip, never a crash. Duplicate keys keep the
 * first. */
export function parseWorkTabsState(raw: string | null | undefined): WorkTabsState {
  if (!raw) return EMPTY_WORK_TABS
  let json: unknown
  try {
    json = JSON.parse(raw)
  } catch {
    return EMPTY_WORK_TABS
  }
  if (typeof json !== `object` || json === null) return EMPTY_WORK_TABS
  const source = json as { tabs?: unknown }
  const seen = new Set<string>()
  const tabs: WorkTab[] = []
  for (const entry of Array.isArray(source.tabs) ? source.tabs : []) {
    const tab = parseTab(entry)
    if (!tab || seen.has(tabKey(tab))) continue
    seen.add(tabKey(tab))
    tabs.push(tab)
  }
  return { tabs }
}
