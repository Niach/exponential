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
import { FeedbackWidgetProvider } from "@/components/feedback-widget-provider"
import { WebMcpProvider } from "@/components/webmcp-provider"
import { IssueRefProvider } from "@/components/issue-ref-provider"
import { MentionProvider } from "@/components/mention-provider"
import { AgentDockProvider } from "@/components/agent-dock/agent-dock-provider"
import { AgentDock } from "@/components/agent-dock/agent-dock"
import {
  useTeamBySlug,
  useTeamBoards,
} from "@/hooks/use-team-data"

export const Route = createFileRoute(`/t/$teamSlug`)({
  beforeLoad: async ({ params }) => {
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
          search: { redirect: undefined },
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
      // recovery — after sign-in they might gain access.
      if (!session) {
        throw redirect({
          to: `/auth/login`,
          search: { redirect: undefined },
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
      <IssueRefProvider
        teamId={team?.id}
        teamSlug={teamSlug}
      >
        <MentionProvider teamId={team?.id}>
          {/* The agent-coding dock (EXP-106) lives at layout level so it
              survives $teamSlug param changes and pins to the viewport. */}
          <AgentDockProvider teamId={team?.id ?? ``}>
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
            <MobileTabBar
              teamSlug={teamSlug}
              team={team}
              boards={boards}
              onOpenSearch={() => setSearchOpen(true)}
            />

            {team && (
              <IssueSearchSheet
                open={searchOpen}
                onOpenChange={setSearchOpen}
                teamId={team.id}
                teamSlug={teamSlug}
              />
            )}
          </AgentDockProvider>
        </MentionProvider>
      </IssueRefProvider>
    </SidebarProvider>
  )
}
