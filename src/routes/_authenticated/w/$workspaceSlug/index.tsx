import { useEffect } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useLiveQuery, eq } from "@tanstack/react-db"
import {
  workspaceCollection,
  projectCollection,
} from "@/lib/collections"

export const Route = createFileRoute(`/_authenticated/w/$workspaceSlug/`)({
  component: WorkspaceIndexPage,
})

function WorkspaceIndexPage() {
  const { workspaceSlug } = Route.useParams()
  const navigate = useNavigate()

  const { data: allWorkspaces } = useLiveQuery((q) =>
    q.from({ workspaces: workspaceCollection })
  )

  const workspace = allWorkspaces?.find((w) => w.slug === workspaceSlug)

  const { data: projects } = useLiveQuery(
    (q) =>
      workspace
        ? q
            .from({ projects: projectCollection })
            .where(({ projects }) => eq(projects.workspaceId, workspace.id))
            .orderBy(({ projects }) => projects.sortOrder)
        : undefined,
    [workspace?.id]
  )

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
