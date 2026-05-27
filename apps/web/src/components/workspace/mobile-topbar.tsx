import { useState } from "react"
import { Link, useNavigate, useParams } from "@tanstack/react-router"
import {
  LogIn,
  LogOut,
  Plug,
  Plus,
  Search,
  Settings,
  Shield,
} from "lucide-react"
import type { Project } from "@/db/schema"
import { authClient } from "@/lib/auth/client"
import { useSession } from "@/hooks/use-session"
import { getInitials } from "@/lib/utils"
import { IssueSearchSheet } from "@/components/issue-search-sheet"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { SidebarTrigger } from "@/components/ui/sidebar"

interface WorkspaceMobileTopbarProps {
  workspaceSlug: string
  projects: Project[]
  workspaceId?: string
}

export function WorkspaceMobileTopbar({
  workspaceSlug,
  projects,
  workspaceId,
}: WorkspaceMobileTopbarProps) {
  const params = useParams({ strict: false }) as { projectSlug?: string }
  const { data: session } = useSession()
  const navigate = useNavigate()
  const [searchOpen, setSearchOpen] = useState(false)
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
        {workspaceId && (
          <Button
            size="icon"
            variant="ghost"
            className="size-9 text-muted-foreground"
            onClick={() => setSearchOpen(true)}
            aria-label="Search issues"
          >
            <Search className="size-5" />
          </Button>
        )}
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
      {workspaceId && (
        <IssueSearchSheet
          open={searchOpen}
          onOpenChange={setSearchOpen}
          workspaceId={workspaceId}
          workspaceSlug={workspaceSlug}
        />
      )}
    </header>
  )
}
