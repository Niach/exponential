import { createFileRoute, redirect } from "@tanstack/react-router"
import { PhoneTeam } from "@/components/team/phone-teams"
import { pageTitle } from "@/lib/page-title"

// #EXP-2109 (sidebar-tree.json `phone`): a team's page of the phones' Teams
// tab; `?folder=` = one of its folders. md+ forwards to Home (`widePathFor`).
export const Route = createFileRoute(`/t/$teamSlug/team`)({
  head: () => ({ meta: [{ title: pageTitle(`Team`) }] }),
  validateSearch: (search: Record<string, unknown>): { folder?: string } =>
    typeof search.folder === `string` && search.folder ? { folder: search.folder } : {},
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({ to: `/auth/login`, search: { redirect: location.href } })
    }
  },
  component: TeamRoute,
})

function TeamRoute() {
  const { teamSlug } = Route.useParams()
  const { folder } = Route.useSearch()
  return <PhoneTeam key={`${teamSlug}:${folder ?? ``}`} teamSlug={teamSlug} folderId={folder ?? null} />
}
