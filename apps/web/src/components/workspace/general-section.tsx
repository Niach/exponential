import { useEffect, useState } from "react"
import type { Workspace } from "@/db/schema"
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
import { trpc } from "@/lib/trpc-client"

// Workspace visibility is deliberately NOT configurable: the only public
// workspace is the bootstrap-created feedback board. Regular workspaces are
// always private, so this card is just the name.
export function WorkspaceGeneralSection({
  workspace,
  solo = false,
}: {
  workspace: Workspace
  // Solo users don't see the "workspace" concept, so the name field (a name
  // nobody else sees) is hidden — nothing to render.
  solo?: boolean
}) {
  const [name, setName] = useState(workspace.name)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    setName(workspace.name)
  }, [workspace.id, workspace.name])

  const dirty = name !== workspace.name

  const handleSave = async () => {
    if (!dirty) return
    setSaving(true)
    setError(null)
    try {
      await trpc.workspaces.update.mutate({
        id: workspace.id,
        name: name.trim() || workspace.name,
      })
    } catch (e) {
      setError(e instanceof Error ? e.message : `Failed to save changes`)
    } finally {
      setSaving(false)
    }
  }

  if (solo) return null

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">General</CardTitle>
        <CardDescription>Team name</CardDescription>
      </CardHeader>
      <CardContent className="space-y-5">
        <div className="space-y-2">
          <Label htmlFor="workspace-name">Name</Label>
          <Input
            id="workspace-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </div>

        {error && <p className="text-sm text-destructive">{error}</p>}

        <div className="flex justify-end">
          <Button onClick={handleSave} disabled={!dirty || saving}>
            {saving ? `Saving...` : `Save changes`}
          </Button>
        </div>
      </CardContent>
    </Card>
  )
}
