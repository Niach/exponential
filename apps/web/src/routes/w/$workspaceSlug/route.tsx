import {
  createFileRoute,
  Outlet,
  notFound,
  redirect,
} from "@tanstack/react-router"
import { TRPCClientError } from "@trpc/client"
import { useEffect, useState } from "react"
import { fetchSessionOnce } from "@/lib/auth/client"
import { trpc } from "@/lib/trpc-client"
import { SidebarProvider } from "@/components/ui/sidebar"
import { WorkspaceMobileTopbar } from "@/components/workspace/mobile-topbar"
import { WorkspaceSidebar } from "@/components/workspace/sidebar"
import { IssueSearchSheet } from "@/components/issue-search-sheet"
import { FeedbackWidgetProvider } from "@/components/feedback-widget-provider"
import { IssueRefProvider } from "@/components/issue-ref-provider"
import { WorkspaceJoinGate } from "@/components/workspace/join-gate"
import {
  useWorkspaceBySlug,
  useWorkspaceProjects,
} from "@/hooks/use-workspace-data"

export const Route = createFileRoute(`/w/$workspaceSlug`)({
  beforeLoad: async ({ params }) => {
    const slug = params.workspaceSlug
    const sessionData = await fetchSessionOnce()
    const session = sessionData?.session ?? null
    const user = sessionData?.user ?? null

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
      return { session, user, joinGateWorkspace: null }
    }

    // Public-aware lookup. Anonymous callers can resolve a public workspace
    // and continue; authed non-members of a private workspace get NOT_FOUND.
    try {
      const workspace = await trpc.workspaces.getBySlug.query({ slug })
      // Public boards don't sync for signed-in non-members — instead of an
      // empty shell, show the explicit join gate.
      if (workspace.isPublic && session && workspace.membership === null) {
        return { session, user, joinGateWorkspace: workspace }
      }
      return { session, user, joinGateWorkspace: null }
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
  const { joinGateWorkspace } = Route.useRouteContext()
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const projects = useWorkspaceProjects(workspace?.id)
  const [searchOpen, setSearchOpen] = useState(false)

  // Linear-style global search shortcut: Cmd/Ctrl+F always opens the app
  // search, unconditionally (mirrors the Cmd+B sidebar-toggle handler in
  // `components/ui/sidebar.tsx`).
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === `f` && (event.metaKey || event.ctrlKey)) {
        event.preventDefault()
        setSearchOpen(true)
      }
    }
    window.addEventListener(`keydown`, handleKeyDown)
    return () => window.removeEventListener(`keydown`, handleKeyDown)
  }, [])

  if (joinGateWorkspace) {
    return (
      <WorkspaceJoinGate
        workspaceSlug={workspaceSlug}
        workspaceName={joinGateWorkspace.name}
        workspaceId={joinGateWorkspace.id}
      />
    )
  }

  return (
    <SidebarProvider>
      {/* Workspace-scoped `#IDENTIFIER` resolution for pill rendering, the
          composer autocomplete and the duplicate-of picker. */}
      <IssueRefProvider
        workspaceId={workspace?.id}
        workspaceSlug={workspaceSlug}
      >
        <FeedbackWidgetProvider />
        <WorkspaceSidebar
          workspaceSlug={workspaceSlug}
          workspace={workspace}
          projects={projects}
          onOpenSearch={() => setSearchOpen(true)}
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

        {workspace && (
          <IssueSearchSheet
            open={searchOpen}
            onOpenChange={setSearchOpen}
            workspaceId={workspace.id}
            workspaceSlug={workspaceSlug}
          />
        )}
      </IssueRefProvider>
    </SidebarProvider>
  )
}
