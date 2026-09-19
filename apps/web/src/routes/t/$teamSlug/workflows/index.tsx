import { createFileRoute, redirect } from "@tanstack/react-router"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { WorkflowsList } from "@/components/workflows-list"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useTeamWorkflows } from "@/hooks/use-workflows"
import { pageTitle } from "@/lib/page-title"
import { WORKFLOWS_TITLE } from "@/lib/workflow-view"

// EXP-981: the team's Workflows — the sidebar entry directly after
// Automations (and, on a phone, the Agent page's Workflows button). A
// workflow is a picked set of backlog issues of ONE repository planned as a
// DAG; this page only lists them, the detail does the rest.

export const Route = createFileRoute(`/t/$teamSlug/workflows/`)({
  head: () => ({ meta: [{ title: pageTitle(WORKFLOWS_TITLE) }] }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: WorkflowsPage,
})

function WorkflowsPage() {
  const { teamSlug } = Route.useParams()
  const team = useTeamBySlug(teamSlug)
  const workflows = useTeamWorkflows(team?.id)

  return (
    <div className="h-full overflow-y-auto">
      <div className={`mx-auto w-full max-w-3xl px-4 py-4 ${TAB_BAR_CLEARANCE}`}>
        {team ? (
          <WorkflowsList workflows={workflows} teamSlug={teamSlug} />
        ) : (
          <div className="p-6 text-sm text-muted-foreground">Loading…</div>
        )}
      </div>
    </div>
  )
}
