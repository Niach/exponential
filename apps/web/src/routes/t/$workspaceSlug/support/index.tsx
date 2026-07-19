import { createFileRoute, redirect } from "@tanstack/react-router"
import { SupportInbox } from "@/components/helpdesk/support-inbox"
import { useWorkspaceBySlug } from "@/hooks/use-workspace-data"

// The helpdesk member inbox (EXP-128): a 3-pane Featurebase-style view over
// the workspace's support threads. The sidebar links here only when the
// workspace has helpdesk_enabled, but the route itself just renders empty
// lists otherwise — the server-side member gate on the helpdesk router is the
// boundary.
export const Route = createFileRoute(`/t/$workspaceSlug/support/`)({
  beforeLoad: async ({ context }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: undefined },
      })
    }
  },
  component: SupportPage,
})

function SupportPage() {
  const { workspaceSlug } = Route.useParams()
  const workspace = useWorkspaceBySlug(workspaceSlug)

  if (!workspace) {
    return (
      <div className="p-6 text-sm text-muted-foreground">Loading…</div>
    )
  }

  return (
    <SupportInbox workspaceId={workspace.id} workspaceSlug={workspaceSlug} />
  )
}
