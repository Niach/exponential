import { useMemo } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { DraftsList } from "@/components/drafts-list"
import { useMemberTeams } from "@/components/home/use-member-teams"
import { pageTitle } from "@/lib/page-title"

// #EXP-2027 (sidebar-tree.json ROOT `drafts`): the TEAM-LESS Drafts page of web
// md+, the team's Drafts list (EXP-878) over EVERY member team: each row
// wears its team mark and reopens on its own team's New issue page
// (`/t/<team>/drafts/<id>`). Phones keep their inbox tab.

export const Route = createFileRoute(`/_app/drafts`)({
  head: () => ({ meta: [{ title: pageTitle(`Drafts`) }] }),
  component: DraftsRoute,
})

function DraftsRoute() {
  const { user, defaultTeamSlug } = Route.useRouteContext()
  const memberTeams = useMemberTeams(user.id)
  const teams = useMemo(
    () =>
      memberTeams.teams.map((team) => ({ id: team.id, slug: team.slug, name: team.name, iconUrl: team.iconUrl ?? null })),
    [memberTeams.teams]
  )
  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto w-full max-w-3xl px-4 py-4 md:max-w-5xl">
        <DraftsList teamId={undefined} teamSlug={defaultTeamSlug} teams={teams} />
      </div>
    </div>
  )
}
