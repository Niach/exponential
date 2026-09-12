import { useCallback } from "react"
import {
  useLocation,
  useNavigate,
  useParams,
  useSearch,
} from "@tanstack/react-router"
import type { CodingSession } from "@/db/schema"
import {
  capturedOrigin,
  deriveOrigin,
  formatOrigin,
  parseOrigin,
  screenFromPath,
  type DetailOrigin,
} from "@/lib/detail-origin"

// EXP-740: "open this run" is a NAVIGATION, not a dock toggle. Every surface
// that used to call `openDock(id)` — the issue detail's Watch, the sidebar's
// Sessions rows, the Agent page's list, the Automations runs list, the
// post-start watch in use-remote-start — calls this instead, so there is ONE
// place that decides where a run is steered.
//
// EXP-818 (the navigation rule): the run carries WHERE IT WAS OPENED FROM
// (`?from=`, `lib/detail-origin.ts` — the desktop's `derive_origin`), so the
// sidebar keeps that list beside it and Back returns there.
//
// EXP-851: two refinements.
//   * The origin can be passed EXPLICITLY. A list hands its own token
//     (`{ origin: { kind: 'agent' } }`); a context-free row — the sidebar's
//     Sessions and Pinned groups, the board switcher sheet — hands `null`, so
//     the sidebar's main menu stays put. Omitting the argument keeps the old
//     behaviour: derive the origin from the current URL.
//   * A run opened with an ISSUE origin lands on that issue's own session
//     route, not the flat `/sessions/$sessionId` one — the issue keeps its
//     board's list nav and Back returns to the issue.

/** What the hook needs off a run — a whole `CodingSession` satisfies it. */
export type OpenableSession = Pick<
  CodingSession,
  `id` | `issueId` | `actionId` | `actionName`
>

export interface OpenSessionOptions {
  /** The list the click came from. `null` = context-free (main menu stays).
   *  Omit the KEY (or the whole options object) to derive it from the URL. */
  origin?: DetailOrigin | null
  /** EXP-856: the issue the origin token NAMES, when the caller holds its id.
   *  The token carries a board slug + identifier, not an id, so only a caller
   *  that already has the issue row can prove a run belongs to it — and a run
   *  that belongs to ANOTHER issue must not be routed onto this issue's URL. */
  originIssueId?: string
}

/** The navigation a run's row performs — pure, so every case is a test. */
export function sessionNavigation(
  teamSlug: string,
  session: OpenableSession,
  origin: DetailOrigin | null,
  originIssueId?: string
): {
  to: string
  params: Record<string, string>
  search?: { from?: string; run?: string }
} {
  const token = formatOrigin(origin)
  // The issue's OWN run: its session route, beside the board list, with Back
  // to the issue (EXP-851 §C1). Only for a run that HAS an issue — an action,
  // batch or chat run started from an issue detail is not that issue's run —
  // and (EXP-856) only when that issue is THIS issue: a caller holding the
  // origin's issue id proves it, and a mismatch takes the flat route instead
  // of silently landing on a page showing a different run.
  if (
    origin?.kind === `issue` &&
    session.issueId &&
    (originIssueId === undefined || originIssueId === session.issueId)
  ) {
    return {
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier/session`,
      params: {
        teamSlug,
        boardSlug: origin.boardSlug,
        issueIdentifier: origin.identifier,
      },
      // EXP-856: `run` NAMES the session — without it the page falls back to
      // the issue's newest own run, which is not always the one clicked.
      search: token ? { from: token, run: session.id } : { run: session.id },
    }
  }
  return {
    to: `/t/$teamSlug/sessions/$sessionId`,
    params: { teamSlug, sessionId: session.id },
    ...(token ? { search: { from: token } } : {}),
  }
}

/** EXP-856: which run the issue-scoped session route steers — the one `?run=`
 *  names when it belongs to this issue (`rows` is already the issue's own),
 *  else the issue's NEWEST own run, the rule the Watch pill follows. Pure, so
 *  the fallback is a test rather than a render. */
export function issueSessionTarget<
  T extends { id: string; userId: string; startedAt: Date | string },
>(rows: readonly T[], runId: string | undefined, currentUserId: string | undefined): T | null {
  if (runId) {
    const named = rows.find((row) => row.id === runId)
    if (named) return named
  }
  const own = rows.filter((row) => row.userId === currentUserId)
  if (own.length === 0) return null
  return own.reduce((newest, row) =>
    new Date(row.startedAt) > new Date(newest.startedAt) ? row : newest
  )
}

export function useOpenSession(): (
  session: OpenableSession,
  options?: OpenSessionOptions
) => void {
  const navigate = useNavigate()
  // Loose match: every caller lives under `/t/$teamSlug`, but the hook must
  // not throw in a story/test that mounts one outside the team layout.
  const { teamSlug } = useParams({ strict: false })
  // Where the click happens — the previous screen of the rule. `strict: false`
  // keeps this safe on routes that declare no search at all.
  const location = useLocation({ select: (current) => current.pathname })
  const from = useSearch({
    strict: false,
    select: (search) =>
      typeof (search as { from?: unknown }).from === `string`
        ? ((search as { from?: string }).from ?? null)
        : null,
  })

  return useCallback(
    (session: OpenableSession, options?: OpenSessionOptions) => {
      if (!teamSlug) {
        console.warn(`useOpenSession: no teamSlug in scope, ignoring`)
        return
      }
      const screen = screenFromPath(location ?? ``)
      // `origin` PRESENT (even as `null`) is the caller's answer; absent means
      // derive it — so a caller that only knows `originIssueId` still gets the
      // URL-derived origin.
      const origin =
        options && `origin` in options
          ? (options.origin ?? null)
          : deriveOrigin(screen, capturedOrigin(screen, parseOrigin(from)), {
              kind: `session`,
            })
      void navigate(
        sessionNavigation(
          teamSlug,
          session,
          origin,
          options?.originIssueId
        ) as never
      )
    },
    [navigate, teamSlug, location, from]
  )
}
