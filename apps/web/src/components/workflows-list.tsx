import { Link } from "@tanstack/react-router"
import { conceptIcon, EmptyState, GlassSectionHeader, ListRow } from "@exp/ui"
import type { SyncedWorkflow } from "@/db/schema"
import {
  workflowBand,
  workflowRowSubtitle,
  WORKFLOW_BANDS,
  WORKFLOWS_EMPTY_BODY,
  WORKFLOWS_EMPTY_TITLE,
} from "@/lib/workflow-view"

// EXP-981: the team's workflows — three bands (Running · Draft · Done) of
// FLAT rows, newest first inside a band, empty bands hidden, no row buttons.
// A row says what the workflow IS (`workflowRowSubtitle`: the shape line, led
// by the status word for the two a band alone does not tell apart) and warns
// when its graph holds a blocking cycle; everything else lives on the detail.

const WorkflowIcon = conceptIcon(`nav-workflows`)
const WarningIcon = conceptIcon(`ui-warning`)

export function WorkflowsList({
  workflows,
  teamSlug,
}: {
  /** The team's rows, already newest first (`useTeamWorkflows`). */
  workflows: SyncedWorkflow[]
  teamSlug: string
}) {
  if (workflows.length === 0) {
    return (
      <EmptyState
        icon={WorkflowIcon}
        title={WORKFLOWS_EMPTY_TITLE}
        description={WORKFLOWS_EMPTY_BODY}
      />
    )
  }

  return (
    <div data-testid="workflows-list">
      {WORKFLOW_BANDS.map((band) => {
        const rows = workflows.filter(
          (workflow) => workflowBand(workflow.status) === band.key
        )
        if (rows.length === 0) return null
        return (
          <div key={band.key} className="mb-6">
            <GlassSectionHeader label={band.title} count={rows.length} />
            <div className="flex flex-col">
              {rows.map((workflow) => (
                <ListRow
                  key={workflow.id}
                  asChild
                  interactive
                  data-testid={`workflow-row-${workflow.id}`}
                >
                  <Link
                    to="/t/$teamSlug/workflows/$workflowId"
                    params={{ teamSlug, workflowId: workflow.id }}
                  >
                    <WorkflowIcon className="size-4 shrink-0 text-muted-foreground" />
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="truncate text-sm font-medium">
                        {workflow.name}
                      </span>
                      <span className="truncate text-xs text-muted-foreground">
                        {workflowRowSubtitle(workflow.status, workflow.metrics)}
                      </span>
                    </span>
                    {workflow.metrics.cycles.length > 0 && (
                      <WarningIcon
                        className="size-4 shrink-0 text-destructive"
                        data-testid={`workflow-row-${workflow.id}-cycle`}
                      />
                    )}
                  </Link>
                </ListRow>
              ))}
            </div>
          </div>
        )
      })}
    </div>
  )
}
