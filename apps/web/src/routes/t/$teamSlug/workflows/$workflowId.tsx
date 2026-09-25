import { createFileRoute, redirect } from "@tanstack/react-router"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { WorkflowDetail } from "@/components/workflow-detail"
import {
  parseWorkflowFace,
  type WorkflowFace,
} from "@/components/workflow-detail-selection"
import { useWorkflow } from "@/hooks/use-workflows"
import { pageTitle } from "@/lib/page-title"
import { WORKFLOWS_TITLE } from "@/lib/workflow-view"

// EXP-981/EXP-1084: ONE workflow — every issue, run, change and result of it
// behind the node strip and the face toggle. The row is read from the synced
// shape, so the page settles on its own as Electric catches up. `?face=` and
// `?node=` seed the page's first face and pick, and the page writes them back
// (`replace`) as they change, so a node × face URL is shareable.

type WorkflowSearch = { face?: WorkflowFace; node?: string }

export const Route = createFileRoute(`/t/$teamSlug/workflows/$workflowId`)({
  head: () => ({ meta: [{ title: pageTitle(WORKFLOWS_TITLE) }] }),
  validateSearch: (search: Record<string, unknown>): WorkflowSearch => ({
    face: parseWorkflowFace(search.face),
    node: typeof search.node === `string` && search.node ? search.node : undefined,
  }),
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
  const { face, node } = Route.useSearch()
  const workflow = useWorkflow(workflowId)

  return (
    <div className={`flex h-full min-h-0 flex-col ${TAB_BAR_CLEARANCE}`}>
      {workflow ? (
        <WorkflowDetail
          key={workflow.id}
          workflow={workflow}
          teamSlug={teamSlug}
          initialFace={face}
          initialNodeId={node}
        />
      ) : (
        <div className="p-6 text-sm text-muted-foreground">Loading…</div>
      )}
    </div>
  )
}
