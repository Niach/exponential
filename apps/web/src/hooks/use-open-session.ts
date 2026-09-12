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
   *  Omit the whole options object to derive it from the current URL. */
  origin: DetailOrigin | null
}

/** The navigation a run's row performs — pure, so every case is a test. */
export function sessionNavigation(
  teamSlug: string,
  session: OpenableSession,
  origin: DetailOrigin | null
): {
  to: string
  params: Record<string, string>
  search?: { from: string }
} {
  const token = formatOrigin(origin)
  // The issue's OWN run: its session route, beside the board list, with Back
  // to the issue (EXP-851 §C1). Only for a run that HAS an issue — an action,
  // batch or chat run started from an issue detail is not that issue's run.
  if (origin?.kind === `issue` && session.issueId) {
    return {
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier/session`,
      params: {
        teamSlug,
        boardSlug: origin.boardSlug,
        issueIdentifier: origin.identifier,
      },
      ...(token ? { search: { from: token } } : {}),
    }
  }
  return {
    to: `/t/$teamSlug/sessions/$sessionId`,
    params: { teamSlug, sessionId: session.id },
    ...(token ? { search: { from: token } } : {}),
  }
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
      const origin = options
        ? options.origin
        : deriveOrigin(screen, capturedOrigin(screen, parseOrigin(from)), {
            kind: `session`,
          })
      void navigate(sessionNavigation(teamSlug, session, origin) as never)
    },
    [navigate, teamSlug, location, from]
  )
}
