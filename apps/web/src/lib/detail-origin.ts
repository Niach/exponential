// EXP-818 / EXP-851: the ONE rule for where a freshly opened DETAIL belongs —
// the web port of the desktop's `navigation::derive_origin` (apps/desktop/
// crates/ui/src/navigation.rs), same rule, same cases:
//
//   * Opened from a LIST screen (board, inbox, chats): that list rides the
//     detail's `?from=` token, and Back returns to it.
//   * Opened CONTEXT-FREE (a pinned row, a sidebar session row, a deep link, a
//     full-page screen like Devices/Settings): no token.
//
// EXP-851 made the token the only input: `?from=` decides where Back goes. The
// vocabulary is the list set —
// `board:<slug>`, `inbox`, `inbox:my-issues`, `chats` (EXP-1293: the
// Chats page, which replaced the Agent page's lists and Reviews),
// `drafts` (EXP-1170), `running`. The retired `agent`, `agent:recent`,
// `reviews`, `sessions` and `action:<id>` (NEW-2) tokens parse
// to "no origin". #FEED-191: no list stays beside a detail any more (the wide
// Inbox is a popover); an old `?from=inbox` link just names Back on phones.
// Pure, so every combination is a test.

/** The list a detail sits beside / returns to. */
export type DetailOrigin =
  /** EXP-1170: `drafts` = the phone inbox's Drafts tab — a draft reopened
   *  from it returns there, not to Notifications. */
  | { kind: `inbox`; tab?: `my-issues` | `drafts` }
  | { kind: `board`; boardSlug: string }
  /** EXP-1293: the Chats page (Needs you · Running · Recent). */
  | { kind: `chats` }
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
  /** EXP-1293: the Chats page — a list context, never context-free. */
  | { kind: `chats` }
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
  if (rest === `/chats`) return { kind: `chats` }
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
    case `chats`:
      return { kind: `chats` }
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
    case `chats`:
      return `chats`
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
  if (value === `chats`) return { kind: `chats` }
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
    case `chats`:
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
    case `chats`:
      return { to: `/t/$teamSlug/chats`, params: { teamSlug }, search: {} }
    case `drafts`:
      return { to: `/t/$teamSlug/drafts`, params: { teamSlug }, search: {} }
    case `running`:
      // The sidebar's Running section is not a page — a chat opened from it
      // has no list to go back to, so each caller falls back, exactly like no
      // origin at all.
      return null
  }
}

/** Who occupies the sidebar column: the main menu, or (on `/settings`) the
 *  settings nav in its place — desktop `left_occupant_for`. */
export type SidebarOccupant = { kind: `main` } | { kind: `settings` }

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
