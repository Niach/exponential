import {
  createFileRoute,
  Outlet,
  notFound,
  redirect,
  useParams,
} from "@tanstack/react-router"
import { TRPCClientError } from "@trpc/client"
import { useEffect, useState } from "react"
import { fetchSessionOnce } from "@/lib/auth/client"
import { rememberLastVisited } from "@/lib/last-visited"
import { trpc } from "@/lib/trpc-client"
import { SidebarProvider } from "@/components/ui/sidebar"
import { TeamMobileTopbar } from "@/components/team/mobile-topbar"
import { MobileTabBar } from "@/components/team/mobile-tab-bar"
import { TeamSidebar } from "@/components/team/sidebar"
import { IssueSearchSheet } from "@/components/issue-search-sheet"
import { OfflineBanner } from "@/components/offline-banner"
import { FeedbackWidgetProvider } from "@/components/feedback-widget-provider"
import { WebMcpProvider } from "@/components/webmcp-provider"
import { IssueRefProvider } from "@/components/issue-ref-provider"
import { TeamStatusesProvider } from "@/hooks/use-team-statuses"
import {
  GettingStartedProgressProvider,
  useGettingStartedProgress,
} from "@/hooks/use-getting-started-progress"
import { MentionProvider } from "@/components/mention-provider"
import { AgentDockProvider } from "@/components/agent-dock/agent-dock-provider"
import { AgentDock } from "@/components/agent-dock/agent-dock"
import { GettingStartedSheetProvider } from "@/components/getting-started/getting-started-sheet"
import { IssueSearchProvider } from "@/hooks/use-issue-search"
import {
  useTeamBySlug,
  useTeamBoards,
} from "@/hooks/use-team-data"

export const Route = createFileRoute(`/t/$teamSlug`)({
  beforeLoad: async ({ params, location }) => {
    const slug = params.teamSlug
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
        throw redirect({
          to: `/t/$teamSlug`,
          params: { teamSlug: team.slug },
        })
      }
      return { session, user }
    }

    // Members-only lookup (EXP-180 removed public boards): getBySlug 404s for
    // everyone but members, so any failure funnels into the recovery below.
    try {
      await trpc.teams.getBySlug.query({ slug })
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
      throw notFound()
    }
  },
  component: TeamLayout,
})

function TeamLayout() {
  const { teamSlug } = Route.useParams()
  const { user } = Route.useRouteContext()
  const team = useTeamBySlug(teamSlug)
  const boards = useTeamBoards(team?.id)
  const [searchOpen, setSearchOpen] = useState(false)
  // EXP-548: ONE getting-started signal pass per team layout, shared by the
  // sidebar entry (which hides once complete) and the empty-board block.
  const gettingStarted = useGettingStartedProgress(team)
  // Child-route params (loose match): `boardSlug` is set while any
  // board-scoped route (board, issue detail) is active.
  const { boardSlug } = useParams({ strict: false })

  // EXP-69: remember this device's last-used team/board so the root
  // redirect can jump straight back on the next app entry.
  useEffect(() => {
    rememberLastVisited(teamSlug, boardSlug)
  }, [teamSlug, boardSlug])

  // Linear-style global search shortcut: Cmd/Ctrl+F always opens the app
  // search, unconditionally (mirrors the Cmd+B sidebar-toggle handler in
  // `components/ui/sidebar.tsx`).
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === `f` && (event.metaKey || event.ctrlKey)) {
        event.preventDefault()
        setSearchOpen(true)
      }
    }
    window.addEventListener(`keydown`, handleKeyDown)
    return () => window.removeEventListener(`keydown`, handleKeyDown)
  }, [])

  return (
    <SidebarProvider>
      {/* Team-scoped `#IDENTIFIER` + `@email` resolution for pill
          rendering, the editor/composer autocompletes and the duplicate-of
          picker. */}
      {/* EXP-314: one live query for the team's issue_statuses rows, shared by
          every status renderer below (lists, pickers, filter pills, submenus)
          — a 200-row list must not open 200 queries. */}
      <TeamStatusesProvider teamId={team?.id}>
      <GettingStartedProgressProvider value={gettingStarted}>
      <IssueRefProvider
        teamId={team?.id}
        teamSlug={teamSlug}
      >
        <MentionProvider teamId={team?.id}>
          {/* The agent-coding dock (EXP-106) lives at layout level so it
              survives $teamSlug param changes and pins to the viewport. */}
          <AgentDockProvider teamId={team?.id ?? ``}>
          {/* EXP-686: the board header's Search button (mobile) and the
              Actions/Automations lightbulb reach the layout's sheets through
              context instead of a prop drilled through every list. The
              Getting started sheet lives here so the lightbulb can open it
              once the checklist is complete and its sidebar entry is gone. */}
          <IssueSearchProvider value={{ open: () => setSearchOpen(true) }}>
          <GettingStartedSheetProvider teamSlug={teamSlug} team={team}>
            <FeedbackWidgetProvider />
            {team && user && <WebMcpProvider team={team} user={user} />}
            <TeamSidebar
              teamSlug={teamSlug}
              team={team}
              boards={boards}
              onOpenSearch={() => setSearchOpen(true)}
            />

            {/* `min-w-0` on both the flex child and the content wrapper is
                what keeps ANY wide descendant from widening the whole page
                (flex children default to min-width:auto); `overflow-x-clip`
                contains stragglers inside the content region. */}
            <main className="flex-1 flex flex-col min-h-screen min-w-0">
              {/* EXP-533: above the mobile topbar (which is `md:hidden` and
                  hides itself on detail routes), so the "showing cached data"
                  notice is the first thing in the content column on every
                  breakpoint. Renders nothing while the server is reachable. */}
              <OfflineBanner />
              <TeamMobileTopbar
                teamSlug={teamSlug}
                team={team}
                boards={boards}
              />
              <div className="flex-1 min-h-0 min-w-0 overflow-x-clip">
                <Outlet />
              </div>
              {team && user && (
                <AgentDock
                  teamId={team.id}
                  teamSlug={teamSlug}
                  currentUserId={user.id}
                />
              )}
            </main>

            {/* Native-style bottom navigation (EXP-189) — fixed-position,
                so JSX placement only affects stacking. */}
            <MobileTabBar teamSlug={teamSlug} team={team} boards={boards} />

            {team && (
              <IssueSearchSheet
                open={searchOpen}
                onOpenChange={setSearchOpen}
                teamId={team.id}
                teamSlug={teamSlug}
              />
            )}
          </GettingStartedSheetProvider>
          </IssueSearchProvider>
          </AgentDockProvider>
        </MentionProvider>
      </IssueRefProvider>
      </GettingStartedProgressProvider>
      </TeamStatusesProvider>
    </SidebarProvider>
  )
}
