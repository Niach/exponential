import { createFileRoute, redirect } from "@tanstack/react-router"
import { repoTab, type RepoTab } from "@exp/domain-contract/repo-page"
import { SkeletonRows } from "@exp/ui"
import { RepoPage } from "@/components/repo/repo-page"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { pageTitle } from "@/lib/page-title"

// #EXP-1616 (repo-page.json): THE repo page, opened from a Settings ›
// Repositories row (no sidebar entry, Q4). `?tab=` picks the tab
// (`repoTab`: unknown = pulls); `/pulls/<n>` (an eCode PR's url) lands here.
export const Route = createFileRoute(`/t/$teamSlug/repos/$name`)({
  head: ({ params }) => ({ meta: [{ title: pageTitle(params.name) }] }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({ to: `/auth/login`, search: { redirect: location.href } })
    }
  },
  validateSearch: (search: Record<string, unknown>): { tab?: RepoTab } =>
    typeof search.tab === `string` ? { tab: repoTab(search.tab) } : {},
  component: RepoRoute,
})

function RepoRoute() {
  const { teamSlug, name } = Route.useParams()
  const { tab } = Route.useSearch()
  const team = useTeamBySlug(teamSlug)
  const { isOwner, isMember } = useTeamPermissions(team)
  if (!team) {
    return (
      <div className="mx-auto w-full max-w-3xl px-4 py-4">
        <SkeletonRows rowHeight={52} />
      </div>
    )
  }
  return <RepoPage teamId={team.id} teamSlug={teamSlug} name={name} tab={tab ?? `pulls`} isOwner={isOwner} isMember={isMember} />
}
