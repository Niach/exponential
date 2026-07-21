import { useState } from "react"
import { Link, useNavigate } from "@tanstack/react-router"
import {
  Bell,
  Bot,
  Check,
  ChevronsUpDown,
  FolderKanban,
  GitPullRequest,
  Inbox,
  LifeBuoy,
  LogIn,
  LogOut,
  Megaphone,
  Plus,
  Search,
  Settings,
  Shield,
} from "lucide-react"
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
import type { Board, Team } from "@/db/schema"
import {
  useReviewsOpenPrCount,
  useAgentsRunningCount,
} from "@/hooks/use-nav-counts"
import {
  useShowTeamChrome,
  useTeamMemberships,
} from "@/hooks/use-team-data"
import { CreateBoardDialog } from "@/components/create-board-dialog"
import { CreateTeamDialog } from "@/components/create-team-dialog"
import { GettingStartedButton } from "@/components/getting-started/getting-started-button"
import { FeedbackButton } from "@/components/feedback-button"
import { ChangelogSheet, WhatsNewCard } from "@/components/whats-new"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
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

// Rendered only when authed, so the per-user notifications shape (requireAuth)
// isn't subscribed for anonymous public-team viewers.
function InboxUnreadBadge() {
  const unread = useUnreadNotificationCount()
  if (unread === 0) return null
  return <SidebarMenuBadge>{unread > 99 ? `99+` : unread}</SidebarMenuBadge>
}

// Unread helpdesk activity in THIS team, for the Support entry. Same
// authed-only caveat as InboxUnreadBadge.
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
  const isAuthed = Boolean(session?.user)
  // Solo users never see the "team" concept: no switcher, no name. The
  // chrome is revealed once they collaborate (2+ humans) or own 2+ teams.
  // Anonymous public viewers always get the chrome (it carries their Sign in).
  const chromeFromData = useShowTeamChrome(
    team?.id,
    session?.user?.id
  )
  const showChrome = !isAuthed || chromeFromData

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
                  <ChevronsUpDown className="ml-auto h-4 w-4 shrink-0" />
                </SidebarMenuButton>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start" className="w-56">
                {isAuthed &&
                  myTeams.map((ws) => (
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
                        <Check className="ml-auto h-4 w-4" />
                      )}
                    </DropdownMenuItem>
                  ))}
                {isAuthed && <DropdownMenuSeparator />}
                {/* Any signed-in user can create teams (EXP-188) — the
                    server's only gate is the free-tier owned-team cap. */}
                {isAuthed && (
                  <DropdownMenuItem onClick={() => setCreateTeamOpen(true)}>
                    <Plus className="h-4 w-4" />
                    New team
                  </DropdownMenuItem>
                )}
                {isAuthed && (
                  <DropdownMenuItem
                    onClick={() =>
                      navigate({
                        to: `/t/$teamSlug/settings`,
                        params: { teamSlug },
                      })
                    }
                  >
                    <Settings className="h-4 w-4" />
                    Team settings
                  </DropdownMenuItem>
                )}
                {!isAuthed && (
                  <DropdownMenuItem
                    onClick={() =>
                      navigate({
                        to: `/auth/login`,
                        search: { redirect: undefined },
                      })
                    }
                  >
                    <LogIn className="h-4 w-4" />
                    Sign in
                  </DropdownMenuItem>
                )}
              </DropdownMenuContent>
            </DropdownMenu>
          ) : (
            <div className="flex h-10 items-center gap-2 px-2">
              <ExponentialLogo
                variant="light"
                size={28}
                className="shrink-0"
              />
              <span className="text-sm font-semibold truncate">
                Exponential
              </span>
            </div>
          )}
        </SidebarHeader>

        <Separator />

        <SidebarContent>
          {(isAuthed || team) && (
            <SidebarGroup>
              <SidebarGroupContent>
                <SidebarMenu>
                  {team && (
                    <SidebarMenuItem>
                      <SidebarMenuButton onClick={onOpenSearch}>
                        <Search className="h-4 w-4" />
                        <span>Search</span>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  )}
                  {isAuthed && (
                    <SidebarMenuItem>
                      <SidebarMenuButton asChild>
                        <Link to="/t/$teamSlug/inbox" params={{ teamSlug }}>
                          <Inbox className="h-4 w-4" />
                          <span>Inbox</span>
                        </Link>
                      </SidebarMenuButton>
                      <InboxUnreadBadge />
                    </SidebarMenuItem>
                  )}
                  {isAuthed && (
                    <SidebarMenuItem>
                      <SidebarMenuButton asChild>
                        <Link
                          to="/t/$teamSlug/reviews"
                          params={{ teamSlug }}
                        >
                          <GitPullRequest className="h-4 w-4" />
                          <span>Reviews</span>
                        </Link>
                      </SidebarMenuButton>
                      <ReviewsCountBadge boards={boards} />
                    </SidebarMenuItem>
                  )}
                  {isAuthed && (
                    <SidebarMenuItem>
                      <SidebarMenuButton asChild>
                        <Link
                          to="/t/$teamSlug/agents"
                          params={{ teamSlug }}
                        >
                          <Bot className="h-4 w-4" />
                          <span>Agents</span>
                        </Link>
                      </SidebarMenuButton>
                      <AgentsRunningBadge teamId={team?.id} />
                    </SidebarMenuItem>
                  )}
                  {isAuthed && team?.helpdeskEnabled === true && (
                      <SidebarMenuItem>
                        <SidebarMenuButton asChild>
                          <Link
                            to="/t/$teamSlug/support"
                            params={{ teamSlug }}
                          >
                            <LifeBuoy className="h-4 w-4" />
                            <span>Support</span>
                          </Link>
                        </SidebarMenuButton>
                        <SupportUnreadBadge teamId={team?.id} />
                      </SidebarMenuItem>
                    )}
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          )}

          <SidebarGroup>
            <SidebarGroupLabel>Boards</SidebarGroupLabel>
            {isAuthed && (
              <SidebarGroupAction
                onClick={() => setCreateBoardOpen(true)}
                title="Create board"
                aria-label="Create board"
              >
                <Plus className="h-4 w-4" />
              </SidebarGroupAction>
            )}
            <SidebarGroupContent>
              <SidebarMenu>
                {!boards || boards.length === 0 ? (
                  <SidebarMenuItem>
                    <SidebarMenuButton disabled>
                      <FolderKanban className="h-4 w-4" />
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
          {isAuthed && <WhatsNewCard onOpen={() => setWhatsNewOpen(true)} />}
          <SidebarMenu>
            {/* EXP-88: re-entry point for the Getting started cards once the
                board's inline block is gone (issues exist / dismissed). */}
            {isAuthed && (
              <GettingStartedButton
                teamSlug={teamSlug}
                team={team}
              />
            )}
            <FeedbackButton />
          </SidebarMenu>
          {isAuthed ? (
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
                  <span className="truncate text-sm">
                    {session?.user?.email ?? `Loading...`}
                  </span>
                  <ChevronsUpDown className="ml-auto h-4 w-4" />
                </SidebarMenuButton>
              </DropdownMenuTrigger>
              <DropdownMenuContent side="top" align="start" className="w-56">
                {isAdminUser(session?.user) && (
                  <DropdownMenuItem
                    onClick={() => navigate({ to: `/admin` })}
                  >
                    <Shield className="mr-2 h-4 w-4" />
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
                    <Settings className="mr-2 h-4 w-4" />
                    Settings
                  </DropdownMenuItem>
                )}
                <DropdownMenuItem
                  onClick={() => navigate({ to: `/account/notifications` })}
                >
                  <Bell className="mr-2 h-4 w-4" />
                  Account & notifications
                </DropdownMenuItem>
                {/* Re-entry point once the footer card is dismissed. */}
                <DropdownMenuItem onClick={() => setWhatsNewOpen(true)}>
                  <Megaphone className="mr-2 h-4 w-4" />
                  What&apos;s new
                </DropdownMenuItem>
                {!showChrome && (
                  <DropdownMenuItem onClick={() => setCreateTeamOpen(true)}>
                    <Plus className="mr-2 h-4 w-4" />
                    New team
                  </DropdownMenuItem>
                )}
                <DropdownMenuSeparator />
                <DropdownMenuItem onClick={handleSignOut}>
                  <LogOut className="mr-2 h-4 w-4" />
                  Sign out
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          ) : (
            <Button
              variant="default"
              className="w-full"
              onClick={() =>
                navigate({
                  to: `/auth/login`,
                  search: { redirect: undefined },
                })
              }
            >
              <LogIn className="mr-2 h-4 w-4" />
              Sign in to contribute
            </Button>
          )}
        </SidebarFooter>
      </Sidebar>

      {isAuthed && (
        <ChangelogSheet open={whatsNewOpen} onOpenChange={setWhatsNewOpen} />
      )}
      {team && isAuthed && (
        <CreateBoardDialog
          open={createBoardOpen}
          onOpenChange={setCreateBoardOpen}
          team={team}
        />
      )}
      {isAuthed && (
        <CreateTeamDialog
          open={createTeamOpen}
          onOpenChange={setCreateTeamOpen}
        />
      )}
    </>
  )
}
