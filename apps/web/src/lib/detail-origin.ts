// EXP-818 / EXP-851: the ONE rule for where a freshly opened DETAIL belongs —
// the web port of the desktop's `navigation::derive_origin` (apps/desktop/
// crates/ui/src/navigation.rs), same rule, same cases:
//
//   * Opened from a LIST screen (board, inbox, agent, reviews): that
//     list STAYS — it rides the detail's `?from=` token and the sidebar shows
//     it as the LIST NAV panel beside the detail.
//   * Opened CONTEXT-FREE (a pinned row, a sidebar session row, a deep link, a
//     full-page screen like Devices/Settings): no token, and the sidebar's
//     MAIN MENU stays put.
//
// EXP-851 made the token the only input: `?from=` decides the sidebar occupant
// (`sidebarOccupant`) and where Back goes. The vocabulary is the list set —
// `board:<slug>`, `inbox`, `inbox:my-issues`, `agent`, `agent:recent`
// (EXP-1246), `reviews`, `action:<id>` (an action page's Runs), `drafts`
// (EXP-1170) — plus one legacy spelling that still parses: `sessions` (the
// old `agent`). EXP-1246: only `inbox` and `agent:recent` keep their list
// BESIDE the detail (`originHasListNav`); every other origin names Back only.
// Pure, so every combination is a test.

/** The list a detail sits beside / returns to. */
export type DetailOrigin =
  /** EXP-1170: `drafts` = the phone inbox's Drafts tab — a draft reopened
   *  from it returns there, not to Notifications. */
  | { kind: `inbox`; tab?: `my-issues` | `drafts` }
  | { kind: `board`; boardSlug: string }
  | { kind: `reviews` }
  /** The Agent page's Running/Past list. EXP-1246: `tab: recent` = its
   *  Recent panel, a list that STAYS beside the run it opened. */
  | { kind: `agent`; tab?: `recent` }
  /** An action page's Runs (EXP-862, SLOP-2) — the one list that shows a
   *  TRIGGERED run, so a run opened from it returns there rather than to the
   *  Agent page's person-started list. */
  | { kind: `action`; actionId: string }
  /** EXP-923: the sidebar's RUNNING section — a live run of mine, opened from
   *  the main menu itself. It is an origin rather than "no origin" because it
   *  carries two extra rules: the navigation creates NO work tab
   *  (`work-tabs.ts` `TABLESS_ORIGIN`), and the sidebar keeps its MAIN menu
   *  (the section the row lives in IS the list). */
  | { kind: `running` }
  /** EXP-1170: the Drafts list (md+ `/drafts`) — a draft reopened from it
   *  returns there. No list panel: the page is the list. */
  | { kind: `drafts` }

/** The screen a navigation starts FROM. `other` is every full-page screen
 * (Devices, the Actions list, Settings…) — the context-free set, desktop
 * `Screen::is_context_free`. */
export type OriginScreen =
  | { kind: `inbox` }
  | { kind: `board`; boardSlug: string }
  | { kind: `issue`; boardSlug: string; identifier: string }
  | { kind: `session` }
  | { kind: `reviews` }
  /** The Agent page — a list context (desktop `Screen::Chat`: the Sessions
   * column's own center), never context-free. */
  | { kind: `agent` }
  /** An action's page — a list context too (its Runs), never context-free. */
  | { kind: `action`; actionId: string }
  | { kind: `other` }

/** What is being opened. */
export type DetailTarget =
  | { kind: `session` }
  | { kind: `issue`; boardSlug?: string; identifier?: string }

const TEAM_PATH = /^\/t\/[^/]+/

/** The path below `/t/$teamSlug`, without a trailing slash. `null` when the
 * path is not team-scoped at all. */
function teamRest(pathname: string): string | null {
  if (!TEAM_PATH.test(pathname)) return null
  return pathname.replace(TEAM_PATH, ``).replace(/\/$/, ``)
}

/** The current location as an `OriginScreen`. Unknown paths are context-free,
 * which is the safe answer: the detail then brings its own list. */
export function screenFromPath(pathname: string): OriginScreen {
  const rest = teamRest(pathname)
  if (rest === null) return { kind: `other` }
  if (rest === `/inbox`) return { kind: `inbox` }
  if (rest === `/agent`) return { kind: `agent` }
  const action = rest.match(/^\/actions\/([^/]+)$/)
  if (action) return { kind: `action`, actionId: action[1] }
  if (rest === `/reviews`) return { kind: `reviews` }
  if (/^\/sessions\/[^/]+$/.test(rest)) return { kind: `session` }
  const issue = rest.match(/^\/boards\/([^/]+)\/issues\/([^/]+)$/)
  if (issue) {
    return { kind: `issue`, boardSlug: issue[1], identifier: issue[2] }
  }
  const board = rest.match(/^\/boards\/([^/]+)$/)
  if (board) return { kind: `board`, boardSlug: board[1] }
  return { kind: `other` }
}

/** Desktop `Screen::is_context_free`: nothing but full pages and "nowhere". */
export function isContextFree(screen: OriginScreen | null): boolean {
  return screen === null || screen.kind === `other`
}

/** The origin IN FORCE on `screen` — what a detail opened from here inherits.
 * A list screen IS the origin; a detail (an issue, a session) hands on the
 * origin IT carries, and one without a list hands on nothing (EXP-870:
 * desktop `derive_origin` parity — a pinned issue's run keeps the rail). */
export function capturedOrigin(
  screen: OriginScreen | null,
  carriedRaw: DetailOrigin | null = null
): DetailOrigin | null {
  // EXP-923: `running` belongs to the run it opened and travels no further —
  // an issue opened FROM a live run is ordinary work and gets its own tab.
  const carried = carriedRaw?.kind === `running` ? null : carriedRaw
  if (screen === null) return carried
  switch (screen.kind) {
    case `inbox`:
      return { kind: `inbox` }
    case `board`:
      return { kind: `board`, boardSlug: screen.boardSlug }
    case `issue`:
      // EXP-870: an issue opened with no list (a pinned row, search) hands on
      // none either — desktop `derive_origin` parity.
      return carried
    case `reviews`:
      return { kind: `reviews` }
    case `action`:
      return { kind: `action`, actionId: screen.actionId }
    case `session`:
      return carried
    case `agent`:
      return carried ?? { kind: `agent` }
    case `other`:
      return carried
  }
}

/**
 * The origin a newly opened detail gets. `previous` is the screen the
 * navigation starts from, `captured` the origin in force there
 * (`capturedOrigin`), `target` what is being opened.
 */
export function deriveOrigin(
  previous: OriginScreen | null,
  captured: DetailOrigin | null,
  target: DetailTarget
): DetailOrigin | null {
  // A list context stays put — that is the whole rule.
  if (!isContextFree(previous)) return captured
  if (target.kind === `issue`) {
    return target.boardSlug
      ? { kind: `board`, boardSlug: target.boardSlug }
      : captured
  }
  return null
}

/** The `?from=` token for an origin. No origin = no param: the sidebar's main
 * menu stays (EXP-851 §B). */
export function formatOrigin(origin: DetailOrigin | null): string | undefined {
  if (!origin) return undefined
  switch (origin.kind) {
    case `inbox`:
      return origin.tab ? `inbox:${origin.tab}` : `inbox`
    case `board`:
      return `board:${origin.boardSlug}`
    case `reviews`:
      return `reviews`
    case `agent`:
      return origin.tab ? `agent:${origin.tab}` : `agent`
    case `action`:
      return `action:${origin.actionId}`
    case `running`:
      return `running`
    case `drafts`:
      return `drafts`
  }
}

/** The inverse. Anything unrecognised is "no origin" — a link is a shortcut,
 * not a guarantee (`lib/launch-seed.ts`'s rule). `sessions` is the legacy
 * spelling of `agent` (EXP-818 links still in the wild). */
export function parseOrigin(
  value: string | null | undefined
): DetailOrigin | null {
  if (!value) return null
  if (value === `inbox`) return { kind: `inbox` }
  if (value === `inbox:my-issues`) return { kind: `inbox`, tab: `my-issues` }
  if (value === `inbox:drafts`) return { kind: `inbox`, tab: `drafts` }
  if (value === `reviews`) return { kind: `reviews` }
  if (value === `agent` || value === `sessions`) return { kind: `agent` }
  if (value === `agent:recent`) return { kind: `agent`, tab: `recent` }
  if (value === `running`) return { kind: `running` }
  if (value === `drafts`) return { kind: `drafts` }
  const board = value.match(/^board:([^:]+)$/)
  if (board) return { kind: `board`, boardSlug: board[1] }
  const action = value.match(/^action:([^:]+)$/)
  if (action) return { kind: `action`, actionId: action[1] }
  return null
}

/** The list nav's back-row label — the list this detail came from. A board
 * or action origin needs its NAME, which only the caller can resolve. */
export function originLabel(
  origin: DetailOrigin,
  name?: string | null
): string {
  switch (origin.kind) {
    case `inbox`:
      return `Inbox`
    case `reviews`:
      return `Reviews`
    case `agent`:
      // `agent:recent` too: Back lands on the Agent page (Recent still open,
      // the list's own band names it).
      return `Agent`
    case `action`:
      return name || `Action`
    case `running`:
      return `Running`
    case `drafts`:
      return `Drafts`
    case `board`:
      return name || `Board`
  }
}

/** The board a list nav renders, when it renders one. */
export function originBoardSlug(origin: DetailOrigin): string | null {
  return origin.kind === `board` ? origin.boardSlug : null
}

/** EXP-870: the ONE "back to the list" navigation — the list nav's back row,
 * the session route's Back and the issue's phone back all land here. `null` for no origin: each
 * caller owns its own fallback (a session goes to the Agent page, an issue to
 * its board). Pure, so the destinations are a test. */
export function originListNavigation(
  teamSlug: string,
  origin: DetailOrigin | null
): {
  to: string
  params: Record<string, string>
  search: Record<string, string>
} | null {
  if (!origin) return null
  switch (origin.kind) {
    case `board`:
      return {
        to: `/t/$teamSlug/boards/$boardSlug`,
        params: { teamSlug, boardSlug: origin.boardSlug },
        search: {},
      }
    case `inbox`:
      return {
        to: `/t/$teamSlug/inbox`,
        params: { teamSlug },
        search: origin.tab ? { tab: origin.tab } : {},
      }
    case `reviews`:
      return { to: `/t/$teamSlug/reviews`, params: { teamSlug }, search: {} }
    case `agent`:
      // EXP-1246: `agent:recent` lands on the same page — the Recent panel
      // stays open across the detail (`recent-runs-panel.ts`), so Back is the
      // list with nothing selected.
      return { to: `/t/$teamSlug/agent`, params: { teamSlug }, search: {} }
    case `action`:
      return {
        to: `/t/$teamSlug/actions/$actionId`,
        params: { teamSlug, actionId: origin.actionId },
        search: { tab: `runs` },
      }
    case `drafts`:
      return { to: `/t/$teamSlug/drafts`, params: { teamSlug }, search: {} }
    case `running`:
      // The sidebar's Running section is not a page — a run opened from it
      // has no list to go back to, so each caller falls back (a run to the
      // Agent page, an issue to its board), exactly like no origin at all.
      return null
  }
}

/** Which of the sidebar's two panels occupies the slot. EXP-1246: `list` =
 *  the ONE list-detail host (`components/team/list-detail.tsx`): the Inbox
 *  and the Agent page's Recent runs. The diff's file tree moved into the
 *  Guide's section page, so there is no `review` panel any more. */
export type SidebarOccupant =
  | { kind: `main` }
  | { kind: `settings` }
  | { kind: `list`; origin: DetailOrigin }

/** A DETAIL route below `/t/$teamSlug` — the only routes that can show a list
 * nav. Board/inbox/agent/reviews are LIST screens and keep the main
 * menu. */
function isDetailRest(rest: string): boolean {
  if (/^\/boards\/[^/]+\/issues\/[^/]+$/.test(rest)) return true
  if (/^\/sessions\/[^/]+$/.test(rest)) return true
  // EXP-1170: the New issue page is the issue detail in draft mode.
  if (/^\/drafts\/[^/]+$/.test(rest)) return true
  return false
}

/**
 * EXP-851: the sidebar's occupant, from the URL alone — settings while any
 * `/settings` route is active, the LIST-DETAIL host on a detail route whose
 * `?from=` keeps its list (`originHasListNav`), the main menu otherwise.
 * EXP-1246: the md+ Inbox page IS that host with nothing selected (`tab` =
 * its `?tab=`), and the Agent page with its Recent panel open (`recentOpen`,
 * the one input that is a disclosure rather than the URL) is the other.
 * Derived (never click state) so a deep link lands settled and every entry
 * point drives the same swap.
 */
export function sidebarOccupant(
  pathname: string,
  from: string | null | undefined,
  options: {
    /** The Inbox page's `?tab=` — which of its lists the host shows. */
    tab?: string | null
    /** The Agent page's Recent panel (`lib/recent-runs-panel.ts`). */
    recentOpen?: boolean
  } = {}
): SidebarOccupant {
  const rest = teamRest(pathname)
  if (rest === null) return { kind: `main` }
  if (rest === `/settings` || rest.startsWith(`/settings/`)) {
    return { kind: `settings` }
  }
  if (rest === `/inbox`) {
    return {
      kind: `list`,
      origin:
        options.tab === `my-issues`
          ? { kind: `inbox`, tab: `my-issues` }
          : { kind: `inbox` },
    }
  }
  if (rest === `/agent`) {
    return options.recentOpen
      ? { kind: `list`, origin: { kind: `agent`, tab: `recent` } }
      : { kind: `main` }
  }
  if (!isDetailRest(rest)) return { kind: `main` }
  const origin = parseOrigin(from)
  return origin && originHasListNav(origin)
    ? { kind: `list`, origin }
    : { kind: `main` }
}

/** EXP-1246: the ONLY origins whose list stays beside the detail — the Inbox
 * (both its lists) and the Agent page's Recent runs (desktop
 * `second_sidebar_for` parity). Board, reviews, action, agent, running and
 * drafts origins keep the main menu and name only where Back goes. */
export function originHasListNav(origin: DetailOrigin): boolean {
  if (origin.kind === `inbox`) return origin.tab !== `drafts`
  return origin.kind === `agent` && origin.tab === `recent`
}

/** EXP-870: how deep an occupant sits — the main menu 0, the list-detail
 * host 1, settings 2. The slide reads direction off it. */
export function occupantDepth(kind: SidebarOccupant[`kind`]): number {
  if (kind === `main`) return 0
  if (kind === `list`) return 1
  return 2
}

/**
 * EXP-870: where one of the two sliding panels sits, in panel widths — `0` in
 * the slot, `-1` tucked under the rail's edge (deeper than what is up: it
 * enters FROM the rail going forward and slides back under it on return), `1`
 * pushed out to the right (shallower than what is up: a list nav settings
 * slid in over). Directional, so going deeper and coming back never look the
 * same.
 */
export function panelOffset(
  panel: `list` | `settings`,
  occupant: SidebarOccupant[`kind`]
): -1 | 0 | 1 {
  const depth = occupantDepth(panel)
  const current = occupantDepth(occupant)
  if (depth === current) return 0
  return depth > current ? -1 : 1
}
