import { useCallback } from "react"
import { useNavigate, useParams } from "@tanstack/react-router"

// #EXP-1493: THE plan page's ways in (plan-page.json WAYS IN): a story's
// `Open plan`, a plan card's header link, an inbox row / a Needs-you row
// whose session waits on a plan (`planPageFor`).

/** `teamSlug` absent = the route's. */
export function useOpenPlan(teamSlug?: string): (sessionId: string, messageId: string) => void {
  const navigate = useNavigate()
  const params = useParams({ strict: false }) as { teamSlug?: string }
  const slug = teamSlug ?? params.teamSlug ?? ``
  return useCallback(
    (sessionId: string, messageId: string) =>
      void navigate({
        to: `/t/$teamSlug/sessions/$sessionId/plan/$messageId`,
        params: { teamSlug: slug, sessionId, messageId },
      }),
    [navigate, slug]
  )
}
