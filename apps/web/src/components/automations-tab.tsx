import { useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { conceptIcon } from "@/lib/icons.generated"
import {
  nextScheduleRun,
  parseActionTrigger,
  triggerSummary,
} from "@/lib/action-triggers"
import { deviceIsOnline, type SteerDevice } from "@/lib/steer-devices"
import { getActionIcon } from "@/lib/board-icons"
import { codingSessionCollection } from "@/lib/collections"
import { relativeTime } from "@/components/comment-rows/format"
import { trpc } from "@/lib/trpc-client"
import type { ActionTrigger } from "@exp/db-schema/domain"
import type { CodingSession } from "@/db/schema"
import type { TeamAction } from "@/components/action-editor-dialog"
import { SectionLabel } from "@/components/agent-session-row"
import { Badge } from "@/components/ui/badge"
import { Switch } from "@/components/ui/switch"

// The Automations tab (EXP-530): every non-builtin action carrying a parseable
// trigger as a dense row, plus the recent automated-run history. Runs are
// `coding_sessions` rows with a non-null `started_reason` — set only by the
// device-side automation hosts, so the list is exactly "what fired by itself".

const AutomationIcon = conceptIcon(`action-automation`)

// Same sentence the action dialog's Automation section shows — one reason,
// one wording, wherever the enabled switch is locked.
const REQUIRED_INPUTS_HINT = `This action has required inputs, and an automated run has none to fill them with. Make the inputs optional to enable it.`

const SESSION_STATUS_LABELS: Record<string, string> = {
  running: `Running`,
  in_review: `In review`,
  merged: `Merged`,
  ended: `Ended`,
}

function sessionStatusLabel(status: string): string {
  return SESSION_STATUS_LABELS[status] ?? status
}

function formatNextRun(date: Date): string {
  return date.toLocaleString(undefined, {
    weekday: `short`,
    month: `short`,
    day: `numeric`,
    hour: `2-digit`,
    minute: `2-digit`,
  })
}

function AutomationRow({
  action,
  trigger,
  devices,
  isOwner,
  lastRun,
}: {
  action: TeamAction
  trigger: ActionTrigger
  devices: SteerDevice[]
  isOwner: boolean
  lastRun: CodingSession | undefined
}) {
  const RowIcon = getActionIcon(action)
  const [flipping, setFlipping] = useState(false)
  // The devices shape only syncs own + team-shared rows — a teammate's
  // private machine bound here has no row for the viewer, so the raw steer
  // id is the honest fallback label.
  const device = devices.find((d) => d.deviceId === trigger.deviceId)
  const next =
    trigger.kind === `schedule` ? nextScheduleRun(trigger, new Date()) : null
  // An automated run has nobody to type required inputs, so the server refuses
  // to ENABLE such a trigger — but a legacy row that is already on must stay
  // switchable OFF.
  const hasRequiredInputs = (action.inputs ?? []).some((def) => def.required)
  const locked = hasRequiredInputs && !trigger.enabled

  const flipEnabled = async (enabled: boolean) => {
    setFlipping(true)
    try {
      // Whole-object replace — the server re-validates the unchanged binding.
      await trpc.actions.update.mutate({
        id: action.id,
        trigger: { ...trigger, enabled },
      })
    } catch {
      // Global mutation-error toast already shown; the synced row keeps the
      // old state, so the switch snaps back on its own.
    } finally {
      setFlipping(false)
    }
  }

  return (
    <div className="flex items-center gap-3 border-b border-border/30 px-3 py-2">
      <RowIcon className="size-4 shrink-0 text-muted-foreground" />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5 text-sm">
          <span className="truncate font-medium">{action.name}</span>
        </div>
        <div className="flex min-w-0 flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
          <span className="truncate">{triggerSummary(trigger)}</span>
          <span className="flex items-center gap-1">
            <span
              aria-hidden
              className={`size-1.5 shrink-0 rounded-full ${
                device && deviceIsOnline(device)
                  ? `bg-green-500`
                  : `bg-muted-foreground/40`
              }`}
            />
            <span className="truncate">
              {device?.deviceLabel ?? trigger.deviceId}
            </span>
          </span>
          {next && <span>{`Next ${formatNextRun(next)} (device time)`}</span>}
          {lastRun && (
            <span>
              {`Last run ${sessionStatusLabel(lastRun.status)} · ${relativeTime(lastRun.createdAt)}`}
            </span>
          )}
        </div>
        {locked && (
          <p className="text-xs text-muted-foreground">
            {REQUIRED_INPUTS_HINT}
          </p>
        )}
      </div>
      <Switch
        checked={trigger.enabled}
        disabled={!isOwner || flipping || locked}
        onCheckedChange={(enabled) => void flipEnabled(enabled)}
        aria-label={`Automation enabled for ${action.name}`}
        title={locked ? REQUIRED_INPUTS_HINT : undefined}
      />
    </div>
  )
}

export function AutomationsTab({
  actions,
  devices,
  isOwner,
  teamId,
}: {
  /** The route's sorted action list (builtin included — filtered out here). */
  actions: TeamAction[] | null
  devices: SteerDevice[]
  isOwner: boolean
  teamId: string
}) {
  const automations = useMemo(() => {
    const entries: { action: TeamAction; trigger: ActionTrigger }[] = []
    for (const action of actions ?? []) {
      if (action.builtin) continue
      const trigger = parseActionTrigger(action.trigger)
      if (trigger) entries.push({ action, trigger })
    }
    return entries
  }, [actions])

  const { data: sessionRows } = useLiveQuery(
    (query) =>
      query
        .from({ sessions: codingSessionCollection })
        .where(({ sessions }) => eq(sessions.teamId, teamId)),
    [teamId]
  )
  const automatedRuns = useMemo(
    () =>
      [...((sessionRows ?? []) as CodingSession[])]
        .filter((session) => session.startedReason !== null)
        .sort(
          (a, b) =>
            new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()
        ),
    [sessionRows]
  )
  const lastRunByAction = useMemo(() => {
    const byAction = new Map<string, CodingSession>()
    // Newest-first: the first row seen per action is its latest run.
    for (const session of automatedRuns) {
      if (session.actionId && !byAction.has(session.actionId)) {
        byAction.set(session.actionId, session)
      }
    }
    return byAction
  }, [automatedRuns])

  const actionNameById = useMemo(
    () => new Map((actions ?? []).map((action) => [action.id, action.name])),
    [actions]
  )

  if (actions === null) {
    return (
      <div className="px-3 py-3 text-sm text-muted-foreground">Loading…</div>
    )
  }

  return (
    <div className="space-y-4">
      <div>
        <SectionLabel label="Automations" count={automations.length} />
        {automations.length === 0 ? (
          <div className="px-3 py-3 text-sm text-muted-foreground">
            No automations yet. Add a schedule or event trigger to an action.
          </div>
        ) : (
          automations.map(({ action, trigger }) => (
            <AutomationRow
              key={action.id}
              action={action}
              trigger={trigger}
              devices={devices}
              isOwner={isOwner}
              lastRun={lastRunByAction.get(action.id)}
            />
          ))
        )}
      </div>

      <div>
        <SectionLabel
          label="Recent automated runs"
          count={automatedRuns.length}
        />
        {automatedRuns.length === 0 ? (
          <div className="px-3 py-3 text-sm text-muted-foreground">
            Nothing has fired yet.
          </div>
        ) : (
          automatedRuns.slice(0, 10).map((session) => (
            <div
              key={session.id}
              className="flex items-center gap-2 border-b border-border/30 px-3 py-2 text-sm"
            >
              <Badge
                variant="outline"
                className="shrink-0 gap-1 text-[0.625rem]"
              >
                <AutomationIcon className="h-3 w-3" />
                Automated
              </Badge>
              <span className="min-w-0 flex-1 truncate">
                {session.actionName ??
                  (session.actionId
                    ? actionNameById.get(session.actionId)
                    : null) ??
                  `Action`}
              </span>
              <span className="shrink-0 text-xs text-muted-foreground">
                {`${sessionStatusLabel(session.status)} · ${relativeTime(session.createdAt)}`}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  )
}
