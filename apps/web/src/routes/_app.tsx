import { useEffect } from "react"
import { createFileRoute, redirect, useNavigate, useRouterState } from "@tanstack/react-router"
import { useIsMobile } from "@exp/ui"
import { fetchSessionOnce, invalidateSessionCache } from "@/lib/auth/client"
import { hasCompletedOnboarding } from "@/lib/auth/app-user"
import { phonePathFor } from "@/lib/app-paths"
import { trpc } from "@/lib/trpc-client"
import { AppLayout } from "@/components/team/app-layout"

// #EXP-2027 (sidebar-tree.json): the TEAM-LESS pages of web md+ (Home,
// Chats, Needs you, Drafts, the device pages, the personal settings) under
// the same app chrome as a team's pages. There is no active team: the
// providers and the work tabs read the person's default team (their oldest
// membership, `teams.getDefault`), nothing on these pages is scoped by it.
// Phones keep today's team-scoped pages: a team-less URL forwards into the
// default team there.

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
  const isMobile = useIsMobile()
  const navigate = useNavigate()
  const location = useRouterState({ select: (s) => s.location })
  const phoneTarget = isMobile ? phonePathFor(location.pathname, defaultTeamSlug) : null
  useEffect(() => {
    if (phoneTarget) void navigate({ href: `${phoneTarget}${location.searchStr}`, replace: true })
  }, [phoneTarget, location.searchStr, navigate])
  if (phoneTarget) return null
  return <AppLayout teamSlug={defaultTeamSlug} user={user} />
}
