import { useState } from "react"
import { Link, useNavigate } from "@tanstack/react-router"
import { conceptIcon } from "@/lib/icons.generated"
import { ExponentialLogo } from "@/components/exponential-logo"
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
import { useShowTeamChrome, useTeamMemberships } from "@/hooks/use-team-data"
import { CreateBoardDialog } from "@/components/create-board-dialog"
import { CreateTeamDialog } from "@/components/create-team-dialog"
import { GettingStartedButton } from "@/components/getting-started/getting-started-button"
import { FeedbackButton } from "@/components/feedback-button"
import { ChangelogSheet, WhatsNewCard } from "@/components/whats-new"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
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
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"

// EXP-317: the cross-client nav glyphs come from the shared registry
// (packages/icons/icons.json) so web, desktop, iOS and Android agree.
const NavAdminIcon = conceptIcon(`nav-admin`)
const NavAgentsIcon = conceptIcon(`nav-agents`)
const NavBoardsIcon = conceptIcon(`nav-boards`)
const NavChangelogIcon = conceptIcon(`nav-changelog`)
const NavInboxIcon = conceptIcon(`nav-inbox`)
const NavNotificationsIcon = conceptIcon(`nav-notifications`)
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

// Live count of running coding sessions in the team, for the Agents entry.
// Amber while any session waits on a plan approval / question (EXP-214).
function AgentsRunningBadge({ teamId }: { teamId?: string }) {
  const { count, needsInput } = useAgentsRunningCount(teamId)
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
  const [whatsNewOpen, setWhatsNewOpen] = useState(false)
  const { myTeams } = useTeamMemberships(session?.user?.id)
  // Solo users never see the "team" concept: no switcher, no name. The
  // chrome is revealed once they collaborate (2+ humans) or own 2+ teams.
  // The guarded /t/$teamSlug layout is the only render site, so a session is
  // guaranteed — the reactive useSession store may still be pending on cold
  // load, and we render the authed chrome throughout rather than flash a
  // logged-out state.
  const showChrome = useShowTeamChrome(team?.id, session?.user?.id)

  const handleSignOut = useSignOut()

  // Name-less accounts (Apple sign-in stores an empty name) fall back to the
  // email for initials instead of a bare "?".
  const userLabel = session?.user?.name || session?.user?.email
  const userInitials = userLabel ? getInitials(userLabel) : `?`

  return (
    <>
      <Sidebar>
        <SidebarHeader className="p-2">
          {showChrome ? (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <SidebarMenuButton
                  className="w-full h-10"
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
                <DropdownMenuItem
                  onClick={() =>
                    navigate({
                      to: `/t/$teamSlug/settings`,
                      params: { teamSlug },
                    })
                  }
                >
                  <NavSettingsIcon className="h-4 w-4" />
                  Team settings
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          ) : (
            <div className="flex h-10 items-center gap-2 px-2">
              <ExponentialLogo variant="light" size={28} className="shrink-0" />
              <span className="text-sm font-semibold truncate">
                Exponential
              </span>
            </div>
          )}
        </SidebarHeader>

        <Separator />

        <SidebarContent>
          <SidebarGroup>
            <SidebarGroupContent>
              <SidebarMenu>
                {team && (
                  <SidebarMenuItem>
                    <SidebarMenuButton onClick={onOpenSearch}>
                      <NavSearchIcon className="h-4 w-4" />
                      <span>Search</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                )}
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
                <SidebarMenuItem>
                  <SidebarMenuButton asChild>
                    <Link to="/t/$teamSlug/agents" params={{ teamSlug }}>
                      <NavAgentsIcon className="h-4 w-4" />
                      <span>Agents</span>
                    </Link>
                  </SidebarMenuButton>
                  <AgentsRunningBadge teamId={team?.id} />
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
            <GettingStartedButton teamSlug={teamSlug} team={team} />
            <FeedbackButton />
          </SidebarMenu>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <SidebarMenuButton className="w-full" aria-label="User menu">
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
              {/* In solo mode the switcher is hidden, so Settings + New
                    team live here instead (framed as account-level). */}
              {!showChrome && (
                <DropdownMenuItem
                  onClick={() =>
                    navigate({
                      to: `/t/$teamSlug/settings`,
                      params: { teamSlug },
                    })
                  }
                >
                  <NavSettingsIcon className="mr-2 h-4 w-4" />
                  Settings
                </DropdownMenuItem>
              )}
              <DropdownMenuItem
                onClick={() => navigate({ to: `/account/notifications` })}
              >
                <NavNotificationsIcon className="mr-2 h-4 w-4" />
                Account & notifications
              </DropdownMenuItem>
              {/* Re-entry point once the footer card is dismissed. */}
              <DropdownMenuItem onClick={() => setWhatsNewOpen(true)}>
                <NavChangelogIcon className="mr-2 h-4 w-4" />
                What&apos;s new
              </DropdownMenuItem>
              {!showChrome && (
                <DropdownMenuItem onClick={() => setCreateTeamOpen(true)}>
                  <UiAddIcon className="mr-2 h-4 w-4" />
                  New team
                </DropdownMenuItem>
              )}
              <DropdownMenuSeparator />
              <DropdownMenuItem onClick={handleSignOut}>
                <NavSignOutIcon className="mr-2 h-4 w-4" />
                Sign out
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </SidebarFooter>
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
    </>
  )
}
