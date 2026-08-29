import { useEffect, useRef, useState } from "react"
import {
  Link,
  useNavigate,
  useParams,
  useRouter,
  useRouterState,
} from "@tanstack/react-router"
import { conceptIcon } from "@/lib/icons.generated"
import { getBoardIcon } from "@/lib/board-icons"
import { useSession } from "@/hooks/use-session"
import {
  useUnreadNotificationCount,
  useUnreadSupportCount,
} from "@/hooks/use-unread-notifications"
import { isAdminUser } from "@/lib/auth/app-user"
import { useSignOut } from "@/hooks/use-sign-out"
import { getInitials } from "@/lib/utils"
import { firstName } from "@/lib/user-display"
import type { Board, Team } from "@/db/schema"
import {
  useReviewsOpenPrCount,
  useAgentsRunningCount,
} from "@/hooks/use-nav-counts"
import { useTeamMemberships } from "@/hooks/use-team-data"
import { CreateBoardDialog } from "@/components/create-board-dialog"
import { CreateTeamDialog } from "@/components/create-team-dialog"
import { BoardSettingsDialog } from "@/components/team/board-settings-dialog"
import { SettingsSidebar } from "@/components/team/settings-sidebar"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { GettingStartedButton } from "@/components/getting-started/getting-started-button"
import { ChangelogSheet, WhatsNewCard } from "@/components/whats-new"
import { resolveBoardTarget } from "@/components/team/mobile-tab-bar"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Separator } from "@/components/ui/separator"
import {
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
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"

// EXP-317: the cross-client nav glyphs come from the shared registry
// (packages/icons/icons.json) so web, desktop, iOS and Android agree.
const NavAboutIcon = conceptIcon(`settings-about`)
const NavAdminIcon = conceptIcon(`nav-admin`)
const NavActionsIcon = conceptIcon(`nav-actions`)
const NavAutomationsIcon = conceptIcon(`nav-automations`)
const NavBoardsIcon = conceptIcon(`nav-boards`)
const NavChangelogIcon = conceptIcon(`nav-changelog`)
const NavCreateIssueIcon = conceptIcon(`nav-create-issue`)
const NavDevicesIcon = conceptIcon(`nav-devices`)
const NavInboxIcon = conceptIcon(`nav-inbox`)
const NavReviewsIcon = conceptIcon(`nav-reviews`)
const NavSearchIcon = conceptIcon(`nav-search`)
const NavSettingsIcon = conceptIcon(`nav-settings`)
const NavSignOutIcon = conceptIcon(`nav-sign-out`)
const NavSupportIcon = conceptIcon(`nav-support`)
const NavTeamSwitcherIcon = conceptIcon(`nav-team-switcher`)
const UiAddIcon = conceptIcon(`ui-add`)
const UiCheckIcon = conceptIcon(`ui-check`)

// Live unread count from the per-user notifications shape.
function InboxUnreadBadge() {
  const unread = useUnreadNotificationCount()
  if (unread === 0) return null
  return <SidebarMenuBadge>{unread > 99 ? `99+` : unread}</SidebarMenuBadge>
}

// Unread helpdesk activity in THIS team, for the Support entry.
function SupportUnreadBadge({ teamId }: { teamId?: string }) {
  const unread = useUnreadSupportCount(teamId)
  if (unread === 0) return null
  return <SidebarMenuBadge>{unread > 99 ? `99+` : unread}</SidebarMenuBadge>
}

// Open-PR count across the team's boards (DISTINCT PRs — EXP-131).
function ReviewsCountBadge({ boards }: { boards: Board[] | undefined }) {
  const count = useReviewsOpenPrCount(boards)
  if (count === 0) return null
  return <SidebarMenuBadge>{count > 99 ? `99+` : count}</SidebarMenuBadge>
}

// Live count of running coding sessions in the team, for the Devices entry.
// Amber while any session waits on a plan approval / question (EXP-214).
function DevicesRunningBadge({ teamId }: { teamId?: string }) {
  const { data: session } = useSession()
  const { count, needsInput } = useAgentsRunningCount(teamId, session?.user?.id)
  if (count === 0) return null
  return (
    <SidebarMenuBadge className={needsInput ? `text-amber-400` : undefined}>
      {count > 99 ? `99+` : count}
    </SidebarMenuBadge>
  )
}

interface TeamSidebarProps {
  teamSlug: string
  team: Team | null | undefined
  boards: Board[] | undefined
  onOpenSearch: () => void
}

export function TeamSidebar({
  teamSlug,
  team,
  boards,
  onOpenSearch,
}: TeamSidebarProps) {
  const { data: session } = useSession()
  const navigate = useNavigate()
  const [createBoardOpen, setCreateBoardOpen] = useState(false)
  const [createTeamOpen, setCreateTeamOpen] = useState(false)
  // FEED-3: hover gear on a board row → the board-settings dialog (rename,
  // icon, color, repo). Owner-gated like the settings Boards pages; the
  // dialog receives the LIVE row so a concurrent trash closes it.
  const [editBoardId, setEditBoardId] = useState<string | null>(null)
  const editBoard = boards?.find((board) => board.id === editBoardId) ?? null
  const permissions = useTeamPermissions(team)
  const { isOwner, canCreate } = permissions
  // EXP-449: the header's New-issue button works from any team route — it
  // navigates to the active board (else the device's last-used / first board,
  // same resolution as the mobile FAB) with ?new=1, which opens the dialog.
  const { boardSlug } = useParams({ strict: false })
  const boardTarget = resolveBoardTarget(teamSlug, boards, boardSlug)
  const [whatsNewOpen, setWhatsNewOpen] = useState(false)
  const { myTeams } = useTeamMemberships(session?.user?.id)
  // The guarded /t/$teamSlug layout is the only render site, so a session is
  // guaranteed — the reactive useSession store may still be pending on cold
  // load, and we render the authed chrome throughout rather than flash a
  // logged-out state.

  const handleSignOut = useSignOut()

  // Name-less accounts (Apple sign-in stores an empty name) fall back to the
  // email for initials instead of a bare "?".
  const userLabel = session?.user?.name || session?.user?.email
  const userInitials = userLabel ? getInitials(userLabel) : `?`

  // EXP-456: while any /settings route is active the settings panel occupies
  // the 16rem slot — the main nav slides out left, the settings nav slides in.
  // Derived from the URL (not click state) so every settings entry point
  // (footer gear, mobile topbar menu, deep links) drives the same swap, and a
  // direct load lands settled with no first-paint animation. Pathname, not
  // useMatchRoute: the matches lag the (eagerly updated) location during a
  // pending navigation, and the back-target effect below must see the SAME
  // snapshot or it records a settings URL as the return target.
  const router = useRouter()
  const locationHref = useRouterState({ select: (s) => s.location.href })
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const settingsBase = `/t/${teamSlug}/settings`
  const inSettings =
    pathname === settingsBase || pathname.startsWith(`${settingsBase}/`)

  // Last non-settings location — the back button returns exactly here (with
  // its search params), not merely to the team index.
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
      <Sidebar>
        <div className="relative h-full w-full overflow-hidden">
          {/* Main nav panel — slides out left while settings is active. Stays
              mounted so the reverse animation plays; inert removes its tab
              stops and hides it from assistive tech meanwhile. */}
          <div
            inert={inSettings}
            className={`absolute inset-0 flex flex-col transition-transform duration-standard ease-standard motion-reduce:transition-none ${
              inSettings ? `-translate-x-full` : `translate-x-0`
            }`}
          >
            <SidebarHeader className="p-2">
              {/* EXP-449: Linear-style header row — team picker plus icon-only
                  Search and New-issue actions (moved out of the nav list / the
                  board page's filter bar). */}
              <div className="flex items-center gap-1">
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <SidebarMenuButton
                      className="min-w-0 flex-1 h-10"
                      aria-label="Team switcher"
                    >
                      <div className="flex h-7 w-7 items-center justify-center rounded-md bg-primary text-primary-foreground text-xs font-bold shrink-0">
                        {team?.name?.[0]?.toUpperCase() ??
                          teamSlug[0]?.toUpperCase() ??
                          `E`}
                      </div>
                      <span className="text-sm font-semibold truncate">
                        {team?.name ?? teamSlug}
                      </span>
                      <NavTeamSwitcherIcon className="ml-auto h-4 w-4 shrink-0" />
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
                        <div className="flex h-5 w-5 items-center justify-center rounded bg-primary text-primary-foreground text-[0.625rem] font-bold shrink-0">
                          {ws.name[0]?.toUpperCase()}
                        </div>
                        <span className="truncate">{ws.name}</span>
                        {ws.slug === teamSlug && (
                          <UiCheckIcon className="ml-auto h-4 w-4" />
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
                        asChild
                        variant="default"
                        size="icon"
                        className="size-8 shrink-0"
                        aria-label="New issue"
                      >
                        <Link
                          to="/t/$teamSlug/boards/$boardSlug"
                          params={{ teamSlug, boardSlug: boardTarget.slug }}
                          search={(prev) => ({ ...prev, new: 1 })}
                        >
                          <NavCreateIssueIcon className="size-4" />
                        </Link>
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>New issue</TooltipContent>
                  </Tooltip>
                )}
              </div>
            </SidebarHeader>
    
            <Separator />
    
            <SidebarContent>
              <SidebarGroup>
                <SidebarGroupContent>
                  <SidebarMenu>
                    <SidebarMenuItem>
                      <SidebarMenuButton asChild>
                        <Link to="/t/$teamSlug/inbox" params={{ teamSlug }}>
                          <NavInboxIcon className="h-4 w-4" />
                          <span>Inbox</span>
                        </Link>
                      </SidebarMenuButton>
                      <InboxUnreadBadge />
                    </SidebarMenuItem>
                    <SidebarMenuItem>
                      <SidebarMenuButton asChild>
                        <Link to="/t/$teamSlug/reviews" params={{ teamSlug }}>
                          <NavReviewsIcon className="h-4 w-4" />
                          <span>Reviews</span>
                        </Link>
                      </SidebarMenuButton>
                      <ReviewsCountBadge boards={boards} />
                    </SidebarMenuItem>
                    {/* EXP-686: Devices · Actions · Automations, the three
                        surfaces the old Agents entry bundled. */}
                    <SidebarMenuItem>
                      <SidebarMenuButton asChild>
                        <Link to="/t/$teamSlug/devices" params={{ teamSlug }}>
                          <NavDevicesIcon className="h-4 w-4" />
                          <span>Devices</span>
                        </Link>
                      </SidebarMenuButton>
                      <DevicesRunningBadge teamId={team?.id} />
                    </SidebarMenuItem>
                    <SidebarMenuItem>
                      <SidebarMenuButton asChild>
                        <Link to="/t/$teamSlug/actions" params={{ teamSlug }}>
                          <NavActionsIcon className="h-4 w-4" />
                          <span>Actions</span>
                        </Link>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                    <SidebarMenuItem>
                      <SidebarMenuButton asChild>
                        <Link
                          to="/t/$teamSlug/automations"
                          params={{ teamSlug }}
                        >
                          <NavAutomationsIcon className="h-4 w-4" />
                          <span>Automations</span>
                        </Link>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                    {team?.helpdeskEnabled === true && (
                      <SidebarMenuItem>
                        <SidebarMenuButton asChild>
                          <Link to="/t/$teamSlug/support" params={{ teamSlug }}>
                            <NavSupportIcon className="h-4 w-4" />
                            <span>Support</span>
                          </Link>
                        </SidebarMenuButton>
                        <SupportUnreadBadge teamId={team?.id} />
                      </SidebarMenuItem>
                    )}
                  </SidebarMenu>
                </SidebarGroupContent>
              </SidebarGroup>
    
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
                        <SidebarMenuButton disabled>
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
                            <SidebarMenuButton asChild>
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
                                showOnHover
                                title="Board settings"
                                aria-label={`Settings for ${board.name}`}
                                onClick={() => setEditBoardId(board.id)}
                              >
                                <NavSettingsIcon />
                              </SidebarMenuAction>
                            )}
                          </SidebarMenuItem>
                        )
                      })
                    )}
                  </SidebarMenu>
                </SidebarGroupContent>
              </SidebarGroup>
            </SidebarContent>
    
            <SidebarFooter>
              {/* EXP-164: dismissable "What's new" teaser for the latest changelog
                  entry — hidden again until the next release once dismissed. */}
              <WhatsNewCard onOpen={() => setWhatsNewOpen(true)} />
              <SidebarMenu>
                {/* EXP-88: re-entry point for the Getting started cards once the
                    board's inline block is gone (issues exist / dismissed). */}
                <GettingStartedButton />
              </SidebarMenu>
              {/* EXP-238: settings entry lives down here next to the user block,
                  like the IDE's rail gear — it opens the unified settings page
                  (team sections + the Personal group). */}
              <div className="flex items-center gap-1">
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <SidebarMenuButton
                      className="min-w-0 flex-1"
                      aria-label="User menu"
                    >
                      <Avatar className="h-6 w-6">
                        {session?.user?.image && (
                          <AvatarImage src={session.user.image} />
                        )}
                        <AvatarFallback className="text-xs">
                          {userInitials}
                        </AvatarFallback>
                      </Avatar>
                      {/* First name only (EXP-311) — the full name + email live in
                          account settings; nameless accounts fall back to email. */}
                      <span className="truncate text-sm">
                        {userLabel ? firstName(userLabel) : `Loading...`}
                      </span>
                      <NavTeamSwitcherIcon className="ml-auto h-4 w-4" />
                    </SidebarMenuButton>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent side="top" align="start" className="w-56">
                    {isAdminUser(session?.user) && (
                      <DropdownMenuItem onClick={() => navigate({ to: `/admin` })}>
                        <NavAdminIcon className="mr-2 h-4 w-4" />
                        Admin
                      </DropdownMenuItem>
                    )}
                    {/* Re-entry point once the footer card is dismissed. */}
                    <DropdownMenuItem onClick={() => setWhatsNewOpen(true)}>
                      <NavChangelogIcon className="mr-2 h-4 w-4" />
                      What&apos;s new
                    </DropdownMenuItem>
                    {/* EXP-262: version-less About page with the third-party
                        licence notices. */}
                    <DropdownMenuItem onClick={() => navigate({ to: `/about` })}>
                      <NavAboutIcon className="mr-2 h-4 w-4" />
                      About
                    </DropdownMenuItem>
                    <DropdownMenuSeparator />
                    <DropdownMenuItem onClick={handleSignOut}>
                      <NavSignOutIcon className="mr-2 h-4 w-4" />
                      Sign out
                    </DropdownMenuItem>
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
          </div>

          {/* Settings panel — slides in from the right edge of the slot. */}
          <div
            inert={!inSettings}
            className={`absolute inset-0 flex flex-col transition-transform duration-standard ease-standard motion-reduce:transition-none ${
              inSettings ? `translate-x-0` : `translate-x-full`
            }`}
          >
            <SettingsSidebar
              teamSlug={teamSlug}
              permissions={permissions}
              onBack={handleSettingsBack}
            />
          </div>
        </div>
      </Sidebar>

      <ChangelogSheet open={whatsNewOpen} onOpenChange={setWhatsNewOpen} />
      {team && (
        <CreateBoardDialog
          open={createBoardOpen}
          onOpenChange={setCreateBoardOpen}
          team={team}
        />
      )}
      {team && (
        <BoardSettingsDialog
          board={editBoard}
          team={team}
          onOpenChange={(open) => {
            if (!open) setEditBoardId(null)
          }}
          onRepoChanged={() => {}}
        />
      )}
      <CreateTeamDialog
        open={createTeamOpen}
        onOpenChange={setCreateTeamOpen}
      />
    </>
  )
}
