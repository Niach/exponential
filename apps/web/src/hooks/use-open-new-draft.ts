import { useCallback } from "react"
import { useNavigate, useRouter } from "@tanstack/react-router"
import { draftOriginFrom, newDraftNavigation } from "@/lib/issue-draft-page"
import type { StatusRowOption } from "@/lib/team-statuses"

/**
 * EXP-1170: every "New issue" opener (sidebar button, phone compose arm,
 * board empty state, a group header's "+") runs this — a fresh draft id
 * minted at TAP time, the page opened on it, the list in force handed on as
 * `?from=`. Never a dialog.
 */
export function useOpenNewDraft(teamSlug: string) {
  const navigate = useNavigate()
  const router = useRouter()
  return useCallback(
    (options: { boardId?: string; status?: StatusRowOption } = {}) => {
      const location = router.state.location
      const fromParam = (location.search as { from?: unknown }).from
      const from = draftOriginFrom(
        location.pathname,
        typeof fromParam === `string` ? fromParam : undefined
      )
      void navigate(newDraftNavigation({ teamSlug, ...options, from }))
    },
    [navigate, router, teamSlug]
  )
}
