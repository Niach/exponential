import { useEffect, useState } from "react"
import type { Team } from "@/db/schema"
import {
  GlassGroup,
  GlassInputRow,
  GlassSectionHeader,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@exp/ui"
import { trpc } from "@/lib/trpc-client"
import { ESTIMATION_TYPE_OPTIONS } from "@/lib/issue-estimate"
import type { IssueEstimation } from "@/lib/domain"

// Team visibility is deliberately NOT configurable: every team is
// member-only (EXP-180), so this section is just the name.
export function TeamGeneralSection({ team }: { team: Team }) {
  const [name, setName] = useState(team.name)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [estimationBusy, setEstimationBusy] = useState(false)
  const estimationType = (team.estimationType ?? `none`) as IssueEstimation

  // EXP-630: the estimate scale — Linear's "Estimates" setting, `none` = off.
  const persistEstimation = async (next: IssueEstimation) => {
    if (next === estimationType) return
    setEstimationBusy(true)
    setError(null)
    try {
      await trpc.teams.update.mutate({ teamId: team.id, estimationType: next })
    } catch (e) {
      setError(e instanceof Error ? e.message : `Failed to save changes`)
    } finally {
      setEstimationBusy(false)
    }
  }

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
        {/* EXP-818: the Linear settings row — label left, value right, and
            it SAVES ITSELF on blur and on Enter (the device editor's Name row,
            the IDE's team_general.rs twin). No Save button. */}
        <GlassGroup>
          <GlassInputRow
            id="team-name"
            label="Name"
            value={name}
            maxLength={255}
            onChange={(e) => setName(e.target.value)}
            onBlur={() => void handleSave()}
            onKeyDown={(e) => {
              if (e.key === `Enter`) {
                e.preventDefault()
                e.currentTarget.blur()
              }
            }}
            trailing={
              saving ? (
                <span className="shrink-0 text-xs text-muted-foreground">Saving…</span>
              ) : dirty ? (
                <span className="shrink-0 text-xs text-muted-foreground">Unsaved</span>
              ) : undefined
            }
          />
        </GlassGroup>
      </div>

      <div>
        <GlassSectionHeader label="Estimates" />
        <GlassGroup>
          <div className="flex items-center justify-between gap-3 px-4 py-3">
            <div className="min-w-0">
              <p className="text-sm">Estimate scale</p>
              <p className="text-xs text-muted-foreground">
                Off by default. Pick a scale to size issues from their
                properties row; a scale change keeps the values already set.
              </p>
            </div>
            <Select
              value={estimationType}
              disabled={estimationBusy}
              onValueChange={(next) => void persistEstimation(next as IssueEstimation)}
            >
              <SelectTrigger className="w-64" aria-label="Estimate scale">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {ESTIMATION_TYPE_OPTIONS.map((option) => (
                  <SelectItem key={option.value} value={option.value}>
                    {option.label}
                    {option.hint && (
                      <span className="ml-1.5 whitespace-nowrap text-muted-foreground">({option.hint})</span>
                    )}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </GlassGroup>
      </div>

      {error && <p className="text-sm text-destructive">{error}</p>}
    </div>
  )
}
