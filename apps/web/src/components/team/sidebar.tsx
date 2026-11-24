import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react"
import type { ReactNode } from "react"
import { Link, useNavigate, useParams, useRouter, useRouterState } from "@tanstack/react-router"
import { chatsBadge } from "@exp/domain-contract/nav-badges"
import { sidebarTeams } from "@exp/domain-contract/sidebar-tree"
import {
  conceptIcon,
  Button,
  ExponentialLogo,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
  ResizeHandle,
  Separator,
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  useSidebar,
} from "@exp/ui"
import { InboxPopover } from "@/components/team/inbox-popover"
import { useMemberNeedsYouCounts } from "@/hooks/use-app-icon-badge"
import { useSessionReads, useChatsList } from "@/hooks/use-session-tree"
import { chatWordOf } from "@/components/chat/chats-list"
import { useSession } from "@/hooks/use-session"
import { cn } from "@/lib/utils"
import { firstName } from "@/lib/user-display"
import type { Board, Team } from "@/db/schema"
import type { ChatsListRow } from "@/lib/session-tree"
import { useTeamMemberships } from "@/hooks/use-team-data"
import { useSidebarOccupant } from "@/hooks/use-sidebar-occupant"
import { CreateTeamDialog } from "@/components/create-team-dialog"
import { JoinTeamDialog } from "@/components/join-team-dialog"
import { AddDeviceDialog } from "@/components/add-device-dialog"
import { SettingsSidebar } from "@/components/team/settings-sidebar"
import { SidebarRunningSection, type SidebarChats, type SidebarRunningTeam } from "@/components/team/sidebar-running"
import { SidebarRootRows, SidebarTeamsGroup, sidebarActive } from "@/components/team/sidebar-tree"
import { useSidebarAssistant } from "@/hooks/use-sidebar-assistant"
import { mainPanelEdge } from "@/components/team/app-shell"
import { UserAvatar, UserMenuItems } from "@/components/team/sidebar-shared"
import { useOpenNewDraft } from "@/hooks/use-open-new-draft"
import {
  useCrossTeamDraftCount,
  usePullRequestsByRootForTeams,
  useSidebarDevices,
} from "@/hooks/use-sidebar-tree"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { homePath } from "@/lib/app-paths"
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
const NavCreateIssueIcon = conceptIcon(`nav-create-issue`)
const NavSearchIcon = conceptIcon(`nav-search`)
const NavSettingsIcon = conceptIcon(`nav-settings`)
const NavTeamSwitcherIcon = conceptIcon(`nav-team-switcher`)

// EXP-862: every arm of the panel slot runs at the sidebar's COMPACT density
// — `SidebarMenuButton density="compact"` (EXP-962), whose rationale lives
// with the prop in `packages/ui/src/sidebar.tsx`.

// EXP-870: one motion for every width and slide in the column — the shared
// motion tokens (styles.css, packages/design-tokens).
// EXP-1156: a drag (`data-resizing` on the sidebar wrapper) switches it off,
// or every slot would trail the pointer by the whole duration.
const SLOT_MOTION = `duration-standard ease-standard motion-reduce:transition-none group-data-[resizing=true]/sidebar-wrapper:transition-none`

interface TeamSidebarProps {
  /** The route's team on a team page, the person's DEFAULT team on a
   *  team-less one (#EXP-2027): only the header's New issue reads it; the
   *  tree has no active team. */
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
  const me = session?.user?.id
  const navigate = useNavigate()
  const [createTeamOpen, setCreateTeamOpen] = useState(false)
  const [joinTeamOpen, setJoinTeamOpen] = useState(false)
  const [addDeviceOpen, setAddDeviceOpen] = useState(false)
  const { canCreate } = useTeamPermissions(team)
  // EXP-449: the header's New-issue button files onto the route's board
  // (else the device's last-used / first board of the team, same resolution
  // as the mobile FAB); no board = no button. EXP-1170: it opens the New
  // issue page on a fresh draft.
  const { boardSlug } = useParams({ strict: false })
  const boardTarget = resolveBoardTarget(teamSlug, boards, boardSlug)
  const openNewDraft = useOpenNewDraft(teamSlug)
  const [whatsNewOpen, setWhatsNewOpen] = useState(false)
  // #EXP-2124: the account row's right-click opens its click menu.
  const [userMenuOpen, setUserMenuOpen] = useState(false)
  // EXP-1022: the card's visibility lives here because the scroll content
  // reserves room under its last row only while the card floats over it.
  const whatsNew = useWhatsNew()

  // #EXP-2027 (sidebar-tree.json): no active team — every count and list of
  // the root rows and the Active band spans the viewer's member teams.
  const { myTeams, memberships } = useTeamMemberships(me)
  const teamOrder = useMemo(
    () => sidebarTeams(myTeams.map((t) => ({ id: t.id, name: t.name, kind: (t as { kind?: string }).kind ?? `team` }))),
    [myTeams]
  )
  const teamsById = useMemo(() => new Map(myTeams.map((t) => [t.id, t as Team])), [myTeams])
  const orderedTeams = useMemo(
    () => teamOrder.flatMap((id) => (teamsById.has(id) ? [teamsById.get(id) as Team] : [])),
    [teamOrder, teamsById]
  )
  const owners = useMemo(
    () => new Set(memberships.filter((m) => m.role === `owner`).map((m) => m.teamId)),
    [memberships]
  )
  const myTeamIds = useMemo(() => myTeams.map((t) => t.id), [myTeams])
  const needsYou = useMemberNeedsYouCounts()
  const draftCount = useCrossTeamDraftCount(myTeamIds)
  const devices = useSidebarDevices(me, teamOrder)
  const assistant = useSidebarAssistant(team, teamSlug, me)
  // EXP-1293: ONE Chats list read for the sidebar — the Chats row's badge,
  // the placed sessions' marks and the Active band.
  const chatsList = useChatsList(myTeamIds)
  const prs = usePullRequestsByRootForTeams(myTeamIds)
  const chats: SidebarChats = {
    groups: chatsList.groups,
    wordOf: (row) => chatWordOf(row, prs.byRoot.get(row.sessionId) ?? [], prs.all),
  }
  const chatRows = useMemo(() => {
    const map = new Map<string, ChatsListRow>()
    for (const group of chatsList.groups) for (const row of group.rows) map.set(row.sessionId, row)
    return map
  }, [chatsList.groups])
  const sessionReads = useSessionReads()
  const chatsCount = chatsBadge(chatsList.groups, sessionReads)
  const runningTeams = useMemo(
    () =>
      new Map<string, SidebarRunningTeam>(
        orderedTeams.map((t) => [t.id, { slug: t.slug, iconUrl: t.iconUrl, name: t.name }])
      ),
    [orderedTeams]
  )

  // Name-less accounts (Apple sign-in stores an empty name) fall back to the
  // email instead of a bare "?".
  const userLabel = session?.user?.name || session?.user?.email

  // EXP-456 / EXP-851: the column's occupant is a function of the URL
  // (`useSidebarOccupant`) — the MAIN menu, or the settings nav in its place
  // (desktop `left_occupant_for`). Nothing folds to icons. Every entry point
  // drives the same swap, and a direct load lands settled.
  const router = useRouter()
  const locationHref = useRouterState({ select: (s) => s.location.href })
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const active = useMemo(() => sidebarActive(pathname), [pathname])
  const occupant = useSidebarOccupant()
  const inSettings = occupant.kind === `settings`

  // Last non-settings location — the back button returns exactly here (with
  // its search params). Read in the same router-state snapshot as the
  // occupant, or a pending navigation would record a settings URL as the
  // return target.
  const lastNonSettingsHref = useRef<string | null>(null)
  useEffect(() => {
    if (!inSettings) lastNonSettingsHref.current = locationHref
  }, [inSettings, locationHref])

  // #EXP-2027: any page (team-less or a team's) is a return target; a direct
  // load into settings returns Home.
  const handleSettingsBack = () => {
    const href = lastNonSettingsHref.current
    if (href) router.history.push(href)
    else void navigate({ href: homePath() })
  }

  const origin = useMemo(
    () => (typeof window === `undefined` ? `https://app.exponential.at` : window.location.origin),
    []
  )

  return (
    <>
      {/* No right hairline: since EXP-723 the content column is a 10px-inset
          rounded card whose own stroke is the divider (sidebar-on-ground). */}
      <Sidebar className="border-r-0!">
        <div className="flex h-full w-full flex-col overflow-hidden">
          {/* EXP-870: the header is FIXED above both occupants — the brand,
              Inbox, Search and New issue stay put in every state (desktop
              parity), instead of sliding out with the main menu. */}
          <SidebarHeader className="p-2">
            {/* #EXP-2027: the Exponential brand (no team switcher), then the
                icon-only Inbox (#FEED-191: a popover), Search and the filled
                New issue. */}
            <div className="flex items-center gap-1">
              <div className="flex h-10 min-w-0 flex-1 items-center gap-2 px-2" data-testid="sidebar-brand">
                <ExponentialLogo variant="light" size={24} className="shrink-0 text-foreground" />
                <span className="truncate text-sm font-semibold">Exponential</span>
              </div>
              <InboxPopover />
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
                    {/* #EXP-2027 (sidebar-tree.json `root`): Home, Needs you
                        (while any), Chats, Drafts (while any), Devices. */}
                    <SidebarGroup>
                      <SidebarGroupContent>
                        <SidebarRootRows
                          active={active}
                          assistant={assistant}
                          needsYou={needsYou.total}
                          drafts={draftCount}
                          chatsBadge={chatsCount}
                          devices={devices}
                          teams={teamsById}
                          onAddDevice={() => setAddDeviceOpen(true)}
                        />
                      </SidebarGroupContent>
                    </SidebarGroup>

                    {/* The Teams label, then one row per team holding its
                        one mixed list. */}
                    <SidebarTeamsGroup
                      teams={orderedTeams}
                      owners={owners}
                      needsYou={needsYou.byTeam}
                      chatRows={chatRows}
                      me={me}
                      active={active}
                      onNewTeam={() => setCreateTeamOpen(true)}
                      onJoinTeam={() => setJoinTeamOpen(true)}
                    />

                    {/* EXP-923 / EXP-1293 / EXP-1318: the ACTIVE group — every
                        team's chats that need you, run or wait on a merge, at
                        the bottom. Hidden when empty. */}
                    <SidebarRunningSection teams={runningTeams} chats={chats} />
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
                      block, like the IDE's rail gear; #EXP-2027: the
                      personal settings. */}
                  <div className="flex items-center gap-1">
                    <DropdownMenu open={userMenuOpen} onOpenChange={setUserMenuOpen}>
                      <DropdownMenuTrigger asChild>
                        <SidebarMenuButton
                          className="min-w-0 flex-1"
                          aria-label="User menu"
                          onContextMenu={(event) => {
                            event.preventDefault()
                            setUserMenuOpen(true)
                          }}
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
                      <Link to="/settings/account" data-testid="sidebar-settings">
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
                <SettingsSidebar onBack={handleSettingsBack} />
              </PanelLayer>
          </div>
        </div>
        {/* EXP-1156: the column's ONE drag edge — EXP-1163: the content
            card's left edge. It resizes the settings nav; the main menu has
            none. */}
        <SidebarResizeEdge panel={occupant.kind} workTabs={workTabs} />
      </Sidebar>

      <ChangelogSheet open={whatsNewOpen} onOpenChange={setWhatsNewOpen} />
      <CreateTeamDialog
        open={createTeamOpen}
        onOpenChange={setCreateTeamOpen}
      />
      <JoinTeamDialog open={joinTeamOpen} onOpenChange={setJoinTeamOpen} />
      <AddDeviceDialog open={addDeviceOpen} onOpenChange={setAddDeviceOpen} origin={origin} />
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
