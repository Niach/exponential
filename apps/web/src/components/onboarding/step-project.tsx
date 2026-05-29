import { useState } from "react"
import { FolderKanban } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { invalidateBillingCache } from "@/hooks/use-billing"
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
import { ColorSwatchGrid } from "@/components/ui/color-swatch-grid"
import type { StepProps } from "./wizard"
import { derivePrefix } from "@/lib/project"

export function StepProject({
  workspaceId,
  onNext,
  onSkip,
}: StepProps) {
  const [name, setName] = useState(``)
  const [prefix, setPrefix] = useState(``)
  const [color, setColor] = useState(`#6366f1`)
  const [saving, setSaving] = useState(false)

  const handleNameChange = (value: string) => {
    setName(value)
    setPrefix(derivePrefix(value))
  }

  const handleContinue = async () => {
    if (!name.trim() || !prefix.trim()) {
      onSkip()
      return
    }
    setSaving(true)
    try {
      await trpc.projects.create.mutate({
        workspaceId,
        name: name.trim(),
        prefix: prefix.trim(),
        color,
      })
      invalidateBillingCache()
      onNext()
    } finally {
      setSaving(false)
    }
  }

  return (
    <Card>
      <CardHeader className="text-center">
        <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-primary/10">
          <FolderKanban className="size-6 text-primary" />
        </div>
        <CardTitle className="text-xl">Create your first project</CardTitle>
        <CardDescription>
          Projects contain your issues and help organize work.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-2">
          <Label htmlFor="onb-project-name">Project name</Label>
          <Input
            id="onb-project-name"
            value={name}
            onChange={(e) => handleNameChange(e.target.value)}
            placeholder="e.g. Backend API"
            autoFocus
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="onb-project-prefix">Prefix</Label>
          <Input
            id="onb-project-prefix"
            value={prefix}
            onChange={(e) => setPrefix(e.target.value.toUpperCase())}
            placeholder="e.g. API"
            maxLength={10}
          />
        </div>
        <div className="space-y-2">
          <Label>Color</Label>
          <ColorSwatchGrid value={color} onChange={setColor} />
        </div>
        <div className="flex justify-between">
          <Button variant="ghost" onClick={onSkip}>
            Skip
          </Button>
          <Button
            onClick={handleContinue}
            disabled={saving || (!name.trim() && !prefix.trim())}
          >
            {saving ? `Creating...` : `Continue`}
          </Button>
        </div>
      </CardContent>
    </Card>
  )
}
