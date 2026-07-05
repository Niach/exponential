import { useEffect, useMemo, useState } from "react"
import { trpc } from "@/lib/trpc-client"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import type { Project } from "@/db/schema"

export function ProjectPreviewSettingsDialog({
  project,
  workspaceProjects,
  isOwner,
  open,
  onOpenChange,
}: {
  project: Project | null
  workspaceProjects: Project[]
  isOwner: boolean
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const defaultFeedbackId = project?.id ?? ``

  const [feedbackProjectId, setFeedbackProjectId] = useState(defaultFeedbackId)
  const [saving, setSaving] = useState(false)

  // Re-seed local state whenever a different project's dialog opens.
  useEffect(() => {
    if (open && project) {
      setFeedbackProjectId(
        project.previewConfig?.feedbackProjectId ?? project.id
      )
    }
  }, [open, project])

  const dirty = useMemo(() => {
    const current = project?.previewConfig?.feedbackProjectId ?? project?.id
    return feedbackProjectId !== current
  }, [feedbackProjectId, project])

  const handleSave = async () => {
    if (!project || !isOwner) return
    setSaving(true)
    try {
      await trpc.projects.updatePreviewConfig.mutate({
        projectId: project.id,
        previewConfig: {
          feedbackProjectId: feedbackProjectId || undefined,
        },
      })
      onOpenChange(false)
    } finally {
      setSaving(false)
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) setSaving(false)
        onOpenChange(next)
      }}
    >
      <DialogContent className="sm:max-w-[30rem]">
        <DialogHeader>
          <DialogTitle>Feedback project</DialogTitle>
          <DialogDescription>
            Choose where feedback filed for {project?.name ?? `this project`} is
            created.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-5">
          <div className="space-y-2">
            <Label htmlFor="feedback-project">Feedback project</Label>
            <Select
              value={feedbackProjectId}
              onValueChange={setFeedbackProjectId}
              disabled={!isOwner}
            >
              <SelectTrigger id="feedback-project" className="w-full">
                <SelectValue placeholder="Select a project" />
              </SelectTrigger>
              <SelectContent>
                {workspaceProjects.map((p) => (
                  <SelectItem key={p.id} value={p.id}>
                    {p.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p className="text-xs text-muted-foreground">
              Issues filed from the device preview are created in this project.
            </p>
          </div>
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={saving}
          >
            Cancel
          </Button>
          <Button onClick={handleSave} disabled={!isOwner || !dirty || saving}>
            {saving ? `Saving...` : `Save`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
