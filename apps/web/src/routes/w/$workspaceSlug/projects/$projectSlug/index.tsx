import { useEffect, useMemo, useState } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { CreateIssueDialog } from "@/components/create-issue-dialog"
import { IssueFilterBar } from "@/components/issue-filter-bar"
import { IssueList } from "@/components/issue-list"
import { useProjectBoardData } from "@/hooks/use-project-board-data"
import { useWorkspacePermissions } from "@/hooks/use-workspace-permissions"
import type { IssueFilters } from "@/lib/filters"
import { issuePriorityOptions, issueStatusOptions } from "@/lib/domain"
import type { IssuePriority, IssueStatus } from "@/lib/domain"

// Filters live in the URL so a filtered board is shareable and survives a
// refresh. They are stored as clean comma-joined values (?status=todo,in_progress
// &priority=high&labels=<id>,<id>); validateSearch drops anything unrecognised.
type ProjectSearch = {
  description?: string
  new?: 1
  title?: string
  status?: string
  priority?: string
  labels?: string
}

const STATUS_VALUES = issueStatusOptions.map((o) => o.value)
const PRIORITY_VALUES = issuePriorityOptions.map((o) => o.value)

// Coerce a raw search value (array or comma string) to a validated, comma-joined
// string, or undefined when empty — so cleared filters drop out of the URL.
function validatedCsv(
  raw: unknown,
  allowed?: readonly string[]
): string | undefined {
  let arr: string[]
  if (Array.isArray(raw)) {
    arr = raw.filter((v): v is string => typeof v === `string`)
  } else if (typeof raw === `string` && raw.length > 0) {
    arr = raw.split(`,`)
  } else {
    return undefined
  }
  const cleaned = allowed
    ? arr.filter((v) => allowed.includes(v))
    : arr.filter((v) => v.length > 0)
  return cleaned.length ? cleaned.join(`,`) : undefined
}

export const Route = createFileRoute(
  `/w/$workspaceSlug/projects/$projectSlug/`
)({
  validateSearch: (search: Record<string, unknown>): ProjectSearch => ({
    new: search.new === 1 || search.new === `1` ? 1 : undefined,
    title: typeof search.title === `string` ? search.title : undefined,
    description:
      typeof search.description === `string` ? search.description : undefined,
    status: validatedCsv(search.status, STATUS_VALUES),
    priority: validatedCsv(search.priority, PRIORITY_VALUES),
    labels: validatedCsv(search.labels),
  }),
  component: ProjectPage,
})

function ProjectPage() {
  const { projectSlug, workspaceSlug } = Route.useParams()
  const search = Route.useSearch()
  const navigate = useNavigate()
  const [createIssueOpen, setCreateIssueOpen] = useState(false)
  const [defaultStatus, setDefaultStatus] = useState<IssueStatus | undefined>()
  const [prefill, setPrefill] = useState<
    { title?: string; description?: string } | undefined
  >(undefined)

  useEffect(() => {
    if (search.new === 1 || search.title || search.description) {
      setCreateIssueOpen(true)
      setPrefill({
        title: search.title,
        description: search.description,
      })
      // Clear only the one-shot create keys; keep any active filter params.
      void navigate({
        to: `/w/$workspaceSlug/projects/$projectSlug`,
        params: { workspaceSlug, projectSlug },
        search: (prev) => ({
          ...prev,
          new: undefined,
          title: undefined,
          description: undefined,
        }),
        replace: true,
      })
    }
  }, [search.new, search.title, search.description, navigate, workspaceSlug, projectSlug])

  const filters = useMemo<IssueFilters>(
    () => ({
      statuses: search.status
        ? (search.status.split(`,`) as IssueStatus[])
        : [],
      priorities: search.priority
        ? (search.priority.split(`,`) as IssuePriority[])
        : [],
      labelIds: search.labels ? search.labels.split(`,`) : [],
    }),
    [search.status, search.priority, search.labels]
  )

  const setFilters = (next: IssueFilters) => {
    void navigate({
      to: `/w/$workspaceSlug/projects/$projectSlug`,
      params: { workspaceSlug, projectSlug },
      search: (prev) => ({
        ...prev,
        status: next.statuses.length ? next.statuses.join(`,`) : undefined,
        priority: next.priorities.length
          ? next.priorities.join(`,`)
          : undefined,
        labels: next.labelIds.length ? next.labelIds.join(`,`) : undefined,
      }),
      replace: true,
    })
  }

  const {
    issueLabelMap,
    labelList,
    project,
    users,
    userMap,
    visibleGroups,
    workspace,
  } = useProjectBoardData({
    filters,
    projectSlug,
    workspaceSlug,
  })

  const permissions = useWorkspacePermissions(workspace)

  const handleNewIssue = (status?: IssueStatus) => {
    if (!permissions.canCreate) return
    setDefaultStatus(status)
    setCreateIssueOpen(true)
  }

  if (!project || !workspace) {
    return (
      <div className="text-muted-foreground text-sm p-6">
        Loading project...
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      <IssueFilterBar
        filters={filters}
        onFiltersChange={setFilters}
        labels={labelList}
        onNewIssue={() => handleNewIssue()}
        canCreate={permissions.canCreate}
      />

      <div className="flex-1 overflow-auto">
        <IssueList
          groups={visibleGroups}
          issueLabelMap={issueLabelMap}
          labels={labelList}
          users={users}
          userMap={userMap}
          onNewIssue={handleNewIssue}
          onIssueClick={(issue) =>
            void navigate({
              to: `/w/$workspaceSlug/projects/$projectSlug/issues/$issueIdentifier`,
              params: {
                workspaceSlug,
                projectSlug,
                issueIdentifier: issue.identifier,
              },
            })
          }
          canCreate={permissions.canCreate}
          canMutateIssue={permissions.canMutateIssue}
          canModerate={permissions.isModerator}
        />
      </div>

      <CreateIssueDialog
        open={createIssueOpen}
        onOpenChange={(next) => {
          setCreateIssueOpen(next)
          if (!next) setPrefill(undefined)
        }}
        projectId={project.id}
        projectPrefix={project.prefix}
        projectColor={project.color}
        workspaceId={workspace.id}
        defaultStatus={defaultStatus}
        prefill={prefill}
        users={users}
        restrictModeration={!permissions.isModerator && workspace.isPublic}
      />
    </div>
  )
}
