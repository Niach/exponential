import { createFileRoute, redirect } from "@tanstack/react-router"
import { queueLinkToast } from "@/lib/link-toast"
import { TRPCClientError } from "@trpc/client"
import { fetchSessionOnce } from "@/lib/auth/client"
import { trpc } from "@/lib/trpc-client"
import { teamCollection } from "@/lib/collections"
import { fastTeamGate, rememberTeamMembership } from "@/lib/team-route-gate"
import { AppLayout } from "@/components/team/app-layout"

/** The members-only `teams` shape holds this slug (synced rows only). */
function teamIsSynced(slug: string): boolean {
  for (const team of teamCollection.values()) {
    if ((team as { slug?: string }).slug === slug) return true
  }
  return false
}

export const Route = createFileRoute(`/t/$teamSlug`)({
  beforeLoad: async ({ params, location }) => {
    const slug = params.teamSlug
    // #PERF-7: a team this user's membership was confirmed for (and still
    // synced) passes without the two round trips (`lib/team-route-gate.ts`).
    if (slug !== `default` && typeof window !== `undefined`) {
      const fast = fastTeamGate(slug, teamIsSynced, () => {
        if (!window.location.pathname.startsWith(`/t/`)) return
        const back = `${window.location.pathname}${window.location.search}`
        window.location.assign(`/auth/login?redirect=${encodeURIComponent(back)}`)
      })
      if (fast) return { session: fast.session, user: fast.user }
    }
    const sessionData = await fetchSessionOnce()
    const session = sessionData?.session ?? null
    const user = sessionData?.user ?? null

    // Magic "default" slug resolves to the user's default team. getDefault
    // never creates (EXP-188): a team-less user goes to the onboarding
    // create-or-join choice instead.
    if (slug === `default`) {
      if (!session) {
        throw redirect({
          to: `/auth/login`,
          search: { redirect: location.href },
        })
      }
      const { team } = await trpc.teams.getDefault.query()
      if (!team) {
        throw redirect({ to: `/onboarding` })
      }
      if (team.slug !== `default`) {
        // Keep the rest of the path (`/t/default/agent` = the phone landing).
        throw redirect({
          href: location.href.replace(/^\/t\/default(?=\/|\?|#|$)/, `/t/${team.slug}`),
        })
      }
      return { session, user }
    }

    // Members-only lookup (EXP-180 removed public boards): getBySlug 404s for
    // everyone but members, so any failure funnels into the recovery below.
    try {
      await trpc.teams.getBySlug.query({ slug })
      if (user?.id) rememberTeamMembership(user.id, slug)
      return { session, user }
    } catch (e) {
      const isNotFound =
        e instanceof TRPCClientError && e.data?.code === `NOT_FOUND`
      if (!isNotFound) throw e
      // The team either doesn't exist or is private and we can't read it.
      // If we have no session, sending the user to login is the best
      // recovery — after sign-in they might gain access. Every board/issue
      // deep link funnels through here, so carry the destination along:
      // `location.href` is the full origin-stripped URL of the navigation
      // (not just this layout segment), and login re-clamps it with
      // sanitizeRedirectPath.
      if (!session) {
        throw redirect({
          to: `/auth/login`,
          search: { redirect: location.href },
        })
      }
      // Signed in but not a member (e.g. a link from another account,
      // EXP-1177): a bare "Not Found" page is a dead end, so go home — the
      // root redirect resolves this user's own last-used or default team.
      // #EXP-1674: with the reason (app-link.json `noAccess`), the same words
      // whether the team is private or gone.
      queueLinkToast()
      throw redirect({ to: `/` })
    }
  },
  component: TeamLayout,
})

function TeamLayout() {
  const { teamSlug } = Route.useParams()
  const { user } = Route.useRouteContext()
  return <AppLayout teamSlug={teamSlug} user={user} />
}
