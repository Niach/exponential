import { useMemo, useState } from "react"
import { Link, useNavigate } from "@tanstack/react-router"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import {
  Bell,
  Bot,
  Check,
  ChevronsUpDown,
  CircleUser,
  FolderKanban,
  GitPullRequest,
  Globe,
  Inbox,
  LogIn,
  LogOut,
  Plus,
  Rocket,
  Search,
  Settings,
  Shield,
} from "lucide-react"
import {
  codingSessionCollection,
  issueCollection,
  releaseCollection,
} from "@/lib/collections"
import { ExponentialLogo } from "@/components/exponential-logo"
import { getProjectTypeOption } from "@/lib/project-types"
import { useSession } from "@/hooks/use-session"
import { useUnreadNotificationCount } from "@/hooks/use-unread-notifications"
import { isAdminUser } from "@/lib/auth/app-user"
import { useSignOut } from "@/hooks/use-sign-out"
import { getInitials } from "@/lib/utils"
import type { Project, Workspace } from "@/db/schema"
import {
  useShowWorkspaceChrome,
  useWorkspaceMemberships,
} from "@/hooks/use-workspace-data"
import { CreateProjectDialog } from "@/components/create-project-dialog"
import { CreateWorkspaceDialog } from "@/components/create-workspace-dialog"
import { GettingStartedButton } from "@/components/getting-started/getting-started-button"
import { FeedbackButton } from "@/components/feedback-button"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
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
// isn't subscribed for anonymous public-workspace viewers.
function InboxUnreadBadge() {
  const unread = useUnreadNotificationCount()
  if (unread === 0) return null
  return <SidebarMenuBadge>{unread > 99 ? `99+` : unread}</SidebarMenuBadge>
}

// Open-PR count across the workspace's projects — issue-linked PRs plus open
// RELEASE PRs (EXP-73), matching the Reviews page's synced-row count. Pure
// client-side counting over the already-synced issues + releases shapes.
function ReviewsCountBadge({
  projects,
  workspaceId,
}: {
  projects: Project[] | undefined
  workspaceId?: string
}) {
  const projectIds = useMemo(
    () => (projects ?? []).map((project) => project.id),
    [projects]
  )
  const { data } = useLiveQuery(
    (query) =>
      projectIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) =>
              and(
                inArray(issues.projectId, projectIds),
                eq(issues.prState, `open`)
              )
            )
        : undefined,
    [projectIds.join(`,`)]
  )
  const { data: releasePulls } = useLiveQuery(
    (query) =>
      workspaceId
        ? query
            .from({ releases: releaseCollection })
            .where(({ releases }) =>
              and(
                eq(releases.workspaceId, workspaceId),
                eq(releases.prState, `open`)
              )
            )
        : undefined,
    [workspaceId]
  )
  const count = (data?.length ?? 0) + (releasePulls?.length ?? 0)
  if (count === 0) return null
  return <SidebarMenuBadge>{count > 99 ? `99+` : count}</SidebarMenuBadge>
}

// Live count of running coding sessions in the workspace, for the Agents
// entry. Pure client-side counting over the already-synced coding_sessions
// shape (workspace-scoped by the denormalized workspace_id).
function AgentsRunningBadge({ workspaceId }: { workspaceId?: string }) {
  const { data } = useLiveQuery(
    (query) =>
      workspaceId
        ? query
            .from({ sessions: codingSessionCollection })
            .where(({ sessions }) =>
              and(
                eq(sessions.workspaceId, workspaceId),
                eq(sessions.status, `running`)
              )
            )
        : undefined,
    [workspaceId]
  )
  const count = data?.length ?? 0
  if (count === 0) return null
  return <SidebarMenuBadge>{count > 99 ? `99+` : count}</SidebarMenuBadge>
}

// Live count of UNSHIPPED releases in the workspace, for the Releases entry.
// Pure client-side counting over the already-synced releases shape (member-
// only, workspace-scoped). shipped_at is filtered in JS — the live-query
// operator set has no null check, and workspace release lists are tiny.
function ReleasesUnshippedBadge({ workspaceId }: { workspaceId?: string }) {
  const { data } = useLiveQuery(
    (query) =>
      workspaceId
        ? query
            .from({ releases: releaseCollection })
            .where(({ releases }) => eq(releases.workspaceId, workspaceId))
        : undefined,
    [workspaceId]
  )
  const count = (data ?? []).filter(
    (release) => release.shippedAt === null
  ).length
  if (count === 0) return null
  return <SidebarMenuBadge>{count > 99 ? `99+` : count}</SidebarMenuBadge>
}

interface WorkspaceSidebarProps {
  workspaceSlug: string
  workspace: Workspace | null | undefined
  projects: Project[] | undefined
  onOpenSearch: () => void
}

export function WorkspaceSidebar({
  workspaceSlug,
  workspace,
  projects,
  onOpenSearch,
}: WorkspaceSidebarProps) {
  const { data: session } = useSession()
  const navigate = useNavigate()
  const [createProjectOpen, setCreateProjectOpen] = useState(false)
  const [createWorkspaceOpen, setCreateWorkspaceOpen] = useState(false)
  const { myWorkspaces } = useWorkspaceMemberships(session?.user?.id)
  const isAuthed = Boolean(session?.user)
  // Solo users never see the "workspace" concept: no switcher, no name. The
  // chrome is revealed once they collaborate (2+ humans) or own 2+ workspaces.
  // Anonymous public viewers always get the chrome (it carries their Sign in).
  const chromeFromData = useShowWorkspaceChrome(
    workspace?.id,
    session?.user?.id
  )
  const showChrome = !isAuthed || chromeFromData

  const handleSignOut = useSignOut()

  const userInitials = session?.user?.name
    ? getInitials(session.user.name)
    : `?`

  return (
    <>
      <Sidebar>
        <SidebarHeader className="p-2">
          {showChrome ? (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <SidebarMenuButton
                  className="w-full h-10"
                  aria-label="Workspace switcher"
                >
                  <div className="flex h-7 w-7 items-center justify-center rounded-md bg-primary text-primary-foreground text-xs font-bold shrink-0">
                    {workspace?.name?.[0]?.toUpperCase() ??
                      workspaceSlug[0]?.toUpperCase() ??
                      `E`}
                  </div>
                  <span className="text-sm font-semibold truncate">
                    {workspace?.name ?? workspaceSlug}
                  </span>
                  <ChevronsUpDown className="ml-auto h-4 w-4 shrink-0" />
                </SidebarMenuButton>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start" className="w-56">
                {isAuthed &&
                  myWorkspaces.map((ws) => (
                    <DropdownMenuItem
                      key={ws.id}
                      onClick={() =>
                        navigate({
                          to: `/w/$workspaceSlug`,
                          params: { workspaceSlug: ws.slug },
                        })
                      }
                    >
                      <div className="flex h-5 w-5 items-center justify-center rounded bg-primary text-primary-foreground text-[0.625rem] font-bold shrink-0">
                        {ws.name[0]?.toUpperCase()}
                      </div>
                      <span className="truncate">{ws.name}</span>
                      {ws.slug === workspaceSlug && (
                        <Check className="ml-auto h-4 w-4" />
                      )}
                    </DropdownMenuItem>
                  ))}
                {isAuthed && <DropdownMenuSeparator />}
                {/* Workspace creation is admin-only: regular users live in
                    their single personal workspace (server enforces too). */}
                {isAdminUser(session?.user) && (
                  <DropdownMenuItem onClick={() => setCreateWorkspaceOpen(true)}>
                    <Plus className="h-4 w-4" />
                    New workspace
                  </DropdownMenuItem>
                )}
                {isAuthed && (
                  <DropdownMenuItem
                    onClick={() =>
                      navigate({
                        to: `/w/$workspaceSlug/settings`,
                        params: { workspaceSlug },
                      })
                    }
                  >
                    <Settings className="h-4 w-4" />
                    Workspace settings
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
          {(isAuthed || workspace) && (
            <SidebarGroup>
              <SidebarGroupContent>
                <SidebarMenu>
                  {workspace && (
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
                        <Link
                          to="/w/$workspaceSlug/my-issues"
                          params={{ workspaceSlug }}
                        >
                          <CircleUser className="h-4 w-4" />
                          <span>My Issues</span>
                        </Link>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  )}
                  {isAuthed && (
                    <SidebarMenuItem>
                      <SidebarMenuButton asChild>
                        <Link to="/w/$workspaceSlug/inbox" params={{ workspaceSlug }}>
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
                          to="/w/$workspaceSlug/reviews"
                          params={{ workspaceSlug }}
                        >
                          <GitPullRequest className="h-4 w-4" />
                          <span>Reviews</span>
                        </Link>
                      </SidebarMenuButton>
                      <ReviewsCountBadge
                        projects={projects}
                        workspaceId={workspace?.id}
                      />
                    </SidebarMenuItem>
                  )}
                  {isAuthed && (
                    <SidebarMenuItem>
                      <SidebarMenuButton asChild>
                        <Link
                          to="/w/$workspaceSlug/agents"
                          params={{ workspaceSlug }}
                        >
                          <Bot className="h-4 w-4" />
                          <span>Agents</span>
                        </Link>
                      </SidebarMenuButton>
                      <AgentsRunningBadge workspaceId={workspace?.id} />
                    </SidebarMenuItem>
                  )}
                  {isAuthed && (
                    <SidebarMenuItem>
                      <SidebarMenuButton asChild>
                        <Link
                          to="/w/$workspaceSlug/releases"
                          params={{ workspaceSlug }}
                        >
                          <Rocket className="h-4 w-4" />
                          <span>Releases</span>
                        </Link>
                      </SidebarMenuButton>
                      <ReleasesUnshippedBadge workspaceId={workspace?.id} />
                    </SidebarMenuItem>
                  )}
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          )}

          <SidebarGroup>
            <SidebarGroupLabel>Projects</SidebarGroupLabel>
            {isAuthed && (
              <SidebarGroupAction
                onClick={() => setCreateProjectOpen(true)}
                title="Create project"
                aria-label="Create project"
              >
                <Plus className="h-4 w-4" />
              </SidebarGroupAction>
            )}
            <SidebarGroupContent>
              <SidebarMenu>
                {!projects || projects.length === 0 ? (
                  <SidebarMenuItem>
                    <SidebarMenuButton disabled>
                      <FolderKanban className="h-4 w-4" />
                      <span className="text-muted-foreground">
                        No projects yet
                      </span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ) : (
                  projects.map((project) => {
                    const TypeIcon = getProjectTypeOption(project.type).icon
                    return (
                      <SidebarMenuItem key={project.id}>
                        <SidebarMenuButton asChild>
                          <Link
                            to="/w/$workspaceSlug/projects/$projectSlug"
                            params={{
                              workspaceSlug,
                              projectSlug: project.slug,
                            }}
                          >
                            <TypeIcon
                              className="h-4 w-4 shrink-0"
                              style={{ color: project.color }}
                            />
                            <span>{project.name}</span>
                            {project.type === `feedback` && (
                              <Globe className="ml-auto h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                            )}
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
          <SidebarMenu>
            {/* EXP-88: re-entry point for the Getting started cards once the
                board's inline block is gone (issues exist / dismissed). */}
            {isAuthed && (
              <GettingStartedButton
                workspaceSlug={workspaceSlug}
                workspace={workspace}
              />
            )}
            <FeedbackButton />
          </SidebarMenu>
          {isAuthed ? (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <SidebarMenuButton className="w-full" aria-label="User menu">
                  <Avatar className="h-6 w-6">
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
                    workspace live here instead (framed as account-level). */}
                {!showChrome && (
                  <DropdownMenuItem
                    onClick={() =>
                      navigate({
                        to: `/w/$workspaceSlug/settings`,
                        params: { workspaceSlug },
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
                {!showChrome && isAdminUser(session?.user) && (
                  <DropdownMenuItem onClick={() => setCreateWorkspaceOpen(true)}>
                    <Plus className="mr-2 h-4 w-4" />
                    New workspace
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

      {workspace && isAuthed && (
        <CreateProjectDialog
          open={createProjectOpen}
          onOpenChange={setCreateProjectOpen}
          workspaceId={workspace.id}
        />
      )}
      {isAdminUser(session?.user) && (
        <CreateWorkspaceDialog
          open={createWorkspaceOpen}
          onOpenChange={setCreateWorkspaceOpen}
        />
      )}
    </>
  )
}
