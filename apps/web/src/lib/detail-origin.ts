// EXP-818: the ONE rule for where a freshly opened DETAIL belongs — the web
// port of the desktop's `navigation::derive_origin` (apps/desktop/crates/ui/
// src/navigation.rs), same breadcrumb rule, same cases:
//
//   * Opened from a LIST context (the previous screen was a list, or another
//     detail sitting beside one): the list that is up STAYS. Inbox → issue
//     keeps the inbox; board → issue → Watch keeps the board; a sessions row
//     clicked while the inbox is up keeps the inbox.
//   * Opened CONTEXT-FREE (a full-page screen like Devices/Reviews/Settings, or
//     a deep link at boot): the detail brings its own list — an issue its
//     board, a session the Agent page's list.
//
// On the desktop the origin picks the rail's tool column. On the web it rides
// the detail route's `?from=` param and decides where BACK goes (the session
// route) — the issue route shows its board beside itself regardless, which is
// the same "the board stays" outcome. Pure, so every combination is a test.

/** The list a detail sits beside / returns to. */
export type DetailOrigin =
  | { kind: `inbox` }
  | { kind: `board`; boardSlug: string }
  /** An issue DETAIL (which itself shows its board's list beside it). */
  | { kind: `issue`; boardSlug: string; identifier: string }
  | { kind: `sessions` }

/** The screen a navigation starts FROM. `other` is every full-page screen
 * (Devices, Reviews, Actions, Automations, Settings, Support…) — the
 * context-free set, desktop `Screen::is_context_free`. */
export type OriginScreen =
  | { kind: `inbox` }
  | { kind: `board`; boardSlug: string }
  | { kind: `issue`; boardSlug: string; identifier: string }
  | { kind: `session` }
  /** The Agent page — a list context (desktop `Screen::Chat`: the Sessions
   * column's own center), never context-free. */
  | { kind: `agent` }
  | { kind: `other` }

/** What is being opened. */
export type DetailTarget =
  | { kind: `session` }
  | { kind: `issue`; boardSlug?: string; identifier?: string }

const TEAM_PATH = /^\/t\/[^/]+/

/** The current location as an `OriginScreen`. Unknown paths are context-free,
 * which is the safe answer: the detail then brings its own list. */
export function screenFromPath(pathname: string): OriginScreen {
  if (!TEAM_PATH.test(pathname)) return { kind: `other` }
  const rest = pathname.replace(TEAM_PATH, ``).replace(/\/$/, ``)
  if (rest === `/inbox`) return { kind: `inbox` }
  if (rest === `/agent`) return { kind: `agent` }
  if (/^\/sessions\/[^/]+/.test(rest)) return { kind: `session` }
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
 * A detail screen hands on the origin IT carries (`?from=`), falling back to
 * what it is: an issue page is its own board's list, a session page the Agent
 * list. A list screen IS the origin. */
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
      return (
        carried ?? {
          kind: `issue`,
          boardSlug: screen.boardSlug,
          identifier: screen.identifier,
        }
      )
    case `session`:
      return carried ?? { kind: `sessions` }
    case `agent`:
      return { kind: `sessions` }
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
  return { kind: `sessions` }
}

/** The `?from=` token for an origin. The Agent list is the session route's
 * DEFAULT, so it serialises to nothing at all. */
export function formatOrigin(origin: DetailOrigin | null): string | undefined {
  if (!origin) return undefined
  switch (origin.kind) {
    case `inbox`:
      return `inbox`
    case `board`:
      return `board:${origin.boardSlug}`
    case `issue`:
      return `issue:${origin.boardSlug}:${origin.identifier}`
    case `sessions`:
      return undefined
  }
}

/** The inverse. Anything unrecognised is "no origin" — a link is a shortcut,
 * not a guarantee (`lib/launch-seed.ts`'s rule). */
export function parseOrigin(
  value: string | null | undefined
): DetailOrigin | null {
  if (!value) return null
  if (value === `inbox`) return { kind: `inbox` }
  if (value === `sessions`) return { kind: `sessions` }
  const board = value.match(/^board:([^:]+)$/)
  if (board) return { kind: `board`, boardSlug: board[1] }
  const issue = value.match(/^issue:([^:]+):([^:]+)$/)
  if (issue) {
    return { kind: `issue`, boardSlug: issue[1], identifier: issue[2] }
  }
  return null
}
