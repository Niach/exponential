import { useEffect, useState } from "react"
import type { Team } from "@/db/schema"
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

// Team visibility is deliberately NOT configurable: every team is
// member-only (EXP-180), so this card is just the name.
export function TeamGeneralSection({
  team,
  solo = false,
}: {
  team: Team
  // Solo users don't see the "team" concept, so the name field (a name
  // nobody else sees) is hidden — nothing to render.
  solo?: boolean
}) {
  const [name, setName] = useState(team.name)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    setName(team.name)
  }, [team.id, team.name])

  const dirty = name !== team.name

  const handleSave = async () => {
    if (!dirty) return
    setSaving(true)
    setError(null)
    try {
      await trpc.teams.update.mutate({
        id: team.id,
        name: name.trim() || team.name,
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
          <Label htmlFor="team-name">Name</Label>
          <Input
            id="team-name"
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
