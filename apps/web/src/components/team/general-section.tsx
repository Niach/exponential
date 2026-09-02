import { useEffect, useState } from "react"
import type { Team } from "@/db/schema"
import { Button } from "@/components/ui/button"
import {
  GlassGroup,
  GlassInputRow,
  GlassSectionHeader,
} from "@/components/ui/glass-rows"
import { trpc } from "@/lib/trpc-client"

// Team visibility is deliberately NOT configurable: every team is
// member-only (EXP-180), so this section is just the name.
export function TeamGeneralSection({ team }: { team: Team }) {
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
        teamId: team.id,
        name: name.trim() || team.name,
      })
    } catch (e) {
      setError(e instanceof Error ? e.message : `Failed to save changes`)
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="space-y-5">
      <div>
        <GlassSectionHeader label="General" />
        <GlassGroup>
          <GlassInputRow
            id="team-name"
            label="Name"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </GlassGroup>
      </div>

      {error && <p className="text-sm text-destructive">{error}</p>}

      <div className="flex justify-end">
        <Button onClick={handleSave} disabled={!dirty || saving}>
          {saving ? `Saving...` : `Save changes`}
        </Button>
      </div>
    </div>
  )
}
