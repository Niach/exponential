import { CHATS_COPY } from "@exp/domain-contract/chats"
import { useApps } from "@/hooks/use-apps"
import { SidebarAppsSection } from "@/components/team/sidebar-apps"
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react"
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
  Badge,
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
  SidebarMenuTrailing,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
  blockingLabel,
  whileBlocking,
} from "@exp/ui"
import { NEEDS_YOU_COPY, NEEDS_YOU_TEST_IDS } from "@exp/domain-contract/needs-you"
import { InboxPopover } from "@/components/team/inbox-popover"
import { useNeedsYou, useNeedsYouCounts } from "@/hooks/use-needs-you"
import { appIconBadge, chatsBadge } from "@exp/domain-contract/nav-badges"
import { useAgents, useSessionReads, useChatsList } from "@/hooks/use-session-tree"
import { chatWordOf, useTeamPullRequestsByRoot } from "@/components/chat/chats-list"
import { useSession } from "@/hooks/use-session"
import { cn } from "@/lib/utils"
import { firstName } from "@/lib/user-display"
import type { Board, Team } from "@/db/schema"
import { useTeamMemberships } from "@/hooks/use-team-data"
import { useSidebarOccupant } from "@/hooks/use-sidebar-occupant"
import { CreateBoardDialog } from "@/components/create-board-dialog"
import { Empty } from "@/components/empty"
import { CreateTeamDialog } from "@/components/create-team-dialog"
import { JoinTeamDialog } from "@/components/join-team-dialog"
import { SettingsSidebar } from "@/components/team/settings-sidebar"
import { SidebarPinned } from "@/components/team/sidebar-pinned"
import { SidebarRunningSection, type SidebarChats } from "@/components/team/sidebar-running"
import { SidebarAgentsSection, useAgentBadges } from "@/components/team/sidebar-agents"
import { sidebarAgents } from "@exp/domain-contract/chats"
import { SidebarFolderList } from "@/components/team/sidebar-folders"
import { mainPanelEdge } from "@/components/team/app-shell"
import { NavCountBadge, UserAvatar, UserMenuItems } from "@/components/team/sidebar-shared"
import { useOpenNewDraft } from "@/hooks/use-open-new-draft"
import {
  DRAFTS_LABEL,
  NavDraftsIcon,
  useDraftCount,
} from "@/components/team/sidebar-nav-entries"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import type { SidebarOccupant } from "@/lib/detail-origin"
import {
  SIDEBAR_HANDLE_WIDTH,
  SIDEBAR_KEYBOARD_STEP,
  resetSidebarWidth,
  setSidebarResizing,
  setSidebarWidth,
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
const NavAgentIcon = conceptIcon(`action-chat`)
const NavCreateIssueIcon = conceptIcon(`nav-create-issue`)
const NavDevicesIcon = conceptIcon(`nav-devices`)
const NavNeedsYouIcon = conceptIcon(`nav-needs-you`)
const NavChatsIcon = conceptIcon(`nav-chats`)
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
  // #FEED-232 (nav-badges.json): the picker counts each OTHER team's Needs
  // you rows; the switcher's corner sums them (`appIconBadge`).
  const myTeamIds = useMemo(() => myTeams.map((t) => t.id), [myTeams])
  const teamNeedsYou = useNeedsYouCounts(myTeamIds, session?.user?.id)
  const otherNeedsYou = appIconBadge(
    myTeamIds.filter((id) => id !== team?.id).map((id) => ({ needsYou: teamNeedsYou.byTeam.get(id) ?? 0 }))
  )
  const openNewDraft = useOpenNewDraft(teamSlug)
  // EXP-878: the Drafts entry lists only while a draft is parked.
  const draftCount = useDraftCount(team?.id)
  // #FEED-191 (needs-you.json): Needs you lists only while a card waits.
  const needsYou = useNeedsYou(team?.id, session?.user?.id)
  // EXP-1293: ONE Chats list read for the sidebar — the Chats entry's
  // dot, and the Running section.
  const chatsList = useChatsList(team?.id)
  const teamPrs = useTeamPullRequestsByRoot(team?.id)
  const chats: SidebarChats = {
    groups: chatsList.groups,
    wordOf: (row) => chatWordOf(row, teamPrs.byRoot.get(row.sessionId) ?? [], teamPrs.all),
  }
  const sessionReads = useSessionReads()
  const chatsCount = chatsBadge(chatsList.groups, sessionReads)
  const { agents } = useAgents(team?.id, session?.user?.id)
  const agentBadges = useAgentBadges(team?.id, agents, session?.user?.id)
  // #EXP-1832: the boards in today's order = the folders' default order.
  const boardIds = useMemo(() => (boards ?? []).map((board) => board.id), [boards])
  const boardsById = useMemo(() => new Map((boards ?? []).map((board) => [board.id, board])), [boards])
  // The guarded /t/$teamSlug layout is the only render site, so a session is
  // guaranteed — the reactive useSession store may still be pending on cold
  // load, and we render the authed chrome throughout rather than flash a
  // logged-out state.

  // Name-less accounts (Apple sign-in stores an empty name) fall back to the
  // email instead of a bare "?".
  const userLabel = session?.user?.name || session?.user?.email

  // EXP-456 / EXP-851: the column's occupant is a function of the URL
  // (`useSidebarOccupant`) — the MAIN menu, or the settings nav in its place
  // (desktop `left_occupant_for`). Nothing folds to icons. Every entry point
  // drives the same swap, and a direct load lands settled.
  const router = useRouter()
  const locationHref = useRouterState({ select: (s) => s.location.href })
  const activeAgentId = useRouterState({
    select: (s) => /\/agents\/([^/]+)$/.exec(s.location.pathname)?.[1] ?? null,
  })
  const activeAppSlug = useRouterState({
    // VAPP-242: the row stays active on a screen deep link and in the editor.
    select: (s) => /^\/t\/[^/]+\/(?:apps|app-editor)\/([^/]+)/.exec(s.location.pathname)?.[1] ?? null,
  })
  const { apps } = useApps(team?.id)
  // #FEED-239 (chats.json `sidebarAgentCases`): an app's agent is listed
  // under Apps only.
  const sidebarAgentRows = useMemo(
    () => sidebarAgents(agents, apps.map((app) => ({ agentId: app.row?.agentId ?? null }))),
    [agents, apps]
  )
  const occupant = useSidebarOccupant()
  const inSettings = occupant.kind === `settings`

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
          {/* EXP-870: the header is FIXED above both occupants — the team
              picker, Search and New issue stay put in every state (desktop
              parity), instead of sliding out with the main menu. */}
          <SidebarHeader className="p-2">
            {/* EXP-449: Linear-style header row — team picker plus icon-only
                Inbox (#FEED-191: a popover), Search and New-issue actions. */}
            <div className="flex items-center gap-1">
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <SidebarMenuButton
                    className="min-w-0 flex-1 h-10"
                    aria-label="Team switcher"
                  >
                    <TeamAvatar iconUrl={team?.iconUrl} size={28} />
                    <span className="text-sm font-semibold truncate">
                      {team?.name ?? teamSlug}
                    </span>
                    {/* `overflow-visible!`: the menu button truncates its LAST
                        span, which would clip the corner badge. */}
                    <span className="relative ml-auto flex shrink-0 items-center overflow-visible!">
                      <NavTeamSwitcherIcon className="h-4 w-4" />
                      <Badge
                        count={otherNeedsYou}
                        tone="attention"
                        title="Needs you in other teams"
                        data-testid="team-switcher-needs-you-badge"
                        className="absolute -right-1.5 -top-2 ring-2 ring-sidebar"
                      />
                    </span>
                  </SidebarMenuButton>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start" className="w-56">
                  {myTeams.map((ws) => (
                    <DropdownMenuItem
                      key={ws.id}
                      onClick={() => {
                        const go = () =>
                          navigate({
                            to: `/t/$teamSlug`,
                            params: { teamSlug: ws.slug },
                          })
                        // EXP-1275: switching team blocks the window until
                        // the other team's page is up.
                        if (ws.slug === teamSlug) void go()
                        else void whileBlocking(blockingLabel(`switchTeam`, ws.name), go)
                      }}
                    >
                      <TeamAvatar iconUrl={ws.iconUrl} size={20} />
                      <span className="truncate">{ws.name}</span>
                      {ws.slug === teamSlug ? (
                        <UiCheckIcon className="ml-auto h-4 w-4" />
                      ) : (
                        <Badge
                          count={teamNeedsYou.byTeam.get(ws.id) ?? 0}
                          tone="attention"
                          title={NEEDS_YOU_COPY.entry}
                          className="ml-auto"
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
              {team && <InboxPopover teamId={team.id} />}
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

          {/* The occupants share one slot and both stay mounted: going into
              Settings the main menu slides out left as the settings nav
              slides in from the right (desktop `render_left_column`), and
              back. `inert` takes the hidden one out of the tab order and the
              accessibility tree. */}
          <div className="relative min-h-0 flex-1 overflow-hidden">
              <PanelLayer
                panel="main"
                inert={inSettings}
                className={cn(
                  `transition-transform`,
                  inSettings ? `-translate-x-full` : `translate-x-0`
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
                        {/* Nav order (needs-you.json): Agent, Needs you (while
                            any), Chats, Devices, Drafts (while any). */}
                        <SidebarMenu>
                          {/* EXP-818 / EXP-1293: the Agent page — HOME, the
                              composer. No live dot: Chats carries it. */}
                          <SidebarMenuItem>
                            <SidebarMenuButton asChild density="compact">
                              <Link to="/t/$teamSlug/agent" params={{ teamSlug }}>
                                <NavAgentIcon className="h-4 w-4" />
                                <span>Agent</span>
                              </Link>
                            </SidebarMenuButton>
                          </SidebarMenuItem>
                          {needsYou.rows.length > 0 && (
                            <SidebarMenuItem>
                              <SidebarMenuButton asChild density="compact">
                                <Link
                                  to="/t/$teamSlug/needs-you"
                                  params={{ teamSlug }}
                                  data-testid={NEEDS_YOU_TEST_IDS.entry}
                                >
                                  <NavNeedsYouIcon className="h-4 w-4" />
                                  <span>{NEEDS_YOU_COPY.entry}</span>
                                </Link>
                              </SidebarMenuButton>
                              <SidebarMenuTrailing className="pointer-events-none">
                                <Badge count={needsYou.rows.length} tone="attention" data-testid="needs-you-count-badge" />
                              </SidebarMenuTrailing>
                            </SidebarMenuItem>
                          )}
                          {/* EXP-1293: Chats replaced Reviews and the
                              Agent page's history; #FEED-232: its count
                              badge = `chatsBadge`. */}
                          <SidebarMenuItem>
                            <SidebarMenuButton asChild density="compact">
                              <Link
                                to="/t/$teamSlug/chats"
                                params={{ teamSlug }}
                                data-testid="nav-chats"
                              >
                                <NavChatsIcon className="h-4 w-4" />
                                <span>{CHATS_COPY.title}</span>
                              </Link>
                            </SidebarMenuButton>
                            <NavCountBadge value={chatsCount} data-testid="nav-chats-badge" />
                          </SidebarMenuItem>
                          <SidebarMenuItem>
                            <SidebarMenuButton asChild density="compact">
                              <Link to="/t/$teamSlug/devices" params={{ teamSlug }}>
                                <NavDevicesIcon className="h-4 w-4" />
                                <span>Devices</span>
                              </Link>
                            </SidebarMenuButton>
                          </SidebarMenuItem>
                          {/* The Drafts pile (EXP-878, while any) is a
                              root entry, as on desktop. NEW-2: no Actions
                              entry (prompts live in the composer's `/`). */}
                          {draftCount > 0 && (
                            <SidebarMenuItem>
                              <SidebarMenuButton asChild density="compact">
                                <Link
                                  to="/t/$teamSlug/drafts"
                                  params={{ teamSlug }}
                                  data-testid="nav-drafts"
                                >
                                  <NavDraftsIcon className="h-4 w-4" />
                                  <span>{DRAFTS_LABEL}</span>
                                </Link>
                              </SidebarMenuButton>
                              <SidebarMenuTrailing className="pointer-events-none">
                                <Badge count={draftCount} tone="muted" data-testid="drafts-count-badge" />
                              </SidebarMenuTrailing>
                            </SidebarMenuItem>
                          )}
                        </SidebarMenu>
                      </SidebarGroupContent>
                    </SidebarGroup>

                    {/* EXP-778: the caller's pinned issues / sessions / actions
                        in this team — favourites above the boards, hidden when
                        empty. */}
                    {team && <SidebarPinned teamId={team.id} teamSlug={teamSlug} />}

                    {/* EXP-1293: the team's agents, above Boards and drawn
                        like them. */}
                    <SidebarAgentsSection
                      teamId={team?.id}
                      teamSlug={teamSlug}
                      agents={sidebarAgentRows}
                      isOwner={isOwner}
                      activeAgentId={activeAgentId}
                      badges={agentBadges}
                    />

                    {/* VAPP-110: the team's apps, drawn like the agents
                        (fixtures/apps.json). */}
                    {team && (
                      <SidebarAppsSection
                        teamId={team.id}
                        teamSlug={teamSlug}
                        apps={apps}
                        activeAppSlug={activeAppSlug}
                      />
                    )}

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
                        {/* #EXP-1832: the member's folders + drag order
                            over the boards' sortOrder (sidebar-folders.json). */}
                        <SidebarFolderList
                          teamId={team?.id}
                          section="board"
                          ids={boardIds}
                          empty={
                            <SidebarMenuItem>
                              <Empty id="board-switcher" />
                            </SidebarMenuItem>
                          }
                          renderItem={(id, itemProps, guides) => {
                            const board = boardsById.get(id)
                            if (!board) return null
                            const TypeIcon = getBoardIcon(board)
                            return (
                              <SidebarMenuItem key={board.id} {...itemProps}>
                                {guides}
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
                          }}
                        />
                      </SidebarGroupContent>
                    </SidebarGroup>
                    {/* EXP-923 / EXP-1293 / EXP-1318: the ACTIVE group — the
                        team's chats that need you, run or wait on a
                        merge, under the boards. Hidden when empty. */}
                    <SidebarRunningSection teamSlug={teamSlug} chats={chats} />
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
                    {/* EXP-88/EXP-1270: Getting started's re-entry point,
                        the same block in a centred dialog. */}
                    <GettingStartedButton team={team} teamSlug={teamSlug} />
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
                          {userLabel ? (
                            <span className="truncate text-sm">
                              {firstName(userLabel)}
                            </span>
                          ) : (
                            // D8: a short placeholder bar (the skeleton's
                            // 8px muted bar) until the name lands.
                            <span
                              aria-hidden
                              data-testid="sidebar-user-placeholder"
                              className="h-2 w-14 shrink-0 rounded bg-muted"
                            />
                          )}
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

              <PanelLayer
                panel="settings"
                inert={!inSettings}
                className={cn(
                  `transition-transform`,
                  inSettings ? `translate-x-0` : `translate-x-full`
                )}
              >
                <SettingsSidebar
                  teamSlug={teamSlug}
                  permissions={permissions}
                  onBack={handleSettingsBack}
                />
              </PanelLayer>
          </div>
        </div>
        {/* EXP-1156: the column's ONE drag edge — EXP-1163: the content
            card's left edge. It resizes the settings nav; the main menu has
            none. */}
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
  // The main menu stays as it is: only the settings nav drags.
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
  const column = sidebarWidthCss(widths[occupant])
  useLayoutEffect(() => {
    if (!wrapper) return
    wrapper.style.setProperty(`--sidebar-width`, column)
    if (resizing) wrapper.dataset.resizing = `true`
    else delete wrapper.dataset.resizing
  }, [wrapper, column, resizing])
  return null
}
