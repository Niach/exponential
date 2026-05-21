import { useEffect, useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { CreateIssueDialog } from "@/components/create-issue-dialog"
import { EditIssueDialog } from "@/components/edit-issue-dialog"
import { IssueFilterBar } from "@/components/issue-filter-bar"
import { IssueList } from "@/components/issue-list"
import { useProjectBoardData } from "@/hooks/use-project-board-data"
import { useWorkspacePermissions } from "@/hooks/use-workspace-permissions"
import type { IssueFilters } from "@/lib/filters"
import { emptyFilters } from "@/lib/filters"
import type { IssueStatus } from "@/lib/domain"

type ProjectSearch = {
  description?: string
  edit?: string
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
    edit: typeof search.edit === `string` ? search.edit : undefined,
  }),
  component: ProjectPage,
})

function ProjectPage() {
  const { projectSlug, workspaceSlug } = Route.useParams()
  const search = Route.useSearch()
  const navigate = Route.useNavigate()
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
      void navigate({
        search: (prev) => ({ ...prev, new: undefined, title: undefined, description: undefined }),
        replace: true,
      })
    }
  }, [search.new, search.title, search.description, navigate])
  const [filters, setFilters] = useState<IssueFilters>(emptyFilters)
  // Editing state lives in the URL so the browser back button closes the
  // editor and the issue is deep-linkable. `?edit=<id>`.
  const editingIssueId = search.edit ?? null
  const setEditingIssueId = (id: string | null) => {
    void navigate({
      search: (prev) => ({ ...prev, edit: id ?? undefined }),
    })
  }

  const {
    editingIssue,
    editingIssueLabelIds,
    issueLabelMap,
    labelList,
    project,
    users,
    userMap,
    visibleGroups,
    workspace,
  } = useProjectBoardData({
    editingIssueId,
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
          onIssueClick={(issue) => setEditingIssueId(issue.id)}
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

      {editingIssue && (
        <EditIssueDialog
          key={editingIssue.id}
          open
          onOpenChange={(open) => {
            if (!open) {
              setEditingIssueId(null)
            }
          }}
          issue={editingIssue}
          projectPrefix={project.prefix}
          projectColor={project.color}
          workspaceId={workspace.id}
          issueLabelIds={editingIssueLabelIds}
          users={users}
          readOnly={!permissions.canMutateIssue(editingIssue)}
          restrictModeration={!permissions.isModerator && workspace.isPublic}
        />
      )}
    </div>
  )
}
