import { useEffect, useMemo, useState } from "react"
import { LoaderCircle } from "lucide-react"
import type { ActionTrigger } from "@exp/db-schema/domain"
import {
  Button,
  Dialog,
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogCancel,
} from "@exp/ui"
import { defaultDeviceId, type SteerDevice } from "@/lib/steer-devices"
import type { TriggerWrite } from "@/lib/action-trigger-writes"
import {
  TriggerLaunchFields,
  TriggerDevicePicker,
  TriggerWhenFields,
  triggerDevices,
  clampAgentFields,
  draftFromTrigger,
  draftToTrigger,
  emptyTriggerDraft,
  seedAccountPin,
  type TriggerAccountPin,
  type TriggerDraft,
} from "@/components/trigger-fields"

// The "New trigger" / "Edit trigger" form (EXP-583; SLOP-2: a trigger lives
// on its action, so there is no action to pick). A plain owner-only form: the
// when-part, the machine that runs it, and optionally the account (EXP-995:
// the agent rides the pick), model and effort. Saving replaces the action's
// whole `triggers` array; no run is started — the bound device watches its
// own synced rows and fires by itself.

/** Replace the action's triggers: `build` gets the CURRENT array — the last
 * write's result until the synced row catches up, never a stale render's —
 * and returns the whole array to store. `quiet` = the caller shows the error
 * itself. */
export type WriteTriggers = (
  build: (current: ActionTrigger[]) => TriggerWrite[],
  options?: { quiet?: boolean }
) => Promise<void>

// One reason, one wording, wherever a required input blocks a trigger.
export const REQUIRED_INPUTS_HINT = `This action has required inputs, and a triggered run has none to fill them with. Make the inputs optional to enable it.`

export function TriggerDialog({
  open,
  onOpenChange,
  teamId,
  write,
  devices,
  trigger,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  teamId: string
  /** Saves the action's whole `triggers` array (the section owns the base). */
  write: WriteTriggers
  /** The caller's machines; trigger-capable ones are pickable. */
  devices: SteerDevice[]
  /** The trigger being edited; absent/null = add a new one. */
  trigger?: ActionTrigger | null
}) {
  const editing = trigger != null

  const [draft, setDraft] = useState<TriggerDraft>(emptyTriggerDraft)
  const [deviceId, setDeviceId] = useState<string | null>(null)
  // EXP-995: ONE pin — the agent and its profile — off the account picker.
  const [pin, setPin] = useState<TriggerAccountPin>({ agent: ``, account: `` })
  const [model, setModel] = useState(``)
  const [effort, setEffort] = useState(``)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const capableDevices = useMemo(() => triggerDevices(devices), [devices])

  // Seed on OPEN only — a device connecting mid-dialog must never rewrite a
  // picked binding (the same rule every other dialog here follows).
  useEffect(() => {
    if (!open) return
    setDraft(draftFromTrigger(trigger ?? null))
    setDeviceId(
      trigger?.deviceId ??
        // EXP-622: the caller's default machine, else the first capable one.
        defaultDeviceId(capableDevices) ??
        capableDevices[0]?.deviceId ??
        null
    )
    setPin({ agent: trigger?.agent ?? ``, account: trigger?.account ?? `` })
    setModel(trigger?.model ?? ``)
    setEffort(trigger?.effort ?? ``)
    setSubmitting(false)
    setError(null)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  const device = capableDevices.find(
    (candidate) => candidate.deviceId === deviceId
  )
  // EXP-615/995: no "Device default" pill — whenever the bound machine
  // changes (a pick, or its rows refreshing) the pin re-seeds to a login THAT
  // machine reports: the same agent's last used login when it runs it, else
  // the machine's LAST USED login, which names the agent, exactly like the
  // composer. A pin the machine reports as-is is left alone, so what the
  // Account row shows is what Save stores (profile ids are device-local).
  useEffect(() => {
    if (!open) return
    const next = seedAccountPin(device, pin)
    if (!next) return
    switchPin(next)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, device])

  // A model/effort belongs to ONE agent — another login of the same agent
  // keeps them, a different agent re-clamps them.
  const switchPin = (next: TriggerAccountPin) => {
    setPin(next)
    const clamped = clampAgentFields(next.agent, model, effort)
    setModel(clamped.model)
    setEffort(clamped.effort)
  }

  const canSubmit = Boolean(deviceId) && !submitting

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!canSubmit || !deviceId) return
    setSubmitting(true)
    setError(null)
    const when = draftToTrigger(draft)
    const written: TriggerWrite = {
      ...(editing ? { id: trigger.id } : {}),
      enabled: trigger?.enabled ?? true,
      deviceId,
      ...(pin.agent === `` ? {} : { agent: pin.agent }),
      ...(pin.account === `` ? {} : { account: pin.account }),
      ...(model === `` ? {} : { model }),
      ...(effort === `` ? {} : { effort }),
      ...(when.kind === `event` ? { ...when, source: `exponential` } : when),
    }
    try {
      await write(
        (current) =>
          editing
            ? current.map((existing) =>
                existing.id === trigger.id ? written : existing
              )
            : [...current, written],
        { quiet: true }
      )
      onOpenChange(false)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/* EXP-616: a bottom sheet on mobile — the body is an ordinary
          DialogBody, so it takes the fixed 94dvh detent's free height and
          scrolls inside it, content anchored to the top. */}
      <DialogContent mobile="sheet-full" className="sm:max-h-[85dvh] sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{editing ? `Edit trigger` : `New trigger`}</DialogTitle>
        </DialogHeader>

        <form onSubmit={submit} className="flex min-h-0 flex-1 flex-col gap-3">
          <DialogBody className="space-y-3">
            <TriggerWhenFields draft={draft} onChange={setDraft} teamId={teamId} />

            <TriggerDevicePicker
              deviceId={deviceId}
              devices={capableDevices}
              onChange={setDeviceId}
            />

            <TriggerLaunchFields
              device={device}
              pin={pin}
              onPinChange={switchPin}
              model={model}
              onModelChange={setModel}
              effort={effort}
              onEffortChange={setEffort}
            />

            {error && (
              <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                {error}
              </div>
            )}
          </DialogBody>

          <DialogFooter>
            <DialogCancel disabled={submitting} />
            <Button type="submit" disabled={!canSubmit}>
              {submitting && <LoaderCircle className="animate-spin" />}
              {editing ? `Save changes` : `Add trigger`}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
