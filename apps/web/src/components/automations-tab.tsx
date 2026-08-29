import { useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { Ellipsis, LoaderCircle, Pencil, Trash2 } from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
import {
  nextScheduleRun,
  parseAutomationTrigger,
  triggerSummary,
} from "@/lib/action-triggers"
import {
  deviceCanResumeRun,
  deviceIsOnline,
  type SteerDevice,
} from "@/lib/steer-devices"
import { EndedSessionRow } from "@/components/agent-session-row"
import { SuggestionsButton } from "@/components/getting-started/getting-started-sheet"
import { useAgentDock } from "@/components/agent-dock/agent-dock-provider"
import { getActionIcon } from "@/lib/board-icons"
import {
  automationCollection,
  codingSessionCollection,
} from "@/lib/collections"
import { relativeTime } from "@/components/comment-rows/format"
import { trpc } from "@/lib/trpc-client"
import type { Automation, CodingSession } from "@/db/schema"
import type { TeamAction } from "@/components/action-editor-dialog"
import {
  AutomationDialog,
  REQUIRED_INPUTS_HINT,
} from "@/components/automation-dialog"
import { AGENT_LABELS } from "@/components/launch-dialog/launch-options-pane"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { GlassRow, GlassSectionHeader } from "@/components/ui/glass-rows"
import { Switch } from "@/components/ui/switch"

// The Automations tab (EXP-530; own rows since EXP-583): every synced
// `automations` row as a dense row, joined client-side with its action, the
// bound device and its last run, plus the recent automated-run history. Runs
// are `coding_sessions` rows with a non-null `started_reason` — set only by
// the device-side automation hosts, so the list is exactly "what fired by
// itself".

// EXP-615: `action-create` is the sparkle of the agent-authored "New action"
// flow — a New-automation button is the automation concept (desktop's
// New-automation button already draws it).
const AutomationCreateIcon = conceptIcon(`action-automation`)

const SESSION_STATUS_LABELS: Record<string, string> = {
  running: `Running`,
  in_review: `In review`,
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

// Owner-only ⋯ menu on a row.
function AutomationMenu({
  name,
  onEdit,
  onDelete,
}: {
  name: string
  onEdit: () => void
  onDelete: () => void
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7 text-muted-foreground"
          aria-label={`Automation menu for ${name}`}
        >
          <Ellipsis className="h-4 w-4" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onClick={onEdit}>
          <Pencil className="h-4 w-4" />
          Edit
        </DropdownMenuItem>
        <DropdownMenuItem variant="destructive" onClick={onDelete}>
          <Trash2 className="h-4 w-4" />
          Delete
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function AutomationRow({
  automation,
  action,
  devices,
  isOwner,
  lastRun,
  onEdit,
  onDelete,
}: {
  automation: Automation
  action: TeamAction | undefined
  devices: SteerDevice[]
  isOwner: boolean
  lastRun: CodingSession | undefined
  onEdit: () => void
  onDelete: () => void
}) {
  const RowIcon = getActionIcon(action ?? {})
  const [flipping, setFlipping] = useState(false)
  const trigger = parseAutomationTrigger(automation.trigger)
  // The devices shape only syncs own + team-shared rows — a teammate's
  // private machine bound here has no row for the viewer, so the raw steer
  // id is the honest fallback label.
  const device = devices.find((d) => d.deviceId === automation.deviceId)
  const next =
    trigger?.kind === `schedule` ? nextScheduleRun(trigger, new Date()) : null
  // An automated run has nobody to type required inputs, so the server refuses
  // to ENABLE such a row — but one that is already on must stay switchable OFF.
  const blockedByInputs = (action?.inputs ?? []).some((def) => def.required)
  const locked = blockedByInputs && !automation.enabled
  const launch = [
    automation.agent ? (AGENT_LABELS[automation.agent] ?? automation.agent) : null,
    automation.model,
  ]
    .filter(Boolean)
    .join(` · `)

  const flipEnabled = async (enabled: boolean) => {
    setFlipping(true)
    try {
      const { txid } = await trpc.automations.update.mutate({
        id: automation.id,
        enabled,
      })
      await automationCollection.utils.awaitTxId(txid)
    } catch {
      // Global mutation-error toast already shown; the synced row keeps the
      // old state, so the switch snaps back on its own.
    } finally {
      setFlipping(false)
    }
  }

  return (
    <GlassRow>
      <RowIcon className="size-4 shrink-0 text-foreground/70" />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5 text-sm">
          <span className="truncate font-medium">
            {action?.name ?? `Action`}
          </span>
        </div>
        <div className="flex min-w-0 flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
          <span className="truncate">
            {trigger ? triggerSummary(trigger) : `Unsupported trigger`}
          </span>
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
              {device?.deviceLabel || automation.deviceId}
            </span>
          </span>
          {launch && <span className="truncate">{launch}</span>}
          {next && <span>{`Next ${formatNextRun(next)} (device time)`}</span>}
          {lastRun && (
            <span>
              {`Last run ${sessionStatusLabel(lastRun.status)} · ${relativeTime(lastRun.createdAt)}`}
            </span>
          )}
        </div>
        {locked && (
          <p className="text-xs text-muted-foreground">{REQUIRED_INPUTS_HINT}</p>
        )}
      </div>
      <Switch
        checked={automation.enabled}
        disabled={!isOwner || flipping || locked}
        onCheckedChange={(enabled) => void flipEnabled(enabled)}
        aria-label={`Automation enabled for ${action?.name ?? `action`}`}
        title={locked ? REQUIRED_INPUTS_HINT : undefined}
      />
      {isOwner && (
        <AutomationMenu
          name={action?.name ?? `action`}
          onEdit={onEdit}
          onDelete={onDelete}
        />
      )}
    </GlassRow>
  )
}

export function AutomationsTab({
  actions,
  devices,
  isOwner,
  steerEnabled,
  teamId,
  showSuggestions = false,
}: {
  /** The lookup pool for run/automation names — the builtins included, so a
   * fix-conflicts run is named even though it is not listed (EXP-686). */
  actions: TeamAction[] | null
  devices: SteerDevice[]
  isOwner: boolean
  /** Same gate as the Actions tab's "New action" button. */
  steerEnabled: boolean
  teamId: string
  /** EXP-686: the desktop-viewport `/automations` page carries the lightbulb
   * to Getting started's suggestions; the mobile tabs have their own tab. */
  showSuggestions?: boolean
}) {
  const dock = useAgentDock()
  const { data: automationRows } = useLiveQuery(
    (query) =>
      query
        .from({ automations: automationCollection })
        .where(({ automations }) => eq(automations.teamId, teamId)),
    [teamId]
  )
  const automations = useMemo(
    () =>
      [...((automationRows ?? []) as Automation[])].sort(
        (a, b) =>
          a.sortOrder - b.sortOrder ||
          new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime()
      ),
    [automationRows]
  )

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
  const lastRunByAutomation = useMemo(() => {
    const byAutomation = new Map<string, CodingSession>()
    // Newest-first: the first row seen per automation is its latest run.
    for (const session of automatedRuns) {
      if (session.automationId && !byAutomation.has(session.automationId)) {
        byAutomation.set(session.automationId, session)
      }
    }
    return byAutomation
  }, [automatedRuns])

  const actionById = useMemo(
    () => new Map((actions ?? []).map((action) => [action.id, action])),
    [actions]
  )

  // EXP-637: Resume relaunches the run on the machine that still holds its
  // worktree — hide the button when that machine is offline or too old to
  // resume, rather than failing after the click.
  const resumableDeviceIds = useMemo(
    () =>
      new Set(
        devices
          .filter(
            (device) => deviceIsOnline(device) && deviceCanResumeRun(device)
          )
          .map((device) => device.deviceId)
      ),
    [devices]
  )

  const [dialogOpen, setDialogOpen] = useState(false)
  const [editing, setEditing] = useState<Automation | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<Automation | null>(null)
  const [deleting, setDeleting] = useState(false)

  const confirmDelete = async () => {
    if (!deleteTarget) return
    setDeleting(true)
    try {
      const { txid } = await trpc.automations.delete.mutate({
        id: deleteTarget.id,
      })
      await automationCollection.utils.awaitTxId(txid)
      setDeleteTarget(null)
    } catch {
      // Toast already shown; keep the confirm open for a retry.
    } finally {
      setDeleting(false)
    }
  }

  const canCreate = steerEnabled && isOwner
  const headerTrailing = !showSuggestions && !canCreate ? undefined : (
    <>
      {showSuggestions && <SuggestionsButton />}
      {canCreate && (
        <Button
          variant="outline"
          size="sm"
          className="h-6 gap-1 px-2 text-xs"
          onClick={() => {
            setEditing(null)
            setDialogOpen(true)
          }}
        >
          <AutomationCreateIcon className="size-3.5" />
          New automation
        </Button>
      )}
    </>
  )

  if (actions === null) {
    return (
      <div className="px-1 py-3 text-sm text-muted-foreground">Loading…</div>
    )
  }

  return (
    <>
      <div className="space-y-6">
        <div>
          <GlassSectionHeader
            label="Automations"
            count={automations.length}
            trailing={headerTrailing}
          />
          {automations.length === 0 ? (
            <div className="px-1 py-3 text-sm text-muted-foreground">
              No automations yet.
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              {automations.map((automation) => (
                <AutomationRow
                  key={automation.id}
                  automation={automation}
                  action={actionById.get(automation.actionId)}
                  devices={devices}
                  isOwner={isOwner}
                  lastRun={lastRunByAutomation.get(automation.id)}
                  onEdit={() => {
                    setEditing(automation)
                    setDialogOpen(true)
                  }}
                  onDelete={() => setDeleteTarget(automation)}
                />
              ))}
            </div>
          )}
        </div>

        <div>
          <GlassSectionHeader
            label="Recent automated runs"
            count={automatedRuns.length}
          />
          {automatedRuns.length === 0 ? (
            <div className="px-1 py-3 text-sm text-muted-foreground">
              Nothing has fired yet.
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              {automatedRuns.slice(0, 10).map((session) =>
                // EXP-637: an ENDED run has a close-out to expand into.
                // EXP-686: a live one has no self-reported state left to
                // show — it just says "Running", and the whole row opens the
                // session in the dock.
                session.status === `ended` ? (
                  <EndedSessionRow
                    key={session.id}
                    row={{
                      session,
                      canResume: Boolean(
                        session.deviceId &&
                          resumableDeviceIds.has(session.deviceId)
                      ),
                    }}
                    title={
                      session.actionName ??
                      (session.actionId
                        ? actionById.get(session.actionId)?.name
                        : null) ??
                      `Action`
                    }
                  />
                ) : (
                  <GlassRow
                    key={session.id}
                    interactive
                    className="gap-2 text-sm"
                    onClick={() => dock?.openDock(session.id)}
                    data-testid={`automated-run-${session.id}`}
                  >
                    <span className="min-w-0 flex-1 truncate">
                      {session.actionName ??
                        (session.actionId
                          ? actionById.get(session.actionId)?.name
                          : null) ??
                        `Action`}
                    </span>
                    <span className="shrink-0 text-xs text-muted-foreground">
                      {`Running · ${relativeTime(session.createdAt)}`}
                    </span>
                  </GlassRow>
                )
              )}
            </div>
          )}
        </div>
      </div>

      {canCreate && (
        <AutomationDialog
          open={dialogOpen}
          onOpenChange={(next) => {
            setDialogOpen(next)
            if (!next) setEditing(null)
          }}
          teamId={teamId}
          devices={devices}
          automation={editing}
        />
      )}

      <Dialog
        open={deleteTarget !== null}
        onOpenChange={(next) => {
          if (!next && !deleting) setDeleteTarget(null)
        }}
      >
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Delete automation</DialogTitle>
            <DialogDescription>
              {`Stop automating "${
                deleteTarget
                  ? (actionById.get(deleteTarget.actionId)?.name ?? `this action`)
                  : ``
              }"? The action itself stays, and runs already going keep going.`}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="ghost"
              onClick={() => setDeleteTarget(null)}
              disabled={deleting}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={() => void confirmDelete()}
              disabled={deleting}
            >
              {deleting ? <LoaderCircle className="animate-spin" /> : <Trash2 />}
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
