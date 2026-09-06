import { useCallback } from "react"
import { useNavigate, useParams } from "@tanstack/react-router"
import type { CodingSession } from "@/db/schema"
import { isChatSession } from "@/lib/session-identity"

// EXP-740: "open this run" is a NAVIGATION now, not a dock toggle. Every
// surface that used to call `openDock(id)` — issue detail's Watch, the
// Devices rows, the Automations runs list, the post-start watch in
// use-remote-start — calls this instead, so there is ONE place that decides
// where a run is steered: a chat run on the team's chat page (EXP-739, one
// conversation per team), anything else on its own session route.

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

  return useCallback(
    (session: OpenableSession) => {
      if (!teamSlug) {
        console.warn(`useOpenSession: no teamSlug in scope, ignoring`)
        return
      }
      if (isChatSession(session)) {
        // EXP-739: the chat page shows ONE chat, so a second running one is
        // only reachable by naming it — `?session=` is that address (absent =
        // the newest running chat).
        void navigate({
          to: `/t/$teamSlug/chat`,
          params: { teamSlug },
          search: { session: session.id },
        })
        return
      }
      void navigate({
        to: `/t/$teamSlug/sessions/$sessionId`,
        params: { teamSlug, sessionId: session.id },
      })
    },
    [navigate, teamSlug]
  )
}
