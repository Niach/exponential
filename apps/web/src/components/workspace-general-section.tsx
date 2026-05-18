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
import { Switch } from "@/components/ui/switch"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { trpc } from "@/lib/trpc-client"
import type { PublicWritePolicy } from "@/lib/domain"

export function WorkspaceGeneralSection({
  workspace,
}: {
  workspace: Workspace
}) {
  const [name, setName] = useState(workspace.name)
  const [isPublic, setIsPublic] = useState(workspace.isPublic)
  const [policy, setPolicy] = useState<PublicWritePolicy>(
    workspace.publicWritePolicy
  )
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    setName(workspace.name)
    setIsPublic(workspace.isPublic)
    setPolicy(workspace.publicWritePolicy)
  }, [workspace.id, workspace.name, workspace.isPublic, workspace.publicWritePolicy])

  const dirty =
    name !== workspace.name ||
    isPublic !== workspace.isPublic ||
    policy !== workspace.publicWritePolicy

  const handleSave = async () => {
    if (!dirty) return
    setSaving(true)
    setError(null)
    try {
      await trpc.workspaces.update.mutate({
        id: workspace.id,
        name: name.trim() || workspace.name,
        isPublic,
        publicWritePolicy: policy,
      })
    } catch (e) {
      setError(e instanceof Error ? e.message : `Failed to save changes`)
    } finally {
      setSaving(false)
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">General</CardTitle>
        <CardDescription>
          Workspace name, visibility, and contribution rules
        </CardDescription>
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

        <div className="flex items-center justify-between gap-4 rounded-md border p-3">
          <div className="space-y-1">
            <Label htmlFor="workspace-public" className="text-sm font-medium">
              Public workspace
            </Label>
            <p className="text-xs text-muted-foreground">
              Anyone can view this workspace, including people who aren't signed
              in. Creating still requires login.
            </p>
          </div>
          <Switch
            id="workspace-public"
            checked={isPublic}
            onCheckedChange={setIsPublic}
          />
        </div>

        {isPublic && (
          <div className="space-y-2">
            <Label htmlFor="workspace-policy">Who can create issues?</Label>
            <Select
              value={policy}
              onValueChange={(v) => setPolicy(v as PublicWritePolicy)}
            >
              <SelectTrigger id="workspace-policy">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="members">Workspace members only</SelectItem>
                <SelectItem value="everyone">Anyone signed in</SelectItem>
              </SelectContent>
            </Select>
            <p className="text-xs text-muted-foreground">
              Updating an issue is always limited to its creator, a workspace
              member, or an admin.
            </p>
          </div>
        )}

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
