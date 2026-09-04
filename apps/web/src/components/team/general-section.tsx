import { useEffect, useState } from "react"
import type { Team } from "@/db/schema"
import { Button } from "@/components/ui/button"
import { GlassSectionHeader } from "@/components/ui/glass-rows"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
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
        {/* EXP-719: a label over a real text field, the desktop pane's
            recipe (team_general.rs) and the board form's. The glass
            label/value row read as a display row next to an explicit Save
            button — the value sat right-aligned with nothing marking it as
            editable. The row vocabulary stays for rows that save themselves
            (device name). */}
        <div className="space-y-2">
          <Label htmlFor="team-name">Name</Label>
          <Input
            id="team-name"
            value={name}
            maxLength={255}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === `Enter`) {
                e.preventDefault()
                void handleSave()
              }
            }}
          />
        </div>
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
