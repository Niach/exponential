import { createFileRoute, redirect } from "@tanstack/react-router"
import { fetchSessionOnce, invalidateSessionCache } from "@/lib/auth/client"
import { hasCompletedOnboarding } from "@/lib/auth/app-user"
import { trpc } from "@/lib/trpc-client"
import { AppLayout } from "@/components/team/app-layout"

// #EXP-2027 (sidebar-tree.json), #EXP-2109 (phones): the TEAM-LESS pages (Home,
// Chats, Needs you, the Inbox, Drafts, the device pages, the personal settings,
// the phones' Teams and Assistant tabs) under
// the same app chrome as a team's pages. There is no active team: the
// providers and the work tabs read the person's default team (their oldest
// membership, `teams.getDefault`), nothing on these pages is scoped by it.

/** The default team's slug for this page life (one round trip per user). */
let defaultTeam = null as { userId: string; slug: string } | null

export const Route = createFileRoute(`/_app`)({
  ssr: false,
  beforeLoad: async ({ location }) => {
    let sessionData = await fetchSessionOnce()
    if (!sessionData?.user) {
      throw redirect({ to: `/auth/login`, search: { redirect: location.href } })
    }
    // #EXP-2050: onboarding just completed (the wizard's board step, an
    // accepted invite) while the cached session still says it hasn't: read
    // it fresh before sending the person back into the wizard.
    if (!hasCompletedOnboarding(sessionData.user)) {
      invalidateSessionCache()
      sessionData = await fetchSessionOnce()
    }
    if (!sessionData?.user) {
      throw redirect({ to: `/auth/login`, search: { redirect: location.href } })
    }
    if (!hasCompletedOnboarding(sessionData.user)) throw redirect({ to: `/onboarding` })
    const userId = sessionData.user.id
    const cached = defaultTeam
    let slug = cached && cached.userId === userId ? cached.slug : null
    if (!slug) {
      const { team } = await trpc.teams.getDefault.query()
      if (!team) throw redirect({ to: `/onboarding` })
      slug = team.slug
      defaultTeam = { userId, slug }
    }
    return { session: sessionData.session, user: sessionData.user, defaultTeamSlug: slug }
  },
  component: TeamlessLayout,
})

function TeamlessLayout() {
  const { user, defaultTeamSlug } = Route.useRouteContext()
  return <AppLayout teamSlug={defaultTeamSlug} user={user} />
}
