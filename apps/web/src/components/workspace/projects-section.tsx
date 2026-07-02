import { useState } from "react"
import { SquareTerminal, Trash2 } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { useWorkspaceProjects } from "@/hooks/use-workspace-data"
import { ProjectRunConfigsDialog } from "@/components/workspace/project-run-configs-dialog"

export function WorkspaceProjectsSection({
  workspaceId,
}: {
  workspaceId: string
}) {
  const projects = useWorkspaceProjects(workspaceId)
  const visibleProjects = projects.filter((p) => !p.archivedAt)

  const [deleteTarget, setDeleteTarget] = useState<{
    id: string
    name: string
  } | null>(null)
  const [deleting, setDeleting] = useState(false)
  const [runConfigsProjectId, setRunConfigsProjectId] = useState<
    string | null
  >(null)

  // The section is only rendered for workspace owners (see settings route), so
  // the per-project run-commands dialog is always editable here — but pass an
  // explicit flag so the dialog can gate the mutations defensively.
  const runConfigsProject =
    visibleProjects.find((p) => p.id === runConfigsProjectId) ?? null

  const handleDelete = async () => {
    if (!deleteTarget) return
    setDeleting(true)
    try {
      await trpc.projects.delete.mutate({ projectId: deleteTarget.id })
    } finally {
      setDeleting(false)
      setDeleteTarget(null)
    }
  }

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            Projects
            <Badge variant="secondary" className="text-xs font-normal">
              {visibleProjects.length}
            </Badge>
          </CardTitle>
          <CardDescription>
            Manage projects in this workspace.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {visibleProjects.length === 0 ? (
            <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">
              No projects in this workspace yet.
            </div>
          ) : (
            <div className="divide-y rounded-md border">
              {visibleProjects.map((project) => (
                <div
                  key={project.id}
                  className="flex items-center gap-3 px-3 py-2.5"
                >
                  <div
                    className="h-2.5 w-2.5 shrink-0 rounded-full"
                    style={{ backgroundColor: project.color }}
                  />
                  <span className="min-w-0 flex-1 truncate text-sm font-medium">
                    {project.name}
                  </span>
                  <Badge
                    variant="outline"
                    className="shrink-0 font-mono text-xs"
                  >
                    {project.prefix}
                  </Badge>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7 shrink-0 text-muted-foreground"
                    title="Run commands"
                    onClick={() => setRunConfigsProjectId(project.id)}
                  >
                    <SquareTerminal className="h-3.5 w-3.5" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7 shrink-0 text-muted-foreground hover:text-destructive"
                    onClick={() =>
                      setDeleteTarget({ id: project.id, name: project.name })
                    }
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </Button>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      <Dialog
        open={deleteTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete project</DialogTitle>
            <DialogDescription>
              This will permanently delete{` `}
              <span className="font-semibold text-foreground">
                {deleteTarget?.name}
              </span>
              {` `}
              and all its issues. This cannot be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setDeleteTarget(null)}
              disabled={deleting}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={handleDelete}
              disabled={deleting}
            >
              {deleting ? `Deleting...` : `Delete project`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <ProjectRunConfigsDialog
        project={runConfigsProject}
        isOwner
        open={runConfigsProjectId !== null}
        onOpenChange={(open) => {
          if (!open) setRunConfigsProjectId(null)
        }}
      />
    </>
  )
}
