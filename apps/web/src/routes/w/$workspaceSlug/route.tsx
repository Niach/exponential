import {
  createFileRoute,
  Outlet,
  notFound,
  redirect,
} from "@tanstack/react-router"
import { TRPCClientError } from "@trpc/client"
import { authClient } from "@/lib/auth/client"
import { trpc } from "@/lib/trpc-client"
import { SidebarProvider } from "@/components/ui/sidebar"
import { WorkspaceMobileTopbar } from "@/components/workspace/mobile-topbar"
import { WorkspaceSidebar } from "@/components/workspace/sidebar"
import {
  useWorkspaceBySlug,
  useWorkspaceProjects,
} from "@/hooks/use-workspace-data"

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
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const projects = useWorkspaceProjects(workspace?.id)

  return (
    <SidebarProvider>
      <WorkspaceSidebar
        workspaceSlug={workspaceSlug}
        workspace={workspace}
        projects={projects}
      />

      <main className="flex-1 flex flex-col min-h-screen">
        <WorkspaceMobileTopbar
          workspaceSlug={workspaceSlug}
          projects={projects ?? []}
          workspaceId={workspace?.id}
        />
        <div className="flex-1">
          <Outlet />
        </div>
      </main>
    </SidebarProvider>
  )
}
