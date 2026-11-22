import { useNotificationToasts } from "@/hooks/use-notification-toasts"
import { isGlobalSearchKey } from "./global-search-shortcut"
import { Outlet, useNavigate, useParams, useRouterState } from "@tanstack/react-router"
import { useEffect, useMemo, useState } from "react"
import { rememberLastVisited } from "@/lib/last-visited"
import { widePathFor } from "@/lib/app-paths"
import type { SessionData } from "@/lib/auth/client"
import { SidebarProvider, useIsMobile } from "@exp/ui"
import { MobileTabBar } from "@/components/team/mobile-tab-bar"
import { AgentLoginDialogHost } from "@/components/agent-login-dialog"
import { LaunchDialogHost } from "@/components/launch-dialog/launch-dialog"
import { SidebarWidthSync, TeamSidebar } from "@/components/team/sidebar"
import { AssistantFab } from "@/components/team/assistant-fab"
import {
  MAIN_COLUMN_CLASS,
  MAIN_OUTLET_CLASS,
  mainPanelClass,
  WORK_TABS_BAND_CLASS,
} from "@/components/team/app-shell"
import { WorkTabsStrip } from "@/components/team/work-tabs-strip"
import { WorkTabsSync } from "@/components/team/work-tabs-sync"
import { useSidebarOccupant } from "@/hooks/use-sidebar-occupant"
import { IssueSearchSheet } from "@/components/issue-search-sheet"
import { OfflineBanner } from "@/components/offline-banner"
import { FeedbackWidgetProvider } from "@/components/feedback-widget-provider"
import { WebMcpProvider } from "@/components/webmcp-provider"
import { IssueRefProvider } from "@/components/issue-ref-provider"
import { CardLinksHost } from "@/components/card-links-host"
import { TeamStatusesProvider } from "@/hooks/use-team-statuses"
import {
  GettingStartedProgressProvider,
  useGettingStartedProgress,
} from "@/hooks/use-getting-started-progress"
import { MentionProvider } from "@/components/mention-provider"
import { IssueContextMenuProvider } from "@/components/issue-context-menu/provider"
import { IssueSearchProvider } from "@/hooks/use-issue-search"
import { MobileChromeProvider } from "@/hooks/use-mobile-chrome"
import { useSteerSessionReaper } from "@/hooks/use-steer-session-reaper"
import { useKeyHolder } from "@/lib/enet/use-key-holder"
import { useAppIconBadge } from "@/hooks/use-app-icon-badge"
import { useWebInstall } from "@/lib/enet/use-web-install"
import { startChatStore, stopChatStore } from "@/lib/chat-store/client"
import {
  useTeamBySlug,
  useTeamBoards,
  useTeamMemberships,
  useBoardsForTeams,
} from "@/hooks/use-team-data"
import { compareTeams } from "@/hooks/use-cross-team-scope"


interface AppLayoutProps {
  /** The team the providers and work tabs read: the route's team on a
   *  team-scoped page, the person's default team on a team-less one
   *  (#EXP-2027: the sidebar has no active team). */
  teamSlug: string
  user: SessionData[`user`] | null | undefined
}

// The app chrome around every page: the sidebar, the work tabs, the content
// card and the providers. Rendered by `/t/$teamSlug` and (#EXP-2027) the
// team-less `_app` layout.
export function AppLayout({ teamSlug, user }: AppLayoutProps) {
  const team = useTeamBySlug(teamSlug)
  const boards = useTeamBoards(team?.id)
  const [searchOpen, setSearchOpen] = useState(false)
  // EXP-548: ONE getting-started signal pass per team layout, shared by the
  // sidebar entry (which hides once complete) and the empty-board block.
  const gettingStarted = useGettingStartedProgress(team)
  // Child-route params (loose match): `boardSlug` is set while any
  // board-scoped route (board, issue detail) is active.
  const { boardSlug } = useParams({ strict: false })
  const isMobile = useIsMobile()
  // #EXP-2027: md+ has no active team: an old team-scoped URL of a page that
  // went team-less (Agent → Home, Chats, Needs you, Drafts, personal
  // settings…) moves to it. Phones keep today's pages.
  const navigate = useNavigate()
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const searchStr = useRouterState({ select: (s) => s.location.searchStr })
  const wideTarget = isMobile ? null : widePathFor(pathname)
  useEffect(() => {
    if (wideTarget) void navigate({ href: `${wideTarget}${searchStr}`, replace: true })
  }, [wideTarget, searchStr, navigate])
  // EXP-69: remember this device's last-used team/board so the root
  // redirect can jump straight back on the next app entry.
  useEffect(() => {
    rememberLastVisited(teamSlug, boardSlug)
  }, [teamSlug, boardSlug])
  // The steer-store reaper runs here, on every breakpoint — no sidebar or
  // work-tab surface is mounted on phones, so none of them can own the
  // sockets.
  useSteerSessionReaper(team?.id, user?.id)
  // EXP-1457/1459: this browser install's eNet node and its tree keys run
  // under the team app too (`_authenticated` does not wrap /t/$teamSlug).
  useWebInstall(user?.id)
  useKeyHolder(user?.id)
  // #EDE-115/#EDE-99: every chat on this browser (OPFS) under the team app
  // too: find in chat's older hits and the global search's Chats read it.
  const storeUserId = user?.id
  useEffect(() => {
    if (!storeUserId) return
    startChatStore(storeUserId)
    return () => stopChatStore()
  }, [storeUserId])
  // #FEED-232: the OS app icon's count (every team's Needs you rows).
  useAppIconBadge(user?.id)
  // #EXP-1815: a new notification while the app is open = THE toast,
  // unless this page already shows it (notification-delivery.json).
  useNotificationToasts(user?.id)

  // The global search shortcut (EXP-870): #EXP-2025 Cmd/Ctrl+K everywhere,
  // Cmd/Ctrl+F unless an open chat took it first (#EDE-129 find in chat).
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (isGlobalSearchKey(event)) {
        event.preventDefault()
        setSearchOpen(true)
      }
    }
    window.addEventListener(`keydown`, handleKeyDown)
    return () => window.removeEventListener(`keydown`, handleKeyDown)
  }, [])

  // EXP-870: the sidebar is the main menu or (settings) the settings nav in
  // its place — the provider's width follows the URL, and the shadcn
  // gap/container animate it on the shared motion tokens. EXP-1156: each
  // width is the user's dragged one; `SidebarWidthSync` writes the column's
  // `--sidebar-width` onto the wrapper so a drag never re-renders this layout.
  const occupant = useSidebarOccupant()
  const [sidebarWrapper, setSidebarWrapper] = useState<HTMLDivElement | null>(
    null
  )
  // EXP-870: browser-like work tabs above the card, md+ only. EXP-1255: the
  // strip mounts with zero tabs too, so its "+" (a New session tab) is
  // reachable. EXP-1293: route-driven — no screen calls into it. Settings is
  // tab-less (desktop EXP-1192): the band keeps its height, without tabs.
  const showWorkTabs = !isMobile && Boolean(team)
  // #EXP-2027: the md+ search reads every member team (no active team); a
  // phone keeps the route's team.
  const { myTeams } = useTeamMemberships(isMobile ? undefined : user?.id)
  const teamsKey = myTeams.map((row) => `${row.id}:${row.slug}:${row.name}`).join(`|`)
  const searchTeams = useMemo(
    () =>
      isMobile || myTeams.length === 0
        ? team
          ? [{ id: team.id, slug: teamSlug }]
          : []
        : [...myTeams].sort(compareTeams).map((row) => ({ id: row.id, slug: row.slug })),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [isMobile, teamsKey, team?.id, teamSlug]
  )
  // #EXP-2027: the work tabs span every member team (md+ only), each tab
  // linking into its own entity's team.
  const tabTeams = useMemo(
    () => (isMobile ? [] : [...myTeams].sort(compareTeams).map((row) => ({ id: row.id, slug: row.slug }))),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [isMobile, teamsKey]
  )
  const tabTeamIds = useMemo(() => tabTeams.map((row) => row.id), [tabTeams])
  const { boards: tabBoards } = useBoardsForTeams(tabTeamIds)

  return (
    <SidebarProvider ref={setSidebarWrapper}>
      <SidebarWidthSync wrapper={sidebarWrapper} occupant={occupant.kind} />
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
        <CardLinksHost teamSlug={teamSlug}>
        <MentionProvider teamId={team?.id}>
        {/* EXP-1074: THE issue context menu — one host above every list, row
            and chip that opts in with `issueMenuProps`. */}
        <IssueContextMenuProvider team={team}>
          {/* EXP-686: the board header's Search button (mobile) reaches the
              layout's search through context instead of a prop drilled
              through every list. */}
          <IssueSearchProvider value={{ open: () => setSearchOpen(true) }}>
          {/* EXP-698 r5: the bulk bar takes the tab bar's slot on phones —
              both bars live under this provider so the one that is up hides
              the other. */}
          <MobileChromeProvider>
            <FeedbackWidgetProvider />
            {team && user && <WebMcpProvider team={team} user={user} />}
            <TeamSidebar
              teamSlug={teamSlug}
              team={team}
              boards={boards}
              onOpenSearch={() => setSearchOpen(true)}
              workTabs={showWorkTabs}
            />

            {/* EXP-723 + EXP-771: the content column is the CUTOUT panel —
                a rounded card floating on the page gradient from `md` up,
                full-bleed on phones — plus, BELOW it on the bare ground, the
                agent dock's band. The column owns the viewport height so the
                card can be `flex-1` and the band end flush with the window's
                bottom edge (IDE parity: its title band sits outside the card
                too). `min-w-0` on the column, the card and the content
                wrapper is what keeps ANY wide descendant from widening the
                whole page (flex children default to min-width:auto);
                `overflow-x-clip` contains stragglers inside the content
                region. */}
            {!isMobile && <WorkTabsSync teams={tabTeams} boards={tabBoards} />}
            <div className={MAIN_COLUMN_CLASS}>
              {showWorkTabs && team && (
                <div className={WORK_TABS_BAND_CLASS}>
                  {occupant.kind === `main` && (
                    <WorkTabsStrip teams={tabTeams} boards={tabBoards} />
                  )}
                </div>
              )}
              <main className={`${mainPanelClass({ tabs: showWorkTabs })} md:relative`}>
                {/* EXP-533: above the mobile topbar (which is `md:hidden` and
                    hides itself on detail routes), so the "showing cached
                    data" notice is the first thing in the content column on
                    every breakpoint. Renders nothing while the server is
                    reachable. */}
                <OfflineBanner />
                <div className={MAIN_OUTLET_CLASS}>
                  <Outlet />
                </div>
                {/* #EXP-1837: the Assistant FAB, bottom-right of the card
                    (wide only; hidden on the Assistant's own page). */}
                <AssistantFab team={team} teamSlug={teamSlug} />
              </main>
              {/* EXP-818: no dock band under the card — EXP-870 put every
                  live session in a work tab above it instead. */}
            </div>

            {/* Native-style bottom navigation (EXP-189) — fixed-position,
                so JSX placement only affects stacking. */}
            <MobileTabBar teamSlug={teamSlug} team={team} boards={boards} />
            {/* EXP-792 (EXP-747 A2): the one "Sign in to <agent>" dialog a
                failed remote start's toast or a machine row opens. */}
            <AgentLoginDialogHost />
            {/* EXP-1019: the ONE launcher every play button opens — the
                composer in a dialog over whatever the person was looking at,
                mounted here so a start from any surface reaches it. */}
            <LaunchDialogHost team={team} />

            {searchTeams.length > 0 && (
              <IssueSearchSheet
                open={searchOpen}
                onOpenChange={setSearchOpen}
                teams={searchTeams}
              />
            )}
          </MobileChromeProvider>
          </IssueSearchProvider>
        </IssueContextMenuProvider>
        </MentionProvider>
        </CardLinksHost>
      </IssueRefProvider>
      </GettingStartedProgressProvider>
      </TeamStatusesProvider>
    </SidebarProvider>
  )
}
