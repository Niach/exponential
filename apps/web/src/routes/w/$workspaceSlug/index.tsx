import { useEffect, useState } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { FolderKanban } from "lucide-react"
import {
  useWorkspaceBySlug,
  useWorkspaceProjects,
} from "@/hooks/use-workspace-data"
import { EmptyState } from "@/components/empty-state"
import { CreateProjectDialog } from "@/components/create-project-dialog"
import { Button } from "@/components/ui/button"

export const Route = createFileRoute(`/w/$workspaceSlug/`)({
  component: WorkspaceIndexPage,
})

function WorkspaceIndexPage() {
  const { workspaceSlug } = Route.useParams()
  const navigate = useNavigate()
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const projects = useWorkspaceProjects(workspace?.id)
  const [createOpen, setCreateOpen] = useState(false)

  useEffect(() => {
    if (projects && projects.length > 0) {
      navigate({
        to: `/w/$workspaceSlug/projects/$projectSlug`,
        params: { workspaceSlug, projectSlug: projects[0].slug },
        replace: true,
      })
    }
  }, [projects, workspaceSlug, navigate])

  if (!projects || projects.length > 0) {
    return null
  }

  return (
    <div className="flex h-full flex-1 items-center justify-center">
      <EmptyState
        icon={FolderKanban}
        title="Create your first project"
        description="Projects hold your issues. Create one to start tracking work."
      >
        <Button onClick={() => setCreateOpen(true)}>
          <FolderKanban className="mr-2 size-4" />
          Create a project
        </Button>
      </EmptyState>
      {workspace && (
        <CreateProjectDialog
          open={createOpen}
          onOpenChange={setCreateOpen}
          workspaceId={workspace.id}
        />
      )}
    </div>
  )
}
