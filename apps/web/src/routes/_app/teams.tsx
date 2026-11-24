import { createFileRoute, Navigate } from "@tanstack/react-router"
import { useIsMobile } from "@exp/ui"
import { PhoneTeams } from "@/components/team/phone-teams"
import { pageTitle } from "@/lib/page-title"

// #EXP-2109: the phones' Teams tab; md+ has the sidebar.
export const Route = createFileRoute(`/_app/teams`)({
  head: () => ({ meta: [{ title: pageTitle(`Teams`) }] }),
  component: TeamsRoute,
})

function TeamsRoute() {
  const { user } = Route.useRouteContext()
  return useIsMobile() ? <PhoneTeams me={user.id} /> : <Navigate to="/home" replace />
}
