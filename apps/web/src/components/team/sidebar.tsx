import { useEffect, useLayoutEffect, useRef, useState } from "react"
import type { ReactNode } from "react"
import {
  Link,
  useNavigate,
  useParams,
  useRouter,
  useRouterState,
} from "@tanstack/react-router"
import {
  conceptIcon,
  getBoardIcon,
  TeamAvatar,
  Button,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  ResizeHandle,
  Separator,
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupAction,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@exp/ui"
import { useShowsReviews } from "@/hooks/use-nav-counts"
import { useSession } from "@/hooks/use-session"
import { cn } from "@/lib/utils"
import { firstName } from "@/lib/user-display"
import type { Board, Team } from "@/db/schema"
import { useTeamMemberships } from "@/hooks/use-team-data"
import { useSidebarOccupant } from "@/hooks/use-sidebar-occupant"
import { CreateBoardDialog } from "@/components/create-board-dialog"
import { CreateTeamDialog } from "@/components/create-team-dialog"
import { JoinTeamDialog } from "@/components/join-team-dialog"
import { SettingsSidebar } from "@/components/team/settings-sidebar"
import { TeamListNav } from "@/components/team/list-nav"
import { ReviewFilesNav } from "@/components/team/review-files-nav"
import { SidebarPinned } from "@/components/team/sidebar-pinned"
import { SidebarRunningSection } from "@/components/team/sidebar-running"
import { RecentRunsSidebar } from "@/components/team/recent-runs-nav"
import { mainPanelEdge } from "@/components/team/app-shell"
import {
  DraftsCountBadge,
  InboxUnreadBadge,
  ReviewsOpenBadge,
  TeamLiveDot,
  TeamSidebarRail,
  UserAvatar,
  UserMenuItems,
} from "@/components/team/sidebar-rail"
import { useSidebarDraftCount } from "@/hooks/use-issue-drafts"
import { useOpenNewDraft } from "@/hooks/use-open-new-draft"
import { useTeamLiveRuns } from "@/hooks/use-team-live-runs"
import { otherTeamsLive } from "@/lib/sessions/team-live-runs"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { panelOffset, type SidebarOccupant } from "@/lib/detail-origin"
import {
  SIDEBAR_HANDLE_WIDTH,
  SIDEBAR_KEYBOARD_STEP,
  resetSidebarWidth,
  setSidebarResizing,
  setSidebarWidth,
  sidebarColumnWidth,
  sidebarResizable,
  sidebarUnitScale,
  sidebarWidthCss,
  useSidebarResizing,
  useSidebarWidth,
  useSidebarWidthBounds,
  useSidebarWidths,
  type SidebarPanelKey,
} from "@/lib/sidebar-widths"
import { FeedbackButton } from "@/components/feedback-button"
import { GettingStartedButton } from "@/components/getting-started/getting-started-button"
import {
  ChangelogSheet,
  useWhatsNew,
  WHATS_NEW_CLEARANCE_CLASS,
  WhatsNewCard,
} from "@/components/whats-new"
import { resolveBoardTarget } from "@/components/team/mobile-tab-bar"

// EXP-317: the cross-client nav glyphs come from the shared registry
// (packages/icons/icons.json) so web, desktop, iOS and Android agree.
const NavActionsIcon = conceptIcon(`nav-actions`)
const NavAgentIcon = conceptIcon(`action-chat`)
const NavBoardsIcon = conceptIcon(`nav-boards`)
const NavCreateIssueIcon = conceptIcon(`nav-create-issue`)
const NavDevicesIcon = conceptIcon(`nav-devices`)
const NavDraftsIcon = conceptIcon(`nav-drafts`)
const NavInboxIcon = conceptIcon(`nav-inbox`)
const NavReviewsIcon = conceptIcon(`nav-reviews`)
const NavSearchIcon = conceptIcon(`nav-search`)
const NavSettingsIcon = conceptIcon(`nav-settings`)
const NavTeamSwitcherIcon = conceptIcon(`nav-team-switcher`)
const UiAddIcon = conceptIcon(`ui-add`)
const UiCheckIcon = conceptIcon(`ui-check`)
const UiInviteIcon = conceptIcon(`ui-invite`)

// EXP-862: every arm of the panel slot runs at the sidebar's COMPACT density
// — `SidebarMenuButton density="compact"` (EXP-962), whose rationale lives
// with the prop in `packages/ui/src/sidebar.tsx`.

// EXP-870: one motion for every width and slide in the column — the shared
// motion tokens (styles.css, packages/design-tokens).
// EXP-1156: a drag (`data-resizing` on the sidebar wrapper) switches it off,
// or every slot would trail the pointer by the whole duration.
const SLOT_MOTION = `duration-standard ease-standard motion-reduce:transition-none group-data-[resizing=true]/sidebar-wrapper:transition-none`

const OFFSET_CLASS = {
  [-1]: `-translate-x-full`,
  0: `translate-x-0`,
  1: `translate-x-full`,
} as const

interface TeamSidebarProps {
  teamSlug: string
  team: Team | null | undefined
  boards: Board[] | undefined
  onOpenSearch: () => void
  /** The work-tabs band shows above the card (its top moves down to 44px). */
  workTabs: boolean
}

export function TeamSidebar({
  teamSlug,
  team,
  boards,
  onOpenSearch,
  workTabs,
}: TeamSidebarProps) {
  const { data: session } = useSession()
  const navigate = useNavigate()
  const [createBoardOpen, setCreateBoardOpen] = useState(false)
  const [createTeamOpen, setCreateTeamOpen] = useState(false)
  const [joinTeamOpen, setJoinTeamOpen] = useState(false)
  const permissions = useTeamPermissions(team)
  const { isOwner, canCreate } = permissions
  // EXP-449: the header's New-issue button works from any team route — it
  // files onto the active board (else the device's last-used / first board,
  // same resolution as the mobile FAB). EXP-1170: it opens the New issue
  // page on a fresh draft.
  const { boardSlug } = useParams({ strict: false })
  const boardTarget = resolveBoardTarget(teamSlug, boards, boardSlug)
  const [whatsNewOpen, setWhatsNewOpen] = useState(false)
  // EXP-1022: the card's visibility lives here because the scroll content
  // reserves room under its last row only while the card floats over it.
  const whatsNew = useWhatsNew()
  const { myTeams } = useTeamMemberships(session?.user?.id)
  // EXP-1075: session lists are team-scoped, so the picker carries the only
  // hint that a run of mine is alive in a team I am not looking at.
  const teamLive = useTeamLiveRuns(session?.user?.id)
  const otherLive = otherTeamsLive(teamLive, team?.id)
  // EXP-878: the Drafts entry exists only while there IS a draft (EXP-1170:
  // other than the one open on the New issue page).
  const draftCount = useSidebarDraftCount(team?.id)
  const openNewDraft = useOpenNewDraft(teamSlug)
  const showsReviews = useShowsReviews(team ?? undefined, boards)
  // The guarded /t/$teamSlug layout is the only render site, so a session is
  // guaranteed — the reactive useSession store may still be pending on cold
  // load, and we render the authed chrome throughout rather than flash a
  // logged-out state.

  // Name-less accounts (Apple sign-in stores an empty name) fall back to the
  // email instead of a bare "?".
  const userLabel = session?.user?.name || session?.user?.email

  // EXP-456 / EXP-851 / EXP-870: the column's layout is a function of the URL
  // (`useSidebarOccupant`) — the MAIN menu alone, or the rail compacted to
  // icons beside the settings nav or the LIST NAV a detail brings along
  // (`?from=`). Every entry point (footer gear, mobile topbar menu, a list
  // row, deep links) drives the same swap, and a direct load lands settled.
  const router = useRouter()
  const locationHref = useRouterState({ select: (s) => s.location.href })
  // EXP-862: the composer's seeded ACTION (`/t/$teamSlug/agent?action=`). The
  // pinned row for that action is what reads as active then, so the Agent
  // entry must NOT — two highlighted rows would both claim the same page.
  const composerActionId = useRouterState({
    select: (s) => {
      const value = (s.location.search as { action?: unknown }).action
      return typeof value === `string` && value !== `` ? value : null
    },
  })
  const agentPage = useRouterState({
    select: (s) => s.location.pathname.endsWith(`/agent`),
  })
  const occupant = useSidebarOccupant()
  const inSettings = occupant.kind === `settings`
  const listOrigin = occupant.kind === `list` ? occupant.origin : null
  // EXP-916: a review detail's panel is its file tree, not a list.
  const reviewFiles = occupant.kind === `review`
  // EXP-923: the Agent page's Recent runs panel.
  const recentRuns = occupant.kind === `recent`
  // EXP-870: a panel is up → the rail compacts to its icon column instead of
  // leaving. Strictly derived, never a toggle.
  const compact = occupant.kind !== `main`

  // Last non-settings location — the back button returns exactly here (with
  // its search params), not merely to the team index. Read in the same
  // router-state snapshot as the occupant, or a pending navigation would
  // record a settings URL as the return target.
  const lastNonSettingsHref = useRef<string | null>(null)
  useEffect(() => {
    if (!inSettings) lastNonSettingsHref.current = locationHref
  }, [inSettings, locationHref])

  const handleSettingsBack = () => {
    const href = lastNonSettingsHref.current
    const base = `/t/${teamSlug}`
    // Guard against a stale href from another team (switched mid-session).
    if (
      href &&
      (href === base ||
        href.startsWith(`${base}/`) ||
        href.startsWith(`${base}?`))
    ) {
      router.history.push(href)
    } else {
      void navigate({ to: `/t/$teamSlug`, params: { teamSlug } })
    }
  }

  return (
    <>
      {/* No right hairline: since EXP-723 the content column is a 10px-inset
          rounded card whose own stroke is the divider (sidebar-on-ground). */}
      <Sidebar className="border-r-0!">
        <div className="flex h-full w-full flex-col overflow-hidden">
          {/* EXP-870: the header is FIXED above both the rail and the panel —
              the team picker, Search and New issue stay put in every state
              (desktop parity), instead of sliding out with the main menu. */}
          <SidebarHeader className="p-2">
            {/* EXP-449: Linear-style header row — team picker plus icon-only
                Search and New-issue actions. */}
            <div className="flex items-center gap-1">
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <SidebarMenuButton
                    className="min-w-0 flex-1 h-10"
                    aria-label="Team switcher"
                  >
                    <TeamAvatar name={team?.name ?? teamSlug} size={28} />
                    <span className="text-sm font-semibold truncate">
                      {team?.name ?? teamSlug}
                    </span>
                    {/* `overflow-visible!`: the menu button truncates its LAST
                        span, which would clip the corner dot. */}
                    <span className="relative ml-auto flex shrink-0 items-center overflow-visible!">
                      <NavTeamSwitcherIcon className="h-4 w-4" />
                      <TeamLiveDot
                        live={otherLive.any ? otherLive : undefined}
                        placement="corner"
                        title="Runs in other teams"
                        data-testid="team-switcher-live-dot"
                      />
                    </span>
                  </SidebarMenuButton>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start" className="w-56">
                  {myTeams.map((ws) => (
                    <DropdownMenuItem
                      key={ws.id}
                      onClick={() =>
                        navigate({
                          to: `/t/$teamSlug`,
                          params: { teamSlug: ws.slug },
                        })
                      }
                    >
                      <TeamAvatar name={ws.name} size={20} />
                      <span className="truncate">{ws.name}</span>
                      {ws.slug === teamSlug ? (
                        <UiCheckIcon className="ml-auto h-4 w-4" />
                      ) : (
                        <TeamLiveDot
                          live={teamLive.get(ws.id)}
                          placement="trailing"
                          title="Runs in this team"
                        />
                      )}
                    </DropdownMenuItem>
                  ))}
                  <DropdownMenuSeparator />
                  {/* Any signed-in user can create teams (EXP-188) — the
                      server's only gate is the free-tier owned-team cap. */}
                  <DropdownMenuItem onClick={() => setCreateTeamOpen(true)}>
                    <UiAddIcon className="h-4 w-4" />
                    New team
                  </DropdownMenuItem>
                  {/* EXP-870: desktop parity — the IDE's team menu joins by
                      invite link too. */}
                  <DropdownMenuItem onClick={() => setJoinTeamOpen(true)}>
                    <UiInviteIcon className="h-4 w-4" />
                    Join team
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
              {team && (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="size-8 shrink-0 text-muted-foreground hover:text-foreground"
                      aria-label="Search"
                      onClick={onOpenSearch}
                    >
                      <NavSearchIcon className="size-4" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Search</TooltipContent>
                </Tooltip>
              )}
              {canCreate && boardTarget && (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant="default"
                      size="icon"
                      className="size-8 shrink-0"
                      aria-label="New issue"
                      onClick={() => openNewDraft({ boardId: boardTarget.id })}
                    >
                      <NavCreateIssueIcon className="size-4" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>New issue</TooltipContent>
                </Tooltip>
              )}
            </div>
          </SidebarHeader>

          <Separator />

          <div className="flex min-h-0 flex-1">
            {/* EXP-870: the RAIL slot — the full main menu (its dragged
                `main` width, EXP-1156) or its 48px icon column beside a
                panel. Both stay mounted and cross-fade while the slot's width
                animates; `inert` takes the hidden one out of the tab order
                and the accessibility tree. */}
            <RailSlot compact={compact}>
              <PanelLayer
                panel="main"
                inert={compact}
                className={cn(
                  `transition-opacity`,
                  compact ? `opacity-0` : `opacity-100`
                )}
              >
                {/* EXP-1022: the scroll area runs to the footer's edge and the
                    What's-new card FLOATS over its bottom — rows slide behind
                    the card instead of the list ending where the card begins.
                    While the card is up the content keeps clearance under its
                    last row so that row can still scroll out from under it. */}
                <div className="relative flex min-h-0 flex-1 flex-col">
                  <SidebarContent
                    className={cn(whatsNew.visible && WHATS_NEW_CLEARANCE_CLASS)}
                  >
                    <SidebarGroup>
                      <SidebarGroupContent>
                        {/* EXP-699: mobile order — Inbox, Support, Devices,
                            Actions, Reviews. */}
                        <SidebarMenu>
                          <SidebarMenuItem>
                            <SidebarMenuButton asChild density="compact">
                              <Link to="/t/$teamSlug/inbox" params={{ teamSlug }}>
                                <NavInboxIcon className="h-4 w-4" />
                                <span>Inbox</span>
                              </Link>
                            </SidebarMenuButton>
                            <InboxUnreadBadge placement="row" />
                          </SidebarMenuItem>
                          {/* EXP-878: Drafts sits directly after Inbox and
                              exists only while the caller HAS a draft — a
                              permanent entry for a surface that is empty
                              almost always would be noise. */}
                          {draftCount > 0 && (
                            <SidebarMenuItem>
                              <SidebarMenuButton asChild density="compact">
                                <Link to="/t/$teamSlug/drafts" params={{ teamSlug }}>
                                  <NavDraftsIcon className="h-4 w-4" />
                                  <span>Drafts</span>
                                </Link>
                              </SidebarMenuButton>
                              <DraftsCountBadge teamId={team?.id} placement="row" />
                            </SidebarMenuItem>
                          )}
                          {/* EXP-686: Devices · Actions, the surfaces the old
                              Agents entry bundled (SLOP-2: an action carries
                              its triggers, so Automations is no entry). */}
                          <SidebarMenuItem>
                            <SidebarMenuButton asChild density="compact">
                              <Link to="/t/$teamSlug/devices" params={{ teamSlug }}>
                                <NavDevicesIcon className="h-4 w-4" />
                                <span>Devices</span>
                              </Link>
                            </SidebarMenuButton>
                          </SidebarMenuItem>
                          <SidebarMenuItem>
                            <SidebarMenuButton asChild density="compact">
                              <Link to="/t/$teamSlug/actions" params={{ teamSlug }}>
                                <NavActionsIcon className="h-4 w-4" />
                                <span>Actions</span>
                              </Link>
                            </SidebarMenuButton>
                          </SidebarMenuItem>
                          {showsReviews && (
                            <SidebarMenuItem>
                              <SidebarMenuButton asChild density="compact">
                                <Link to="/t/$teamSlug/reviews" params={{ teamSlug }}>
                                  <NavReviewsIcon className="h-4 w-4" />
                                  <span>Reviews</span>
                                </Link>
                              </SidebarMenuButton>
                              <ReviewsOpenBadge
                                boards={boards}
                                teamId={team?.id}
                                placement="row"
                              />
                            </SidebarMenuItem>
                          )}
                          {/* EXP-818: the Agent page — the composer over the
                              caller's past runs (the IDE rail's Agent entry).
                              EXP-1120: no live-run dot — live runs already list
                              in the sidebar's Running section. */}
                          <SidebarMenuItem>
                            <SidebarMenuButton
                              asChild
                              density="compact"
                              className={cn(
                                // A pinned action is driving the composer: its
                                // row owns the highlight (EXP-862).
                                agentPage &&
                                  composerActionId !== null &&
                                  `data-[status=active]:bg-transparent data-[status=active]:font-normal data-[status=active]:text-sidebar-foreground`
                              )}
                            >
                              <Link to="/t/$teamSlug/agent" params={{ teamSlug }}>
                                <NavAgentIcon className="h-4 w-4" />
                                <span>Agent</span>
                              </Link>
                            </SidebarMenuButton>
                          </SidebarMenuItem>
                        </SidebarMenu>
                      </SidebarGroupContent>
                    </SidebarGroup>

                    {/* EXP-778: the caller's pinned issues / sessions / actions
                        in this team — favourites above the boards, hidden when
                        empty. */}
                    {team && <SidebarPinned teamId={team.id} teamSlug={teamSlug} />}

                    <SidebarGroup>
                      <SidebarGroupLabel>Boards</SidebarGroupLabel>
                      <SidebarGroupAction
                        onClick={() => setCreateBoardOpen(true)}
                        title="Create board"
                        aria-label="Create board"
                      >
                        <UiAddIcon className="h-4 w-4" />
                      </SidebarGroupAction>
                      <SidebarGroupContent>
                        <SidebarMenu>
                          {!boards || boards.length === 0 ? (
                            <SidebarMenuItem>
                              <SidebarMenuButton disabled density="compact">
                                <NavBoardsIcon className="h-4 w-4" />
                                <span className="text-muted-foreground">
                                  No boards yet
                                </span>
                              </SidebarMenuButton>
                            </SidebarMenuItem>
                          ) : (
                            boards.map((board) => {
                              const TypeIcon = getBoardIcon(board)
                              return (
                                <SidebarMenuItem key={board.id}>
                                  <SidebarMenuButton asChild density="compact">
                                    <Link
                                      to="/t/$teamSlug/boards/$boardSlug"
                                      params={{
                                        teamSlug,
                                        boardSlug: board.slug,
                                      }}
                                    >
                                      <TypeIcon
                                        className="h-4 w-4 shrink-0"
                                        style={{ color: board.color }}
                                      />
                                      <span>{board.name}</span>
                                    </Link>
                                  </SidebarMenuButton>
                                  {isOwner && (
                                    <SidebarMenuAction
                                      asChild
                                      showOnHover
                                      title="Board settings"
                                    >
                                      <Link
                                        to="/t/$teamSlug/settings/boards/$boardId"
                                        params={{ teamSlug, boardId: board.id }}
                                        aria-label={`Settings for ${board.name}`}
                                      >
                                        <NavSettingsIcon />
                                      </Link>
                                    </SidebarMenuAction>
                                  )}
                                </SidebarMenuItem>
                              )
                            })
                          )}
                        </SidebarMenu>
                      </SidebarGroupContent>
                    </SidebarGroup>
                    {/* EXP-923: the RUNNING group — my live runs, nested, under
                        the boards. Hidden entirely when nothing runs. */}
                    <SidebarRunningSection
                      teamId={team?.id}
                      currentUserId={session?.user?.id}
                    />
                  </SidebarContent>
                  {/* EXP-164: dismissable "What's new" teaser for the latest
                      changelog entry — hidden again until the next release
                      once dismissed. Floating (EXP-1022): absolutely placed
                      over the scroll area's bottom edge, in the groups' 8px
                      gutter. */}
                  <div className="pointer-events-none absolute inset-x-0 bottom-0 px-2">
                    <WhatsNewCard
                      className="pointer-events-auto"
                      state={whatsNew}
                      onOpen={() => setWhatsNewOpen(true)}
                    />
                  </div>
                </div>

                <SidebarFooter>
                  <SidebarMenu>
                    {/* EXP-771: the ONLY way into the feedback widget now that
                        the in-app mount is headless. Cloud-only — self-hosted
                        renders nothing here. */}
                    <FeedbackButton />
                    {/* EXP-88: re-entry point for the Getting started cards
                        once the board's inline block is gone. */}
                    <GettingStartedButton />
                  </SidebarMenu>
                  {/* EXP-238: settings entry lives down here next to the user
                      block, like the IDE's rail gear. */}
                  <div className="flex items-center gap-1">
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <SidebarMenuButton
                          className="min-w-0 flex-1"
                          aria-label="User menu"
                        >
                          <UserAvatar />
                          {/* First name only (EXP-311) — the full name + email
                              live in account settings. */}
                          <span className="truncate text-sm">
                            {userLabel ? firstName(userLabel) : `Loading...`}
                          </span>
                          <NavTeamSwitcherIcon className="ml-auto h-4 w-4" />
                        </SidebarMenuButton>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent side="top" align="start" className="w-56">
                        <UserMenuItems onWhatsNew={() => setWhatsNewOpen(true)} />
                      </DropdownMenuContent>
                    </DropdownMenu>
                    <SidebarMenuButton
                      asChild
                      className="w-auto shrink-0"
                      aria-label="Settings"
                      tooltip="Settings"
                    >
                      <Link to="/t/$teamSlug/settings" params={{ teamSlug }}>
                        <NavSettingsIcon className="h-4 w-4" />
                      </Link>
                    </SidebarMenuButton>
                  </div>
                </SidebarFooter>
              </PanelLayer>

              <div
                inert={!compact}
                className={cn(
                  `absolute inset-y-0 left-0 flex w-12 flex-col transition-opacity`,
                  SLOT_MOTION,
                  compact ? `opacity-100` : `pointer-events-none opacity-0`
                )}
              >
                <TeamSidebarRail
                  teamSlug={teamSlug}
                  team={team}
                  boards={boards}
                  onWhatsNew={() => setWhatsNewOpen(true)}
                />
              </div>
            </RailSlot>

            {/* EXP-870: the PANEL slot — 0 wide with the main menu, the
                ACTIVE panel's dragged width (EXP-1156) beside the compact
                rail. Its panels slide by DIRECTION (`panelOffset`, in each
                layer's OWN width): going deeper a panel comes out from under
                the rail's edge, coming back it slides back under it. */}
            <PanelSlot occupant={occupant.kind}>
              <PanelLayer
                panel="settings"
                inert={!inSettings}
                className={cn(
                  `transition-transform`,
                  OFFSET_CLASS[panelOffset(`settings`, occupant.kind)]
                )}
              >
                <SettingsSidebar
                  teamSlug={teamSlug}
                  permissions={permissions}
                  onBack={handleSettingsBack}
                />
              </PanelLayer>

              {/* EXP-851: the LIST NAV — the list an open detail came from.
                  Mounted only while one is up: its lists run live queries and
                  tRPC polls, and an off-screen Support poll every 30s is not
                  free. */}
              <PanelLayer
                panel="list"
                inert={!listOrigin}
                className={cn(
                  `transition-transform`,
                  OFFSET_CLASS[panelOffset(`list`, occupant.kind)]
                )}
              >
                {listOrigin && (
                  <TeamListNav
                    // Keyed by the origin so switching lists remounts instead
                    // of carrying the previous list's tab/fold state over.
                    key={`${listOrigin.kind}:${
                      `boardSlug` in listOrigin ? listOrigin.boardSlug : ``
                    }`}
                    teamSlug={teamSlug}
                    team={team}
                    boards={boards}
                    origin={listOrigin}
                  />
                )}
              </PanelLayer>

              {/* EXP-916: the REVIEW's file tree — a review's context is the
                  files its pull request touches, so that panel sits beside
                  it where another detail keeps its list. Same depth as the
                  list nav, so the two never slide over each other. */}
              <PanelLayer
                panel="review"
                inert={!reviewFiles}
                className={cn(
                  `transition-transform`,
                  OFFSET_CLASS[panelOffset(`review`, occupant.kind)]
                )}
              >
                {reviewFiles && <ReviewFilesNav teamSlug={teamSlug} />}
              </PanelLayer>

              {/* EXP-923: the Agent page's RECENT runs, behind that page's
                  history toggle. Same slot and depth as the list nav — the
                  Agent route never has one, so the two can never collide. */}
              <PanelLayer
                panel="recent"
                inert={!recentRuns}
                className={cn(
                  `transition-transform`,
                  OFFSET_CLASS[panelOffset(`recent`, occupant.kind)]
                )}
              >
                {recentRuns && team && (
                  <RecentRunsSidebar
                    teamId={team.id}
                    currentUserId={session?.user?.id}
                  />
                )}
              </PanelLayer>
            </PanelSlot>
          </div>
        </div>
        {/* EXP-1156: the column's ONE drag edge — EXP-1163: the content
            card's left edge. It resizes the panel beside the rail; the main
            menu has none. */}
        <SidebarResizeEdge panel={occupant.kind} workTabs={workTabs} />
      </Sidebar>

      <ChangelogSheet open={whatsNewOpen} onOpenChange={setWhatsNewOpen} />
      {team && (
        <CreateBoardDialog
          open={createBoardOpen}
          onOpenChange={setCreateBoardOpen}
          team={team}
        />
      )}
      <CreateTeamDialog
        open={createTeamOpen}
        onOpenChange={setCreateTeamOpen}
      />
      <JoinTeamDialog open={joinTeamOpen} onOpenChange={setJoinTeamOpen} />
    </>
  )
}

// EXP-1156: the width-bearing pieces of the column are their OWN components,
// each subscribed to the width store. A drag then re-renders these few
// wrappers per frame while the menus inside them (passed in as `children`,
// created by `TeamSidebar`, which never subscribes) bail out untouched.

/** The rail slot: the main menu's width, or the 48px icon rail (`w-12`). */
function RailSlot({
  compact,
  children,
}: {
  compact: boolean
  children: ReactNode
}) {
  const main = useSidebarWidth(`main`)
  return (
    <div
      className={cn(
        `relative shrink-0 overflow-hidden transition-[width]`,
        SLOT_MOTION,
        compact && `w-12`
      )}
      style={compact ? undefined : { width: sidebarWidthCss(main) }}
    >
      {children}
    </div>
  )
}

/** The panel slot: the ACTIVE panel's width beside the rail, else 0. */
function PanelSlot({
  occupant,
  children,
}: {
  occupant: SidebarOccupant[`kind`]
  children: ReactNode
}) {
  const widths = useSidebarWidths()
  const compact = occupant !== `main`
  return (
    <div
      className={cn(
        `relative shrink-0 overflow-hidden transition-[width]`,
        SLOT_MOTION,
        compact ? `border-l border-glass-stroke` : `w-0`
      )}
      style={compact ? { width: sidebarWidthCss(widths[occupant]) } : undefined}
    >
      {children}
    </div>
  )
}

/** One stacked layer, at ITS OWN panel's width (so it slides by that). */
function PanelLayer({
  panel,
  inert,
  className,
  children,
}: {
  panel: SidebarPanelKey
  inert: boolean
  className?: string
  children: ReactNode
}) {
  const width = useSidebarWidth(panel)
  return (
    <div
      inert={inert}
      className={cn(
        `absolute inset-y-0 left-0 flex flex-col`,
        SLOT_MOTION,
        className
      )}
      style={{ width: sidebarWidthCss(width) }}
    >
      {children}
    </div>
  )
}

/**
 * The column's drag edge — EXP-1163: the content card's LEFT EDGE, not a
 * line in the gutter. The `Sidebar` container is `position: fixed` and
 * full-height, so this absolutely placed strip measures from the column's
 * right edge: it is centred on the card's 1px border (`gutter` + ½px out)
 * and spans only the card's height (`mainPanelEdge`, under the work-tabs band
 * when it shows), its hairline clear of the card's rounded corners. Never on
 * a phone: there `Sidebar` is the (retired) Sheet, and phones keep no sidebar
 * widths at all.
 */
function SidebarResizeEdge({
  panel,
  workTabs,
}: {
  panel: SidebarPanelKey
  workTabs: boolean
}) {
  const { isMobile } = useSidebar()
  const width = useSidebarWidth(panel)
  const { min, max } = useSidebarWidthBounds(panel)
  // The main menu stays as it is: only a panel beside the rail drags.
  if (isMobile || !sidebarResizable(panel)) return null
  const card = mainPanelEdge({ tabs: workTabs })
  return (
    <ResizeHandle
      aria-label="Resize sidebar"
      className="z-20"
      // PX, like the card's insets: the strip's centre is the centre of the
      // card's border pixel, `gutter + ½` past the column.
      style={{
        right: -(card.gutter + 0.5 + SIDEBAR_HANDLE_WIDTH / 2),
        width: SIDEBAR_HANDLE_WIDTH,
        top: card.top,
        bottom: card.bottom,
      }}
      inset={card.radius}
      value={width}
      min={min}
      max={max}
      step={SIDEBAR_KEYBOARD_STEP}
      scale={sidebarUnitScale()}
      onChange={(next) => setSidebarWidth(panel, next, { persist: false })}
      onCommit={(next) => setSidebarWidth(panel, next)}
      onReset={() => resetSidebarWidth(panel)}
      onDraggingChange={setSidebarResizing}
    />
  )
}

/**
 * EXP-1156: the provider's `--sidebar-width` (the fixed container and the
 * layout gap) and its `data-resizing` flag, written straight onto the wrapper
 * from the width store. Imperative on purpose: the team layout renders every
 * provider and the page, and re-rendering it per drag frame would be the
 * whole app; this component re-renders alone.
 */
export function SidebarWidthSync({
  wrapper,
  occupant,
}: {
  /** The provider's wrapper ELEMENT (a callback ref's state, not a ref
   *  object: this effect runs before a parent's ref attaches on mount). */
  wrapper: HTMLDivElement | null
  occupant: SidebarOccupant[`kind`]
}) {
  const widths = useSidebarWidths()
  const resizing = useSidebarResizing()
  const column = sidebarWidthCss(sidebarColumnWidth(widths, occupant))
  useLayoutEffect(() => {
    if (!wrapper) return
    wrapper.style.setProperty(`--sidebar-width`, column)
    if (resizing) wrapper.dataset.resizing = `true`
    else delete wrapper.dataset.resizing
  }, [wrapper, column, resizing])
  return null
}
