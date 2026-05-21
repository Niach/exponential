import { useState } from "react"
import {
  createFileRoute,
  Outlet,
  Link,
  notFound,
  redirect,
  useNavigate,
  useParams,
} from "@tanstack/react-router"
import { TRPCClientError } from "@trpc/client"
import { authClient } from "@/lib/auth-client"
import { trpc } from "@/lib/trpc-client"
import { CreateProjectDialog } from "@/components/create-project-dialog"
import { CreateWorkspaceDialog } from "@/components/create-workspace-dialog"
import { FeedbackButton } from "@/components/feedback-button"
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
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarTrigger,
} from "@/components/ui/sidebar"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Separator } from "@/components/ui/separator"
import {
  FolderKanban,
  LayoutList,
  LogIn,
  LogOut,
  ChevronsUpDown,
  Plus,
  Settings,
  Check,
  Plug,
  Shield,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import type { Project } from "@/db/schema"
import {
  useWorkspaceBySlug,
  useWorkspaceMemberships,
  useWorkspaceProjects,
} from "@/hooks/use-workspace-data"
import { getInitials } from "@/lib/utils"

export const Route = createFileRoute(`/w/$workspaceSlug`)({
  beforeLoad: async ({ params }) => {
    const slug = params.workspaceSlug
    const sessionResult = await authClient.getSession()
    const session = sessionResult.data?.session

    // Magic "default" slug resolves to the user's default workspace.
    if (slug === `default`) {
      if (!session) {
        throw redirect({
          to: `/auth/login`,
          search: { redirect: undefined },
        })
      }
      const { workspace } = await trpc.workspaces.ensureDefault.mutate()
      if (workspace.slug !== `default`) {
        throw redirect({
          to: `/w/$workspaceSlug`,
          params: { workspaceSlug: workspace.slug },
        })
      }
      return
    }

    // Public-aware lookup. Anonymous callers can resolve a public workspace
    // and continue; authed non-members of a private workspace get NOT_FOUND.
    try {
      await trpc.workspaces.getBySlug.query({ slug })
      return
    } catch (e) {
      const isNotFound =
        e instanceof TRPCClientError && e.data?.code === `NOT_FOUND`
      if (!isNotFound) throw e
      // The workspace either doesn't exist or is private and we can't read it.
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
  component: WorkspaceLayout,
})

function WorkspaceLayout() {
  const { workspaceSlug } = Route.useParams()
  const { data: session } = authClient.useSession()
  const navigate = useNavigate()
  const [createProjectOpen, setCreateProjectOpen] = useState(false)
  const [createWorkspaceOpen, setCreateWorkspaceOpen] = useState(false)
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const projects = useWorkspaceProjects(workspace?.id)
  const { myWorkspaces } = useWorkspaceMemberships(session?.user?.id)
  const isAuthed = Boolean(session?.user)

  const handleSignOut = async () => {
    await authClient.signOut()
    navigate({
      to: `/auth/login`,
      search: { redirect: undefined },
    })
  }

  const userInitials = session?.user?.name
    ? getInitials(session.user.name)
    : `?`

  return (
    <SidebarProvider>
      <Sidebar>
        <SidebarHeader className="p-2">
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
                {workspace?.isPublic && (
                  <span className="rounded bg-accent px-1.5 py-0.5 text-[0.625rem] font-medium text-muted-foreground uppercase tracking-wide">
                    Public
                  </span>
                )}
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
              {isAuthed && (
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
        </SidebarHeader>

        <Separator />

        <SidebarContent>
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
                  projects.map((project) => (
                    <SidebarMenuItem key={project.id}>
                      <SidebarMenuButton asChild>
                        <Link
                          to="/w/$workspaceSlug/projects/$projectSlug"
                          params={{ workspaceSlug, projectSlug: project.slug }}
                        >
                          <div
                            className="h-3 w-3 rounded-full shrink-0"
                            style={{ backgroundColor: project.color }}
                          />
                          <span>{project.name}</span>
                        </Link>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  ))
                )}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>

          <SidebarGroup>
            <SidebarGroupLabel>Views</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                <SidebarMenuItem>
                  <SidebarMenuButton disabled>
                    <LayoutList className="h-4 w-4" />
                    <span className="text-muted-foreground">No views yet</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        </SidebarContent>

        <SidebarFooter>
          <SidebarMenu>
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
                {(session?.user as { isAdmin?: boolean })?.isAdmin && (
                  <DropdownMenuItem
                    onClick={() => navigate({ to: `/admin/users` })}
                  >
                    <Shield className="mr-2 h-4 w-4" />
                    Admin
                  </DropdownMenuItem>
                )}
                <DropdownMenuItem
                  onClick={() => navigate({ to: `/account/integrations` })}
                >
                  <Plug className="mr-2 h-4 w-4" />
                  Integrations
                </DropdownMenuItem>
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

      <main className="flex-1 flex flex-col min-h-screen">
        <MobileTopbar workspaceSlug={workspaceSlug} projects={projects} />
        <div className="flex-1">
          <Outlet />
        </div>
      </main>

      {workspace && isAuthed && (
        <CreateProjectDialog
          open={createProjectOpen}
          onOpenChange={setCreateProjectOpen}
          workspaceId={workspace.id}
        />
      )}
      {isAuthed && (
        <CreateWorkspaceDialog
          open={createWorkspaceOpen}
          onOpenChange={setCreateWorkspaceOpen}
        />
      )}
    </SidebarProvider>
  )
}

function MobileTopbar({
  workspaceSlug,
  projects,
}: {
  workspaceSlug: string
  projects: Project[]
}) {
  const params = useParams({ strict: false }) as { projectSlug?: string }
  const { data: session } = authClient.useSession()
  const navigate = useNavigate()
  const activeProject = params.projectSlug
    ? projects.find((p) => p.slug === params.projectSlug)
    : undefined
  const isAuthed = Boolean(session?.user)
  const isAdmin = Boolean((session?.user as { isAdmin?: boolean })?.isAdmin)
  const userInitials = session?.user?.name
    ? getInitials(session.user.name)
    : `?`

  const handleSignOut = async () => {
    await authClient.signOut()
    navigate({
      to: `/auth/login`,
      search: { redirect: undefined },
    })
  }

  return (
    <header className="flex items-center gap-2 border-b px-3 md:px-4 h-12">
      <SidebarTrigger />
      {activeProject && (
        <div className="flex items-center gap-1.5 text-sm font-medium truncate min-w-0 md:hidden">
          <span
            className="h-2.5 w-2.5 rounded-full shrink-0"
            style={{ backgroundColor: activeProject.color }}
          />
          <span className="truncate">{activeProject.name}</span>
        </div>
      )}
      <div className="ml-auto flex items-center gap-1 md:hidden">
        {activeProject && (
          <Button
            asChild
            size="icon"
            variant="ghost"
            className="size-9 text-muted-foreground"
          >
            <Link
              to="/w/$workspaceSlug/projects/$projectSlug"
              params={{ workspaceSlug, projectSlug: activeProject.slug }}
              search={{ new: 1 }}
              aria-label="New issue"
            >
              <Plus className="size-5" />
            </Link>
          </Button>
        )}
        {isAuthed && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                size="icon"
                variant="ghost"
                className="size-9 rounded-full"
                aria-label="User menu"
              >
                <Avatar className="h-7 w-7">
                  <AvatarFallback className="text-xs">
                    {userInitials}
                  </AvatarFallback>
                </Avatar>
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-56">
              {isAdmin && (
                <DropdownMenuItem
                  onClick={() => navigate({ to: `/admin/users` })}
                >
                  <Shield className="mr-2 h-4 w-4" />
                  Admin
                </DropdownMenuItem>
              )}
              <DropdownMenuItem
                onClick={() => navigate({ to: `/account/integrations` })}
              >
                <Plug className="mr-2 h-4 w-4" />
                Integrations
              </DropdownMenuItem>
              <DropdownMenuItem
                onClick={() =>
                  navigate({
                    to: `/w/$workspaceSlug/settings`,
                    params: { workspaceSlug },
                  })
                }
              >
                <Settings className="mr-2 h-4 w-4" />
                Workspace settings
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem onClick={handleSignOut}>
                <LogOut className="mr-2 h-4 w-4" />
                Sign out
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        )}
        {!isAuthed && (
          <Button
            size="icon"
            variant="ghost"
            className="size-9"
            aria-label="Sign in"
            onClick={() =>
              navigate({
                to: `/auth/login`,
                search: { redirect: undefined },
              })
            }
          >
            <LogIn className="size-4" />
          </Button>
        )}
      </div>
    </header>
  )
}
