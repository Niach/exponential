// EXP-818 / EXP-851: the ONE rule for where a freshly opened DETAIL belongs —
// the web port of the desktop's `navigation::derive_origin` (apps/desktop/
// crates/ui/src/navigation.rs), same rule, same cases:
//
//   * Opened from a LIST screen (board, inbox, threads): that
//     list STAYS — it rides the detail's `?from=` token and the content card
//     shows it as its second sidebar beside the detail (#EXP-1672).
//   * Opened CONTEXT-FREE (a pinned row, a sidebar session row, a deep link, a
//     full-page screen like Devices/Settings): no token, and the sidebar's
//     MAIN MENU stays put.
//
// EXP-851 made the token the only input: `?from=` decides the second sidebar
// (`secondSidebarFor`) and where Back goes. The vocabulary is the list set —
// `board:<slug>`, `inbox`, `inbox:my-issues`, `threads` (EXP-1293: the
// Threads page, which replaced the Agent page's lists and Reviews),
// `drafts` (EXP-1170), `running`. The retired `agent`, `agent:recent`,
// `reviews`, `sessions` and `action:<id>` (NEW-2) tokens parse
// to "no origin". EXP-1246: only `inbox` keeps its list BESIDE the detail
// (`originHasListNav`); every other origin names Back only.
// Pure, so every combination is a test.

/** The list a detail sits beside / returns to. */
export type DetailOrigin =
  /** EXP-1170: `drafts` = the phone inbox's Drafts tab — a draft reopened
   *  from it returns there, not to Notifications. */
  | { kind: `inbox`; tab?: `my-issues` | `drafts` }
  | { kind: `board`; boardSlug: string }
  /** EXP-1293: the Threads page (Needs you · Running · Recent). */
  | { kind: `threads` }
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
 * (Devices, Settings…) — the context-free set, desktop
 * `Screen::is_context_free`. */
export type OriginScreen =
  | { kind: `inbox` }
  | { kind: `board`; boardSlug: string }
  | { kind: `issue`; boardSlug: string; identifier: string }
  | { kind: `session` }
  /** EXP-1293: the Threads page — a list context, never context-free. */
  | { kind: `threads` }
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
  if (rest === `/threads`) return { kind: `threads` }
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
    case `threads`:
      return { kind: `threads` }
    case `session`:
      return carried
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
    case `threads`:
      return `threads`
    case `running`:
      return `running`
    case `drafts`:
      return `drafts`
  }
}

/** The inverse. Anything unrecognised is "no origin" — a link is a shortcut,
 * not a guarantee (`lib/launch-seed.ts`'s rule); the retired `agent`,
 * `agent:recent`, `reviews`, `sessions` and (NEW-2) `action:` tokens
 * included. */
export function parseOrigin(
  value: string | null | undefined
): DetailOrigin | null {
  if (!value) return null
  if (value === `inbox`) return { kind: `inbox` }
  if (value === `inbox:my-issues`) return { kind: `inbox`, tab: `my-issues` }
  if (value === `inbox:drafts`) return { kind: `inbox`, tab: `drafts` }
  if (value === `threads`) return { kind: `threads` }
  if (value === `running`) return { kind: `running` }
  if (value === `drafts`) return { kind: `drafts` }
  const board = value.match(/^board:([^:]+)$/)
  if (board) return { kind: `board`, boardSlug: board[1] }
  return null
}

/** The list nav's back-row label — the list this detail came from. A board
 * origin needs its NAME, which only the caller can resolve. */
export function originLabel(
  origin: DetailOrigin,
  name?: string | null
): string {
  switch (origin.kind) {
    case `inbox`:
      return `Inbox`
    case `threads`:
      return `Chats`
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
    case `threads`:
      return { to: `/t/$teamSlug/threads`, params: { teamSlug }, search: {} }
    case `drafts`:
      return { to: `/t/$teamSlug/drafts`, params: { teamSlug }, search: {} }
    case `running`:
      // The sidebar's Running section is not a page — a thread opened from it
      // has no list to go back to, so each caller falls back, exactly like no
      // origin at all.
      return null
  }
}

/** Who occupies the sidebar column: the main menu, or (on `/settings`) the
 *  48px icon rail beside the settings nav. #EXP-1672: the Inbox never folds
 *  the main menu any more — its list is the content card's second sidebar
 *  (`secondSidebarFor`), desktop `second_sidebar_for` parity. */
export type SidebarOccupant = { kind: `main` } | { kind: `settings` }

/** A DETAIL route below `/t/$teamSlug` — the only routes that can show a list
 * nav. Board/inbox/threads are LIST screens. */
function isDetailRest(rest: string): boolean {
  if (/^\/boards\/[^/]+\/issues\/[^/]+$/.test(rest)) return true
  if (/^\/sessions\/[^/]+$/.test(rest)) return true
  // EXP-1170: the New issue page is the issue detail in draft mode.
  if (/^\/drafts\/[^/]+$/.test(rest)) return true
  // EXP-1293: a thread and an agent's channel are details too.
  if (/^\/threads\/[^/]+$/.test(rest)) return true
  if (/^\/agents\/[^/]+$/.test(rest)) return true
  return false
}

/**
 * EXP-851: the sidebar's occupant, from the URL alone — settings while any
 * `/settings` route is active, the main menu otherwise. Derived (never click
 * state) so a deep link lands settled.
 */
export function sidebarOccupant(pathname: string): SidebarOccupant {
  const rest = teamRest(pathname)
  if (rest === `/settings` || rest?.startsWith(`/settings/`)) {
    return { kind: `settings` }
  }
  return { kind: `main` }
}

/**
 * #EXP-1672 (EXP-1246): the list the content card shows as its SECOND
 * sidebar, beside the active screen — desktop `second_sidebar_for`. The md+
 * Inbox page always (nothing selected; `tab` = its `?tab=`), and a detail
 * whose `?from=` keeps its list (`originHasListNav`). `null` = the screen
 * alone, full width.
 */
export function secondSidebarFor(
  pathname: string,
  from: string | null | undefined,
  options: {
    /** The Inbox page's `?tab=` — which of its lists the sidebar shows. */
    tab?: string | null
  } = {}
): DetailOrigin | null {
  const rest = teamRest(pathname)
  if (rest === null) return null
  if (rest === `/inbox`) {
    return options.tab === `my-issues`
      ? { kind: `inbox`, tab: `my-issues` }
      : { kind: `inbox` }
  }
  if (!isDetailRest(rest)) return null
  const origin = parseOrigin(from)
  return origin && originHasListNav(origin) ? origin : null
}

/** EXP-1246: the ONLY origin whose list stays beside the detail — the Inbox
 * (both its lists; desktop `second_sidebar_for` parity). Board, threads,
 * running and drafts origins name only where Back goes. */
export function originHasListNav(origin: DetailOrigin): boolean {
  return origin.kind === `inbox` && origin.tab !== `drafts`
}
