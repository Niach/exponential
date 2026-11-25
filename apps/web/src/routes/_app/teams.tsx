import { createFileRoute, Navigate } from "@tanstack/react-router"
import { useIsMobile } from "@exp/ui"
import { PhoneTeams } from "@/components/team/phone-teams"
import { pageTitle } from "@/lib/page-title"

// #EXP-2109: the phones' Teams tab; md+ has the sidebar. `?team=` = the team
// a link names, unfolded.
export const Route = createFileRoute(`/_app/teams`)({
  head: () => ({ meta: [{ title: pageTitle(`Teams`) }] }),
  validateSearch: (search: Record<string, unknown>): { team?: string } =>
    typeof search.team === `string` && search.team ? { team: search.team } : {},
  component: TeamsRoute,
})

function TeamsRoute() {
  const { user } = Route.useRouteContext()
  const { team } = Route.useSearch()
  return useIsMobile() ? <PhoneTeams me={user.id} teamSlug={team} /> : <Navigate to="/home" replace />
}
