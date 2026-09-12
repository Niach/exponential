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
} from "@/lib/detail-origin"

// EXP-740: "open this run" is a NAVIGATION now, not a dock toggle. Every
// surface that used to call `openDock(id)` — issue detail's Watch, the
// sidebar's Sessions rows, the Agent page's list, the Automations runs list,
// the post-start watch in use-remote-start — calls this instead, so there is
// ONE place that decides where a run is steered: its own session route,
// inside the Agent shell (EXP-818 — a chat run too; the chat page's
// `?session=` detour is gone).
//
// EXP-818 (the navigation rule): the run also carries WHERE IT WAS OPENED FROM
// (`?from=`, `lib/detail-origin.ts` — the desktop's `derive_origin`), so Back
// returns to the list the reader came from instead of always landing on the
// Agent page. An origin of "the Agent list" is the route's default and rides
// no param at all.

/** What the hook needs off a run — a whole `CodingSession` satisfies it. */
export type OpenableSession = Pick<
  CodingSession,
  `id` | `issueId` | `actionId` | `actionName`
>

export function useOpenSession(): (session: OpenableSession) => void {
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
    (session: OpenableSession) => {
      if (!teamSlug) {
        console.warn(`useOpenSession: no teamSlug in scope, ignoring`)
        return
      }
      const screen = screenFromPath(location ?? ``)
      const origin = deriveOrigin(
        screen,
        capturedOrigin(screen, parseOrigin(from)),
        { kind: `session` }
      )
      const token = formatOrigin(origin)
      void navigate({
        to: `/t/$teamSlug/sessions/$sessionId`,
        params: { teamSlug, sessionId: session.id },
        ...(token ? { search: { from: token } } : {}),
      })
    },
    [navigate, teamSlug, location, from]
  )
}
