import { useState } from "react"
import { CheckCircle2 } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import type { StepProps } from "./wizard"

// The onboarding milestone: create the first issue. Only on success do we let
// the wizard finish (which marks onboarding complete) — so a guided run always
// ends with at least one tracked issue.
export function StepFirstIssue({ projectId, onNext, onSkip }: StepProps) {
  const [title, setTitle] = useState(``)
  const [saving, setSaving] = useState(false)

  const handleCreate = async () => {
    if (!projectId || !title.trim()) return
    setSaving(true)
    try {
      await trpc.issues.create.mutate({
        projectId,
        title: title.trim(),
      })
      onNext()
    } finally {
      setSaving(false)
    }
  }

  return (
    <Card>
      <CardHeader className="text-center">
        <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-primary/10">
          <CheckCircle2 className="size-6 text-primary" />
        </div>
        <CardTitle className="text-xl">Create your first issue</CardTitle>
        <CardDescription>
          Track a task, bug, or idea. You can hand any issue to a coding agent
          later.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-2">
          <Label htmlFor="onb-issue-title">Issue title</Label>
          <Input
            id="onb-issue-title"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="e.g. Set up the project README"
            autoFocus
            onKeyDown={(e) => {
              if (e.key === `Enter` && title.trim()) {
                e.preventDefault()
                void handleCreate()
              }
            }}
          />
        </div>
        <div className="flex justify-between">
          <Button variant="ghost" onClick={onSkip}>
            Skip
          </Button>
          <Button onClick={handleCreate} disabled={saving || !title.trim()}>
            {saving ? `Creating...` : `Create issue`}
          </Button>
        </div>
      </CardContent>
    </Card>
  )
}
