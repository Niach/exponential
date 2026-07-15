import {
  createFileRoute,
  Outlet,
  notFound,
  redirect,
  useParams,
} from "@tanstack/react-router"
import { TRPCClientError } from "@trpc/client"
import { useEffect, useState } from "react"
import { fetchSessionOnce } from "@/lib/auth/client"
import { rememberLastVisited } from "@/lib/last-visited"
import { trpc } from "@/lib/trpc-client"
import { SidebarProvider } from "@/components/ui/sidebar"
import { WorkspaceMobileTopbar } from "@/components/workspace/mobile-topbar"
import { WorkspaceSidebar } from "@/components/workspace/sidebar"
import { IssueSearchSheet } from "@/components/issue-search-sheet"
import { FeedbackWidgetProvider } from "@/components/feedback-widget-provider"
import { IssueRefProvider } from "@/components/issue-ref-provider"
import { MentionProvider } from "@/components/mention-provider"
import { PublicWorkspaceView } from "@/components/public-board/public-board-view"
import {
  useWorkspaceBySlug,
  useWorkspaceProjects,
} from "@/hooks/use-workspace-data"

export const Route = createFileRoute(`/t/$workspaceSlug`)({
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
          to: `/t/$workspaceSlug`,
          params: { workspaceSlug: workspace.slug },
        })
      }
      return { session, user, publicView: false }
    }

    // Public-aware lookup. Members get the normal live app; every other
    // visitor of a workspace hosting a public feedback board — anonymous OR
    // signed-in non-member — gets the read-only public view (v7: no join
    // gate; the widget is the write path). Anything else is NOT_FOUND.
    try {
      const workspace = await trpc.workspaces.getBySlug.query({ slug })
      const publicView = workspace.membership === null
      return { session, user, publicView }
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
  const { session, publicView } = Route.useRouteContext()
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const projects = useWorkspaceProjects(workspace?.id)
  const [searchOpen, setSearchOpen] = useState(false)
  // Child-route params (loose match): `projectSlug` is set while any
  // project-scoped route (board, issue detail) is active.
  const { projectSlug } = useParams({ strict: false })

  // EXP-69: remember this device's last-used workspace/project so the root
  // redirect can jump straight back on the next app entry. Members only —
  // a read-only public-board visit is not "the user's workspace".
  useEffect(() => {
    if (publicView) return
    rememberLastVisited(workspaceSlug, projectSlug)
  }, [publicView, workspaceSlug, projectSlug])

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

  // Non-members (anonymous or signed-in) see the read-only public board —
  // their Electric shapes are membership-scoped, so the live tree below would
  // render an empty shell. The public view fetches through the publicBoard
  // tRPC router instead and owns the whole subtree (no Outlet).
  if (publicView) {
    return (
      <PublicWorkspaceView
        workspaceSlug={workspaceSlug}
        isAuthed={Boolean(session)}
      />
    )
  }

  return (
    <SidebarProvider>
      {/* Workspace-scoped `#IDENTIFIER` + `@email` resolution for pill
          rendering, the editor/composer autocompletes and the duplicate-of
          picker. */}
      <IssueRefProvider
        workspaceId={workspace?.id}
        workspaceSlug={workspaceSlug}
      >
        <MentionProvider workspaceId={workspace?.id}>
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
        </MentionProvider>
      </IssueRefProvider>
    </SidebarProvider>
  )
}
