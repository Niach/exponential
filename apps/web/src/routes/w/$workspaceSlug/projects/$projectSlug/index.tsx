import { useEffect, useMemo, useState } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { CreateIssueDialog } from "@/components/create-issue-dialog"
import { GettingStartedSection } from "@/components/getting-started/getting-started-section"
import { IssueFilterBar } from "@/components/issue-filter-bar"
import { IssueList } from "@/components/issue-list"
import { useProjectBoardData } from "@/hooks/use-project-board-data"
import { useWorkspacePermissions } from "@/hooks/use-workspace-permissions"
import {
  hasActiveFilters as filtersActive,
  issueFilterSearchFromFilters,
  issueFiltersFromSearch,
  parseIssueFilterSearch,
} from "@/lib/filters"
import type { IssueFilterSearch, IssueFilters } from "@/lib/filters"
import type { IssueStatus } from "@/lib/domain"

// Filters live in the URL so a filtered board is shareable and survives a
// refresh (parse/serialize helpers shared with the issue-detail route in
// lib/filters.ts); validateSearch drops anything unrecognised.
type ProjectSearch = IssueFilterSearch & {
  description?: string
  new?: 1
  title?: string
}

export const Route = createFileRoute(
  `/w/$workspaceSlug/projects/$projectSlug/`
)({
  validateSearch: (search: Record<string, unknown>): ProjectSearch => ({
    new: search.new === 1 || search.new === `1` ? 1 : undefined,
    title: typeof search.title === `string` ? search.title : undefined,
    description:
      typeof search.description === `string` ? search.description : undefined,
    ...parseIssueFilterSearch(search),
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
    () => issueFiltersFromSearch(search),
    [search.status, search.priority, search.labels]
  )

  const setFilters = (next: IssueFilters) => {
    void navigate({
      to: `/w/$workspaceSlug/projects/$projectSlug`,
      params: { workspaceSlug, projectSlug },
      search: (prev) => ({
        ...prev,
        ...issueFilterSearchFromFilters(next),
      }),
      replace: true,
    })
  }

  const {
    issueLabelMap,
    issuesReady,
    labelList,
    project,
    totalIssueCount,
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
              // Carry the board's active filters so the detail header's
              // prev/next switcher walks the same filtered+sorted sequence.
              search: {
                status: search.status,
                priority: search.priority,
                labels: search.labels,
              },
            })
          }
          canCreate={permissions.canCreate}
          canMutateIssue={permissions.canMutateIssue}
          canModerate={permissions.isModerator}
          bulkWorkspaceId={workspace.id}
          isLoading={!issuesReady}
          hasAnyIssues={totalIssueCount > 0}
          hasActiveFilters={filtersActive(filters)}
          onClearFilters={() =>
            setFilters({ statuses: [], priorities: [], labelIds: [] })
          }
          // Members only — visitors on a public feedback board (or authed
          // non-members) get the plain empty state.
          emptyStateExtra={
            permissions.isMember ? (
              <GettingStartedSection
                workspaceSlug={workspaceSlug}
                projectIsPublic={project.isPublic}
                canManageWidgets={permissions.canManageWidgets}
              />
            ) : undefined
          }
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
      />
    </div>
  )
}
