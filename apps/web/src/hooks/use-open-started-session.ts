import { useCallback } from "react"
import { useNavigate, useParams } from "@tanstack/react-router"
import type { CodingSession } from "@/db/schema"
import { threadIdOfRun, waitForThreadOfRun } from "@/hooks/use-threads"
import { useOpenSession, type OpenSessionOptions } from "@/hooks/use-open-session"
import { formatOrigin } from "@/lib/detail-origin"

// EXP-1284 (review T11): every start lands on its THREAD, the Conversation
// child selected — the started session's thread item usually syncs with its
// row; a start that named a thread knows it up front (`knownThreadId`). With
// neither within the wait, the session's own page opens (the old landing),
// so a start always lands somewhere.

export function useOpenStartedSession(): (
  session: CodingSession,
  options?: OpenSessionOptions & { knownThreadId?: string | null }
) => void {
  const navigate = useNavigate()
  const openSession = useOpenSession()
  const { teamSlug: routeSlug } = useParams({ strict: false }) as { teamSlug?: string }
  return useCallback(
    (session, options) => {
      const teamSlug = options?.teamSlug ?? routeSlug
      const land = (threadId: string | null) => {
        if (!threadId || !teamSlug) {
          openSession(session, options)
          return
        }
        const from = formatOrigin(options?.origin ?? null)
        void navigate({
          to: `/t/$teamSlug/threads/$threadId`,
          params: { teamSlug, threadId },
          search: from ? { from } : {},
        })
      }
      const known = threadIdOfRun(session.id) ?? options?.knownThreadId ?? null
      if (known) {
        land(known)
        return
      }
      void waitForThreadOfRun(session.id).then(land)
    },
    [navigate, openSession, routeSlug]
  )
}
