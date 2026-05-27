import { useEffect, useState } from "react"
import { Building2 } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { useSession } from "@/hooks/use-session"
import { useWorkspaceBySlug } from "@/hooks/use-workspace-data"
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

export function StepWorkspace({
  workspaceId,
  workspaceSlug,
  onNext,
  onSkip,
}: StepProps) {
  const { data: session } = useSession()
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const defaultName =
    workspace?.name ?? (session?.user?.name ? `${session.user.name}'s Workspace` : ``)
  const [name, setName] = useState(defaultName)
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    if (!name && defaultName) setName(defaultName)
  }, [defaultName])

  const handleContinue = async () => {
    if (!name.trim()) {
      onSkip()
      return
    }
    setSaving(true)
    try {
      await trpc.workspaces.update.mutate({
        id: workspaceId,
        name: name.trim(),
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
          <Building2 className="size-6 text-primary" />
        </div>
        <CardTitle className="text-xl">Welcome to Exponential</CardTitle>
        <CardDescription>
          Let's set up your workspace. You can always change this later.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-2">
          <Label htmlFor="ws-name">Workspace name</Label>
          <Input
            id="ws-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="My Workspace"
            autoFocus
          />
        </div>
        <div className="flex justify-between">
          <Button variant="ghost" onClick={onSkip}>
            Skip
          </Button>
          <Button onClick={handleContinue} disabled={saving}>
            {saving ? `Saving...` : `Continue`}
          </Button>
        </div>
      </CardContent>
    </Card>
  )
}
