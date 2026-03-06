import { useEffect } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import {
  useWorkspaceBySlug,
  useWorkspaceProjects,
} from "@/hooks/use-workspace-data"

export const Route = createFileRoute(`/_authenticated/w/$workspaceSlug/`)({
  component: WorkspaceIndexPage,
})

function WorkspaceIndexPage() {
  const { workspaceSlug } = Route.useParams()
  const navigate = useNavigate()
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const projects = useWorkspaceProjects(workspace?.id)

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
    <div className="flex flex-1 items-center justify-center h-full">
      <div className="text-center text-muted-foreground">
        <p className="text-lg font-medium">No projects yet</p>
        <p className="mt-1 text-sm">
          Create a project from the sidebar to get started.
        </p>
      </div>
    </div>
  )
}
