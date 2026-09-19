import { createFileRoute, redirect } from "@tanstack/react-router"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { WorkflowDetail } from "@/components/workflow-detail"
import { useWorkflow } from "@/hooks/use-workflows"
import { pageTitle } from "@/lib/page-title"
import { WORKFLOWS_TITLE } from "@/lib/workflow-view"

// EXP-981: ONE workflow — its name, its graph, the node panel and the start
// configuration. The row is read from the synced shape, so the page settles
// on its own as Electric catches up; a workflow someone else deleted simply
// says so rather than hanging on a spinner.

export const Route = createFileRoute(`/t/$teamSlug/workflows/$workflowId`)({
  head: () => ({ meta: [{ title: pageTitle(WORKFLOWS_TITLE) }] }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: WorkflowDetailPage,
})

function WorkflowDetailPage() {
  const { teamSlug, workflowId } = Route.useParams()
  const workflow = useWorkflow(workflowId)

  return (
    <div className="h-full overflow-y-auto">
      <div className={`mx-auto w-full max-w-3xl px-4 py-4 ${TAB_BAR_CLEARANCE}`}>
        {workflow ? (
          <WorkflowDetail workflow={workflow} teamSlug={teamSlug} />
        ) : (
          <div className="p-6 text-sm text-muted-foreground">Loading…</div>
        )}
      </div>
    </div>
  )
}
