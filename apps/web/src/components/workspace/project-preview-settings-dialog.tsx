import { useEffect, useMemo, useState } from "react"
import { Smartphone, Monitor, Apple, SquareTerminal } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { Badge } from "@/components/ui/badge"
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
import type { Platform } from "@exp/db-schema/domain"

const PLATFORM_META: Record<
  Platform,
  { label: string; icon: typeof Monitor }
> = {
  web: { label: `Web`, icon: Monitor },
  android: { label: `Android`, icon: Smartphone },
  ios: { label: `iOS`, icon: Apple },
  command: { label: `Command`, icon: SquareTerminal },
}

function PlatformBadge({ platform }: { platform: Platform }) {
  const meta = PLATFORM_META[platform] ?? {
    label: platform,
    icon: Monitor,
  }
  const Icon = meta.icon
  return (
    <Badge variant="outline" className="shrink-0 gap-1 text-xs font-normal">
      <Icon className="h-3 w-3" />
      {meta.label}
    </Badge>
  )
}

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
  // The desktop apps populate `targets` after they clone + parse the repo file;
  // the web UI only edits the issue-routing target (feedbackProjectId).
  const targets = project?.previewConfig?.targets ?? []
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
          // Preserve the targets the desktop discovered — the web UI never
          // edits them, only the feedback routing target.
          targets: targets.map((t) => ({
            id: t.id,
            name: t.name,
            platform: t.platform,
          })),
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
          <DialogTitle>Run Targets & Preview</DialogTitle>
          <DialogDescription>
            Configure how {project?.name ?? `this project`} is previewed and
            where feedback is filed.
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

          <div className="space-y-2">
            <Label>Run targets</Label>
            {targets.length === 0 ? (
              <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">
                No run targets yet. Open this project in the desktop app to
                discover the targets declared in{` `}
                <code className="font-mono text-xs">.exponential/config.json</code>
                .
              </div>
            ) : (
              <div className="divide-y rounded-md border">
                {targets.map((target) => (
                  <div
                    key={target.id}
                    className="flex items-center gap-3 px-3 py-2"
                  >
                    <span className="min-w-0 flex-1 truncate text-sm font-medium">
                      {target.name}
                    </span>
                    <code className="shrink-0 font-mono text-xs text-muted-foreground">
                      {target.id}
                    </code>
                    <PlatformBadge platform={target.platform} />
                  </div>
                ))}
              </div>
            )}
            <p className="text-xs text-muted-foreground">
              Build &amp; run commands are defined in{` `}
              <code className="font-mono text-xs">.exponential/config.json</code>
              {` `}and read only from the cloned repo — they are never stored or
              run from here.
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
