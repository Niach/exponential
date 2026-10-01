import { useCallback, useMemo, useRef, useState } from "react"
import { Ellipsis, LoaderCircle, Pencil, Trash2 } from "lucide-react"
import type { ActionTrigger } from "@exp/db-schema/domain"
import {
  AGENT_LABELS,
  conceptIcon,
  Button,
  Pill,
  Dialog,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  GlassSectionHeader,
  ListRow,
  LiveDot,
  Switch,
  toast,
} from "@exp/ui"
import { parseActionTriggers, triggerSummary } from "@/lib/action-triggers"
import { actionCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { deviceIsOnline, type SteerDevice } from "@/lib/steer-devices"
import type { TeamAction } from "@/components/action-prompt-form"
import {
  REQUIRED_INPUTS_HINT,
  TriggerDialog,
  type WriteTriggers,
} from "@/components/trigger-dialog"

// An action's triggers (SLOP-2; the Automations tab of EXP-530/583 folded
// into the action): every schedule and event watcher on it as a dense row,
// joined client-side with the device it runs on. What fired is the action's
// Runs, so a row carries no run history of its own.

const TriggerScheduleIcon = conceptIcon(`trigger-schedule`)
const TriggerEventIcon = conceptIcon(`trigger-event`)

/** The glyph of a trigger kind — the Triggers rows and the Actions list. */
export function TriggerGlyph({
  kind,
  className,
}: {
  kind: ActionTrigger[`kind`]
  className?: string
}) {
  const Icon = kind === `schedule` ? TriggerScheduleIcon : TriggerEventIcon
  return <Icon className={className} />
}

export const UNREADABLE_TRIGGER_MESSAGE = `This action has a trigger this version cannot edit`

/** An action's `triggers` as one read of it: the synced row, or what the
 * last `actions.update` returned. */
export interface TriggersSnapshot {
  actionId: string
  triggers: unknown
  updatedAt: Date | string
}

/** The stored array the NEXT write builds on. A write replaces the WHOLE
 * array, so building on a synced row that has not echoed the previous write
 * yet would silently revert it: the last mutation's result stays the base
 * until the synced row is at least as new. */
export function triggerWriteBase(
  synced: TriggersSnapshot,
  last: TriggersSnapshot | null
): unknown {
  if (!last || last.actionId !== synced.actionId) return synced.triggers
  const newer =
    new Date(last.updatedAt).getTime() > new Date(synced.updatedAt).getTime()
  return newer ? last.triggers : synced.triggers
}

/** The readable triggers of a stored array, and whether it holds entries
 * this build cannot read (a kind it does not know) — writing the readable
 * ones back would delete those. */
export function readTriggerBase(stored: unknown): {
  triggers: ActionTrigger[]
  unreadable: boolean
} {
  const triggers = parseActionTriggers(stored)
  return {
    triggers,
    unreadable: Array.isArray(stored) && stored.length > triggers.length,
  }
}

function syncedSnapshot(action: TeamAction): TriggersSnapshot {
  return {
    actionId: action.id,
    triggers: action.builtin ? [] : action.triggers,
    updatedAt: action.updatedAt,
  }
}

/** The section's ONE writer: writes run one after another, each built on
 * `triggerWriteBase` as it stands when its turn comes. */
function useTriggerWrites(action: TeamAction): {
  /** What the rows show: the base the next write builds on. */
  triggers: ActionTrigger[]
  pending: boolean
  write: WriteTriggers
} {
  const [last, setLast] = useState<TriggersSnapshot | null>(null)
  const [pending, setPending] = useState(0)
  const latest = useRef({ action, last })
  latest.current.action = action
  const queue = useRef<Promise<void>>(Promise.resolve())

  const write = useCallback<WriteTriggers>((build, options) => {
    const run = async () => {
      const current = latest.current
      const base = readTriggerBase(
        triggerWriteBase(syncedSnapshot(current.action), current.last)
      )
      if (base.unreadable) {
        if (!options?.quiet) toast.error(UNREADABLE_TRIGGER_MESSAGE)
        throw new Error(UNREADABLE_TRIGGER_MESSAGE)
      }
      const actionId = current.action.id
      const result = await trpc.actions.update.mutate(
        { id: actionId, triggers: build(base.triggers) },
        options?.quiet ? { context: { skipErrorToast: true } } : undefined
      )
      const written: TriggersSnapshot = {
        actionId,
        triggers: result.action.triggers,
        updatedAt: result.action.updatedAt,
      }
      latest.current.last = written
      setLast(written)
      if (`txId` in result && result.txId !== undefined) {
        await actionCollection.utils.awaitTxId(result.txId)
      }
    }
    setPending((count) => count + 1)
    const done = queue.current.then(run)
    queue.current = done.catch(() => {})
    return done.finally(() => setPending((count) => count - 1))
  }, [])

  const stored = triggerWriteBase(syncedSnapshot(action), last)
  const triggers = useMemo(() => parseActionTriggers(stored), [stored])
  return { triggers, pending: pending > 0, write }
}

// Owner-only ⋯ menu on a row. `onEdit` is absent when this build has no
// trigger editor to open (steer off), and the item goes with it.
function TriggerMenu({
  onEdit,
  onDelete,
}: {
  onEdit: (() => void) | undefined
  onDelete: () => void
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon-sm" aria-label="Trigger menu">
          <Ellipsis />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {onEdit && (
          <DropdownMenuItem onClick={onEdit}>
            <Pencil className="h-4 w-4" />
            Edit
          </DropdownMenuItem>
        )}
        <DropdownMenuItem variant="destructive" onClick={onDelete}>
          <Trash2 className="h-4 w-4" />
          Delete
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function TriggerRow({
  trigger,
  devices,
  isOwner,
  canEdit,
  blockedByInputs,
  writing,
  onToggle,
  onEdit,
  onDelete,
}: {
  trigger: ActionTrigger
  devices: SteerDevice[]
  isOwner: boolean
  /** Whether the editor this row opens is rendered at all. */
  canEdit: boolean
  blockedByInputs: boolean
  /** A trigger write is in flight: the next one waits for it. */
  writing: boolean
  onToggle: (enabled: boolean) => Promise<void>
  onEdit: () => void
  onDelete: () => void
}) {
  const [flipping, setFlipping] = useState(false)
  // The devices shape only syncs own + team-shared rows — a teammate's
  // private machine bound here has no row for the viewer, so the raw steer
  // id is the honest fallback label.
  const device = devices.find((d) => d.deviceId === trigger.deviceId)
  // A triggered run has nobody to fill required inputs, so the server refuses
  // to ENABLE such a trigger — but one that is already on must stay
  // switchable OFF.
  const locked = blockedByInputs && !trigger.enabled
  const launch = [
    trigger.agent ? (AGENT_LABELS[trigger.agent] ?? trigger.agent) : null,
    trigger.model,
  ]
    .filter(Boolean)
    .join(` · `)
  // A schedule runs on the BOUND MACHINE's wall clock, so the recurrence
  // carries that caveat (EXP-812).
  const sentence =
    trigger.kind === `schedule`
      ? `${triggerSummary(trigger)} (device time)`
      : triggerSummary(trigger)

  const flip = async (enabled: boolean) => {
    setFlipping(true)
    try {
      await onToggle(enabled)
    } catch {
      // Global mutation-error toast already shown; the synced row keeps the
      // old state, so the switch snaps back on its own.
    } finally {
      setFlipping(false)
    }
  }

  return (
    <ListRow
      interactive={canEdit}
      onClick={canEdit ? onEdit : undefined}
      // A clickable row is a button to assistive tech; its own name keeps
      // the "..." menu's label from being folded into the row's.
      aria-label={canEdit ? sentence : undefined}
      data-testid={`trigger-${trigger.id}`}
    >
      <TriggerGlyph
        kind={trigger.kind}
        className="size-4 shrink-0 text-foreground/70"
      />
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-medium">{sentence}</div>
        <div className="flex min-w-0 flex-wrap items-center gap-x-1.5 text-xs text-muted-foreground">
          <span className="flex items-center gap-1">
            <LiveDot
              tone={device && deviceIsOnline(device) ? `live` : `idle`}
              className="size-1.5 shrink-0"
            />
            <span className="truncate">
              {device?.deviceLabel || trigger.deviceId}
            </span>
          </span>
          {launch && <span className="truncate">{`· ${launch}`}</span>}
        </div>
        {locked && (
          <p className="text-xs text-muted-foreground">{REQUIRED_INPUTS_HINT}</p>
        )}
      </div>
      {/* EXP-698: the fixed trailing column — toggle then ⋯, centred on the
          row. A non-owner has no menu, so the slot stays as an empty spacer
          and the toggles still line up down the list. */}
      <div
        className="flex shrink-0 items-center gap-1 self-center"
        // The toggle and the menu are their own targets; the row's click must
        // not fire underneath them.
        onClick={(e) => e.stopPropagation()}
      >
        <Switch
          checked={trigger.enabled}
          disabled={!isOwner || flipping || writing || locked}
          onCheckedChange={(enabled) => void flip(enabled)}
          aria-label={`Enabled: ${sentence}`}
          title={locked ? REQUIRED_INPUTS_HINT : undefined}
        />
        {isOwner ? (
          <TriggerMenu onEdit={canEdit ? onEdit : undefined} onDelete={onDelete} />
        ) : (
          <span aria-hidden className="size-8 shrink-0" />
        )}
      </div>
    </ListRow>
  )
}

export function ActionTriggersSection({
  action,
  devices,
  isOwner,
  steerEnabled,
  showHeader = true,
}: {
  action: TeamAction
  devices: SteerDevice[]
  isOwner: boolean
  /** Same gate as the Actions list's "New action" button. */
  steerEnabled: boolean
  /** Off on a phone, where the tab already says "Triggers". */
  showHeader?: boolean
}) {
  // The dialog below is the ONE trigger editor — add and edit both open it —
  // so every path into it shares its gate.
  const canEdit = steerEnabled && isOwner
  const blockedByInputs = (action.inputs ?? []).some((def) => def.required)

  const [dialogOpen, setDialogOpen] = useState(false)
  const [editing, setEditing] = useState<ActionTrigger | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<ActionTrigger | null>(null)
  const [deleting, setDeleting] = useState(false)
  // The readable triggers, in stored order.
  const { triggers, pending: writing, write } = useTriggerWrites(action)

  const confirmDelete = async () => {
    if (!deleteTarget) return
    setDeleting(true)
    try {
      const targetId = deleteTarget.id
      await write((current) =>
        current.filter((trigger) => trigger.id !== targetId)
      )
      setDeleteTarget(null)
    } catch {
      // Toast already shown; keep the confirm open for a retry.
    } finally {
      setDeleting(false)
    }
  }

  const addButton = canEdit ? (
    <Pill
      mode="action"
      onClick={() => {
        setEditing(null)
        setDialogOpen(true)
      }}
    >
      Add trigger
    </Pill>
  ) : undefined

  return (
    <>
      <div>
        {showHeader ? (
          <GlassSectionHeader label="Triggers" trailing={addButton} />
        ) : (
          addButton && <div className="flex justify-end pb-2">{addButton}</div>
        )}
        {triggers.length === 0 ? (
          <div className="px-1 py-3 text-sm text-muted-foreground">
            No triggers. This action runs when someone starts it.
          </div>
        ) : (
          <div className="flex flex-col gap-0">
            {triggers.map((trigger) => (
              <TriggerRow
                key={trigger.id}
                trigger={trigger}
                devices={devices}
                isOwner={isOwner}
                canEdit={canEdit}
                blockedByInputs={blockedByInputs}
                writing={writing}
                onToggle={(enabled) =>
                  write((current) =>
                    current.map((existing) =>
                      existing.id === trigger.id
                        ? { ...existing, enabled }
                        : existing
                    )
                  )
                }
                onEdit={() => {
                  setEditing(trigger)
                  setDialogOpen(true)
                }}
                onDelete={() => setDeleteTarget(trigger)}
              />
            ))}
          </div>
        )}
      </div>

      {canEdit && (
        <TriggerDialog
          open={dialogOpen}
          onOpenChange={(next) => {
            setDialogOpen(next)
            if (!next) setEditing(null)
          }}
          teamId={action.teamId}
          write={write}
          devices={devices}
          trigger={editing}
        />
      )}

      <Dialog
        open={deleteTarget !== null}
        onOpenChange={(next) => {
          if (!next && !deleting) setDeleteTarget(null)
        }}
      >
        <DialogContent mobile="alert" className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Delete trigger?</DialogTitle>
            <DialogDescription>
              It stops firing. Past runs stay in Runs.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel
              onClick={() => setDeleteTarget(null)}
              disabled={deleting}
            />
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
