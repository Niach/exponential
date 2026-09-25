// EXP-630: the Issues settings page — how issues behave team-wide: the
// estimate scale (Linear's "Estimates" setting, off by default) and the PR
// automation targets (EXP-319, moved here from the Statuses page). Labels and
// Statuses are its sub-pages in the settings nav.
import { useMemo, useState } from "react"
import { useLiveQuery, eq } from "@tanstack/react-db"
import { Ban, ChevronDown } from "lucide-react"
import type { Team } from "@/db/schema"
import { teamCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import type { IssueEstimation, IssueOption } from "@/lib/domain"
import { useTeamStatuses } from "@/hooks/use-team-statuses"
import type { StatusRowOption } from "@/lib/team-statuses"
import {
  toStatusMenuOptions,
  toStatusPickerStatuses,
} from "@/components/issue-properties/status-dropdown"
import { ESTIMATION_TYPE_OPTIONS } from "@/lib/issue-estimate"
import {
  Button,
  ICON_COMPONENTS,
  StatusPicker,
  type StatusPickerStatus,
  GlassGroup,
  GlassSectionHeader,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Switch,
} from "@exp/ui"

// The estimate scale. Owner-only (teams.update); members see the value.
function EstimatesCard({ team, canEdit }: { team: Team; canEdit: boolean }) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const estimationType = (team.estimationType ?? `none`) as IssueEstimation
  const current = ESTIMATION_TYPE_OPTIONS.find((option) => option.value === estimationType)

  const persist = async (next: IssueEstimation) => {
    if (next === estimationType) return
    setBusy(true)
    setError(null)
    try {
      await trpc.teams.update.mutate({ teamId: team.id, estimationType: next })
    } catch (e) {
      setError(e instanceof Error ? e.message : `Failed to save changes`)
    } finally {
      setBusy(false)
    }
  }

  return (
    <div>
      <GlassSectionHeader label="Estimates" />
      <GlassGroup>
        <div className="flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
          <div className="min-w-0">
            <p className="text-sm">Estimate scale</p>
          </div>
          {canEdit ? (
            <Select
              value={estimationType}
              disabled={busy}
              onValueChange={(next) => void persist(next as IssueEstimation)}
            >
              <SelectTrigger className="w-full shrink-0 sm:w-64" aria-label="Estimate scale">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {ESTIMATION_TYPE_OPTIONS.map((option) => (
                  <SelectItem key={option.value} value={option.value}>
                    {option.label}
                    {option.hint && (
                      <span className="ml-1.5 whitespace-nowrap text-muted-foreground">
                        ({option.hint})
                      </span>
                    )}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          ) : (
            <span className="shrink-0 text-sm text-muted-foreground">
              {current?.label ?? `Not in use`}
              {current?.hint ? ` (${current.hint})` : ``}
            </span>
          )}
        </div>
      </GlassGroup>
      {error && <p className="mt-2 text-xs text-destructive">{error}</p>}
    </div>
  )
}

// EXP-319 — per-team PR automation targets. Two pickers over the team's
// statuses (duplicate excluded, it needs a canonical issue) plus a "Do
// nothing" entry. The synced teams row is the source of truth: a NULL
// status_id means the builtin default (In Review / Done — also what the
// FK's SET NULL falls back to when a target status is deleted), and
// *_automation=false means "do nothing".
export const PR_AUTOMATION_ROWS = [
  {
    event: `pr_opened`,
    label: `When a pull request opens`,
    verb: `opens`,
    defaultKey: `in_review`,
  },
  {
    event: `pr_merged`,
    label: `When a pull request merges`,
    verb: `merges`,
    defaultKey: `done`,
  },
] as const

export function PrAutomationCard({
  teamId,
  options,
}: {
  teamId: string
  options: StatusRowOption[]
}) {
  const { data: teamRows } = useLiveQuery(
    (query) =>
      query
        .from({ teams: teamCollection })
        .where(({ teams }) => eq(teams.id, teamId)),
    [teamId]
  )
  const team = teamRows?.[0]
  const [error, setError] = useState<string | null>(null)

  const pickable = useMemo(
    () => options.filter((option) => option.category !== `duplicate`),
    [options]
  )
  const menuOptions = useMemo<IssueOption<string>[]>(
    () => [
      ...toStatusMenuOptions(pickable),
      {
        value: `none`,
        label: `Do nothing`,
        icon: Ban,
        color: `text-muted-foreground`,
      },
    ],
    [pickable]
  )

  // The same rows for the PICKER (EXP-1021's shared `StatusPicker`, which
  // takes resolved status rows) — `menuOptions` above stays the TRIGGER's
  // view of them, which splits the colour into a class and a hex.
  const statusRows = useMemo<StatusPickerStatus[]>(
    () => [
      ...toStatusPickerStatuses(pickable),
      // Not a status: `*Automation=false` turns the automation off entirely,
      // so the one target that is not a target rides the same list.
      {
        id: `none`,
        name: `Do nothing`,
        category: `cancelled`,
        icon: ICON_COMPONENTS.ban,
        colorHex: `text-muted-foreground`,
      },
    ],
    [pickable]
  )

  const persist = async (
    event: `pr_opened` | `pr_merged`,
    target: string,
    current: string
  ) => {
    if (target === current) return
    try {
      const { txId } = await trpc.statuses.setPrAutomation.mutate({
        teamId,
        event,
        target,
      })
      await teamCollection.utils.awaitTxId(txId)
      setError(null)
    } catch (err) {
      setError(
        err instanceof Error ? err.message : `Failed to update PR automation.`
      )
    }
  }

  // EXP-711: merge ends the PR's live coding sessions by default (EXP-498);
  // the switch turns that off team-wide. Pre-column rows read as enabled,
  // matching the server default.
  const [endSessionsBusy, setEndSessionsBusy] = useState(false)
  const persistEndSessions = async (enabled: boolean) => {
    setEndSessionsBusy(true)
    try {
      const { txId } = await trpc.statuses.setEndSessionsOnMerge.mutate({
        teamId,
        enabled,
      })
      await teamCollection.utils.awaitTxId(txId)
      setError(null)
    } catch (err) {
      setError(
        err instanceof Error ? err.message : `Failed to update PR automation.`
      )
    } finally {
      setEndSessionsBusy(false)
    }
  }

  if (!team || pickable.length === 0) return null

  return (
    <div>
      <GlassSectionHeader label="PR automation" />
      <GlassGroup>
          {PR_AUTOMATION_ROWS.map(({ event, label, defaultKey }) => {
            const statusId =
              event === `pr_opened`
                ? team.prOpenedStatusId
                : team.prMergedStatusId
            const automation =
              event === `pr_opened`
                ? team.prOpenedAutomation
                : team.prMergedAutomation
            const defaultId =
              pickable.find((option) => option.builtinKey === defaultKey)?.id ??
              `none`
            const value =
              automation === false
                ? `none`
                : statusId && pickable.some((option) => option.id === statusId)
                  ? statusId
                  : defaultId

            // EXP-958: the trigger draws the row `value` names, resolved here
            // — a target the team no longer has falls back to the builtin
            // default (`defaultId`, never the display-ordered first row,
            // REV2-85), which the picker itself would render as nothing.
            const current =
              menuOptions.find((option) => option.value === value) ??
              menuOptions.find((option) => option.value === defaultId) ??
              menuOptions[0]
            const CurrentIcon = current.icon

            return (
              <div
                key={event}
                className="flex items-center justify-between gap-3 px-4 py-3"
              >
                <span className="min-w-0 text-sm">{label}, move issues to</span>
                <StatusPicker
                  value={value}
                  statuses={statusRows}
                  mobileTitle={label}
                  align="end"
                  width="sm"
                  onChange={(picked) => void persist(event, picked, value)}
                  trigger={
                    // Fixed width so both rows' triggers line up (EXP-328) —
                    // the label ellipsizes instead of stretching the button.
                    <Button
                      variant="outline"
                      size="sm"
                      className="w-44 shrink-0 justify-start"
                    >
                      <CurrentIcon
                        className={`h-4 w-4 ${current.color}`}
                        style={
                          current.colorHex
                            ? { color: current.colorHex }
                            : undefined
                        }
                      />
                      <span className="flex-1 truncate text-left">
                        {current.label}
                      </span>
                      <ChevronDown className="h-3 w-3 text-muted-foreground" />
                    </Button>
                  }
                />
              </div>
            )
          })}
        <div className="flex items-center justify-between gap-3 px-4 py-3">
          <div className="min-w-0">
            <p className="text-sm">
              When a pull request merges, end its coding sessions
            </p>
            <p className="text-xs text-muted-foreground">
              The session that merged its own pull request always keeps
              running.
            </p>
          </div>
          <Switch
            checked={team.endSessionsOnMerge !== false}
            disabled={endSessionsBusy}
            onCheckedChange={(next) => void persistEndSessions(next)}
            aria-label="End coding sessions when a pull request merges"
          />
        </div>
      </GlassGroup>
      {error && <p className="mt-2 text-xs text-destructive">{error}</p>}
    </div>
  )
}


export function TeamIssuesSection({ team, canEdit }: { team: Team; canEdit: boolean }) {
  const { options } = useTeamStatuses(team.id)
  return (
    <div className="space-y-6">
      <EstimatesCard team={team} canEdit={canEdit} />
      <PrAutomationCard teamId={team.id} options={options} />
    </div>
  )
}
