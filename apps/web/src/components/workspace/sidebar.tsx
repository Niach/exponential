import { useState } from "react"
import { Link, useNavigate } from "@tanstack/react-router"
import {
  Check,
  ChevronsUpDown,
  FolderKanban,
  LayoutList,
  LogIn,
  LogOut,
  Plug,
  Plus,
  Settings,
  Shield,
} from "lucide-react"
import { authClient } from "@/lib/auth/client"
import { getInitials } from "@/lib/utils"
import type { Project, Workspace } from "@/db/schema"
import { useWorkspaceMemberships } from "@/hooks/use-workspace-data"
import { CreateProjectDialog } from "@/components/create-project-dialog"
import { CreateWorkspaceDialog } from "@/components/create-workspace-dialog"
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
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"

interface WorkspaceSidebarProps {
  workspaceSlug: string
  workspace: Workspace | null | undefined
  projects: Project[] | undefined
}

export function WorkspaceSidebar({
  workspaceSlug,
  workspace,
  projects,
}: WorkspaceSidebarProps) {
  const { data: session } = authClient.useSession()
  const navigate = useNavigate()
  const [createProjectOpen, setCreateProjectOpen] = useState(false)
  const [createWorkspaceOpen, setCreateWorkspaceOpen] = useState(false)
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
    <>
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
    </>
  )
}
