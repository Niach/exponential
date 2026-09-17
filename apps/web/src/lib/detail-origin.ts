// EXP-818 / EXP-851: the ONE rule for where a freshly opened DETAIL belongs —
// the web port of the desktop's `navigation::derive_origin` (apps/desktop/
// crates/ui/src/navigation.rs), same rule, same cases:
//
//   * Opened from a LIST screen (board, inbox, support, agent, reviews): that
//     list STAYS — it rides the detail's `?from=` token and the sidebar shows
//     it as the LIST NAV panel beside the detail.
//   * Opened CONTEXT-FREE (a pinned row, a sidebar session row, a deep link, a
//     full-page screen like Devices/Settings): no token, and the sidebar's
//     MAIN MENU stays put.
//
// EXP-851 made the token the only input: `?from=` decides the sidebar occupant
// (`sidebarOccupant`) and where Back goes. The vocabulary is the list set —
// `board:<slug>`, `inbox`, `inbox:my-issues`, `support`, `agent`, `reviews`,
// `automations` (EXP-862) — plus two legacy spellings that still parse:
// `sessions` (the old `agent`) and `issue:<board>:<identifier>`, which reads
// as its board's list since EXP-870 made a run ONE URL (`/sessions/$id`) and
// retired the issue-scoped session route that origin existed for.
// Pure, so every combination is a test.

/** The list a detail sits beside / returns to. */
export type DetailOrigin =
  | { kind: `inbox`; tab?: `my-issues` }
  | { kind: `board`; boardSlug: string }
  | { kind: `support` }
  | { kind: `reviews` }
  /** The Agent page's Running/Past list. */
  | { kind: `agent` }
  /** EXP-862: the Automations page's finished-automated-runs list — the one
   *  list that shows an UNATTENDED run, so a run opened from it returns
   *  there rather than to the Agent page's person-started list. */
  | { kind: `automations` }

/** The screen a navigation starts FROM. `other` is every full-page screen
 * (Devices, Actions, Automations, Settings…) — the context-free set, desktop
 * `Screen::is_context_free`. */
export type OriginScreen =
  | { kind: `inbox` }
  | { kind: `board`; boardSlug: string }
  | { kind: `issue`; boardSlug: string; identifier: string }
  | { kind: `session` }
  | { kind: `support` }
  | { kind: `reviews` }
  /** The Agent page — a list context (desktop `Screen::Chat`: the Sessions
   * column's own center), never context-free. */
  | { kind: `agent` }
  /** The Automations page — a list context too (desktop
   * `ToolWindow::Automations`), never context-free. */
  | { kind: `automations` }
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
  if (rest === `/automations`) return { kind: `automations` }
  if (rest === `/support`) return { kind: `support` }
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
  carried: DetailOrigin | null = null
): DetailOrigin | null {
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
    case `support`:
      return { kind: `support` }
    case `reviews`:
      return { kind: `reviews` }
    case `automations`:
      return { kind: `automations` }
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
      return origin.tab === `my-issues` ? `inbox:my-issues` : `inbox`
    case `board`:
      return `board:${origin.boardSlug}`
    case `support`:
      return `support`
    case `reviews`:
      return `reviews`
    case `agent`:
      return `agent`
    case `automations`:
      return `automations`
  }
}

/** The inverse. Anything unrecognised is "no origin" — a link is a shortcut,
 * not a guarantee (`lib/launch-seed.ts`'s rule). `sessions` is the legacy
 * spelling of `agent` (EXP-818 links still in the wild), and EXP-870 reads a
 * legacy `issue:<board>:<identifier>` as that board's list. */
export function parseOrigin(
  value: string | null | undefined
): DetailOrigin | null {
  if (!value) return null
  if (value === `inbox`) return { kind: `inbox` }
  if (value === `inbox:my-issues`) return { kind: `inbox`, tab: `my-issues` }
  if (value === `support`) return { kind: `support` }
  if (value === `reviews`) return { kind: `reviews` }
  if (value === `agent` || value === `sessions`) return { kind: `agent` }
  if (value === `automations`) return { kind: `automations` }
  const board = value.match(/^board:([^:]+)$/)
  if (board) return { kind: `board`, boardSlug: board[1] }
  const issue = value.match(/^issue:([^:]+):([^:]+)$/)
  if (issue) return { kind: `board`, boardSlug: issue[1] }
  return null
}

/** The list nav's back-row label — the list this detail came from. A board
 * origin needs the board's NAME, which only the caller can resolve. */
export function originLabel(
  origin: DetailOrigin,
  boardName?: string | null
): string {
  switch (origin.kind) {
    case `inbox`:
      return `Inbox`
    case `support`:
      return `Support`
    case `reviews`:
      return `Reviews`
    case `agent`:
      return `Agent`
    case `automations`:
      return `Automations`
    case `board`:
      return boardName || `Board`
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
        search: origin.tab === `my-issues` ? { tab: `my-issues` } : {},
      }
    case `support`:
      return { to: `/t/$teamSlug/support`, params: { teamSlug }, search: {} }
    case `reviews`:
      return { to: `/t/$teamSlug/reviews`, params: { teamSlug }, search: {} }
    case `agent`:
      return { to: `/t/$teamSlug/agent`, params: { teamSlug }, search: {} }
    case `automations`:
      return {
        to: `/t/$teamSlug/automations`,
        params: { teamSlug },
        search: {},
      }
  }
}

/** Which of the sidebar's three panels occupies the slot. */
export type SidebarOccupant =
  | { kind: `main` }
  | { kind: `settings` }
  | { kind: `list`; origin: DetailOrigin }
  /** EXP-916: a review detail — the panel is the pull request's FILE TREE
   *  (`ReviewFilesNav`), a review's context, whatever list it was opened
   *  from. The desktop's `LeftOccupant::ReviewFiles`. */
  | { kind: `review` }

/** A DETAIL route below `/t/$teamSlug` — the only routes that can show a list
 * nav. Board/inbox/support/agent/reviews are LIST screens and keep the main
 * menu. */
function isDetailRest(rest: string): boolean {
  if (/^\/boards\/[^/]+\/issues\/[^/]+$/.test(rest)) return true
  if (/^\/sessions\/[^/]+$/.test(rest)) return true
  if (/^\/reviews\/[^/]+$/.test(rest)) return true
  if (/^\/support\/[^/]+$/.test(rest)) return true
  return false
}

/**
 * EXP-851: the sidebar's occupant, from the URL alone — settings while any
 * `/settings` route is active, (EXP-916) the review's file tree on a review
 * detail, (EXP-945) the RUN's file tree while its Changes face is up
 * (`?view=diff`), the LIST NAV on any other detail route that carries a
 * parseable `?from=`, the main menu otherwise. Derived (never click state) so
 * a deep link lands settled and every entry point drives the same swap.
 */
export function sidebarOccupant(
  pathname: string,
  from: string | null | undefined,
  /** The detail's `?view=` face — only `diff` changes anything. */
  view?: string | null
): SidebarOccupant {
  const rest = teamRest(pathname)
  if (rest === null) return { kind: `main` }
  if (rest === `/settings` || rest.startsWith(`/settings/`)) {
    return { kind: `settings` }
  }
  if (/^\/reviews\/[^/]+$/.test(rest)) return { kind: `review` }
  // EXP-945: a run's diff is a review's diff — same panel, same slot
  // (`review-files-slot.ts`), so the file tree never floats in the column.
  if (/^\/sessions\/[^/]+$/.test(rest) && view === `diff`) {
    return { kind: `review` }
  }
  if (!isDetailRest(rest)) return { kind: `main` }
  const origin = parseOrigin(from)
  return origin ? { kind: `list`, origin } : { kind: `main` }
}

/** EXP-870: how deep an occupant sits — the main menu 0, a list nav (or a
 * review's file tree) 1, settings 2. The slide reads direction off it. */
export function occupantDepth(kind: SidebarOccupant[`kind`]): number {
  if (kind === `main`) return 0
  if (kind === `list` || kind === `review`) return 1
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
  panel: `list` | `review` | `settings`,
  occupant: SidebarOccupant[`kind`]
): -1 | 0 | 1 {
  const depth = occupantDepth(panel)
  const current = occupantDepth(occupant)
  if (depth === current) return 0
  return depth > current ? -1 : 1
}
