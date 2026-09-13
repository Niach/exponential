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
// that used to call `openDock(id)` — the issue detail's Watch, the Agent
// page's list, the Automations runs list, the
// post-start watch in use-remote-start — calls this instead, so there is ONE
// place that decides where a run is steered.
//
// EXP-818 (the navigation rule): the run carries WHERE IT WAS OPENED FROM
// (`?from=`, `lib/detail-origin.ts` — the desktop's `derive_origin`), so the
// sidebar keeps that list beside it and Back returns there.
//
// EXP-851: the origin can be passed EXPLICITLY. A list hands its own token
// (`{ origin: { kind: 'agent' } }`); a context-free row — the sidebar's Pinned
// group, the board switcher sheet — hands `null`, so the sidebar's main menu
// stays put. Omitting the argument derives the origin from the current URL.
//
// EXP-870: ONE run URL, `/t/$teamSlug/sessions/$sessionId`, whatever the run
// is and wherever it was opened. The issue-scoped session route is a legacy
// redirect now: an issue and its run are one work tab with two faces
// (`lib/work-tabs.ts`), not a page nested under the issue.

/** What the hook needs off a run — a whole `CodingSession` satisfies it. */
export type OpenableSession = Pick<
  CodingSession,
  `id` | `issueId` | `actionId` | `actionName`
>

export interface OpenSessionOptions {
  /** The list the click came from. `null` = context-free (main menu stays).
   *  Omit the KEY (or the whole options object) to derive it from the URL. */
  origin?: DetailOrigin | null
}

/** The navigation a run's row performs — pure, so every case is a test. */
export function sessionNavigation(
  teamSlug: string,
  session: Pick<OpenableSession, `id`>,
  origin: DetailOrigin | null
): {
  to: string
  params: Record<string, string>
  search?: { from?: string }
} {
  const token = formatOrigin(origin)
  return {
    to: `/t/$teamSlug/sessions/$sessionId`,
    params: { teamSlug, sessionId: session.id },
    ...(token ? { search: { from: token } } : {}),
  }
}

/** EXP-856: which run an ISSUE's run face steers — the one `runId` names when
 *  it belongs to this issue (`rows` is already the issue's own), else the
 *  issue's NEWEST own run, the rule the Watch pill follows. EXP-870: backs the
 *  Issue | Run toggle and the legacy issue-session redirect. Pure, so the
 *  fallback is a test rather than a render. */
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
      // derive it from the URL.
      const origin =
        options && `origin` in options
          ? (options.origin ?? null)
          : deriveOrigin(screen, capturedOrigin(screen, parseOrigin(from)), {
              kind: `session`,
            })
      void navigate(sessionNavigation(teamSlug, session, origin) as never)
    },
    [navigate, teamSlug, location, from]
  )
}
