import { useState } from "react"
import {
  createFileRoute,
  Outlet,
  Link,
  redirect,
  useNavigate,
  useParams,
} from "@tanstack/react-router"
import { authClient } from "@/lib/auth-client"
import { trpc } from "@/lib/trpc-client"
import { CreateProjectDialog } from "@/components/create-project-dialog"
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
  LogOut,
  ChevronsUpDown,
  Plus,
  Settings,
  Check,
  Plug,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import type { Project } from "@/db/schema"
import {
  useWorkspaceBySlug,
  useWorkspaceMemberships,
  useWorkspaceProjects,
} from "@/hooks/use-workspace-data"
import { getInitials } from "@/lib/utils"

export const Route = createFileRoute(`/_authenticated/w/$workspaceSlug`)({
  beforeLoad: async ({ params }) => {
    const { workspace } = await trpc.workspaces.ensureDefault.mutate()
    if (params.workspaceSlug === `default` && workspace.slug !== `default`) {
      throw redirect({
        to: `/w/$workspaceSlug`,
        params: { workspaceSlug: workspace.slug },
      })
    }
  },
  component: WorkspaceLayout,
})

function WorkspaceLayout() {
  const { workspaceSlug } = Route.useParams()
  const { data: session } = authClient.useSession()
  const navigate = useNavigate()
  const [createProjectOpen, setCreateProjectOpen] = useState(false)
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const projects = useWorkspaceProjects(workspace?.id)
  const { myWorkspaces } = useWorkspaceMemberships(session?.user?.id)

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
                <ChevronsUpDown className="ml-auto h-4 w-4 shrink-0" />
              </SidebarMenuButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-56">
              {myWorkspaces.map((ws) => (
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
              <DropdownMenuSeparator />
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
            </DropdownMenuContent>
          </DropdownMenu>
        </SidebarHeader>

        <Separator />

        <SidebarContent>
          <SidebarGroup>
            <SidebarGroupLabel>Projects</SidebarGroupLabel>
            <SidebarGroupAction
              onClick={() => setCreateProjectOpen(true)}
              title="Create project"
              aria-label="Create project"
            >
              <Plus className="h-4 w-4" />
            </SidebarGroupAction>
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
        </SidebarFooter>
      </Sidebar>

      <main className="flex-1 flex flex-col min-h-screen">
        <MobileTopbar workspaceSlug={workspaceSlug} projects={projects} />
        <div className="flex-1">
          <Outlet />
        </div>
      </main>

      {workspace && (
        <CreateProjectDialog
          open={createProjectOpen}
          onOpenChange={setCreateProjectOpen}
          workspaceId={workspace.id}
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
  const activeProject = params.projectSlug
    ? projects.find((p) => p.slug === params.projectSlug)
    : undefined

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
      {activeProject && (
        <Button
          asChild
          size="icon-xs"
          variant="ghost"
          className="ml-auto text-muted-foreground md:hidden"
        >
          <Link
            to="/w/$workspaceSlug/projects/$projectSlug"
            params={{ workspaceSlug, projectSlug: activeProject.slug }}
            search={{ new: 1 }}
            aria-label="New issue"
          >
            <Plus className="size-4" />
          </Link>
        </Button>
      )}
    </header>
  )
}
