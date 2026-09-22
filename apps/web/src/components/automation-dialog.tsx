import { useEffect, useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { LoaderCircle } from "lucide-react"
import type { AutomationTrigger } from "@exp/db-schema/domain"
import type { Automation, SyncedAction } from "@/db/schema"
import { actionCollection, automationCollection } from "@/lib/collections"
import { isBuiltinActionId } from "@/lib/builtin-actions"
import { parseAutomationTrigger } from "@/lib/action-triggers"
import {
  getActionIcon,
  Button,
  Dialog,
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogCancel,
  Combobox,
  GlassGroup,
} from "@exp/ui"
import { defaultDeviceId, type SteerDevice } from "@/lib/steer-devices"
import { trpc } from "@/lib/trpc-client"
import {
  AutomationLaunchFields,
  AutomationDevicePicker,
  AutomationTriggerFields,
  automationDevices,
  clampAgentFields,
  draftFromTrigger,
  draftToTrigger,
  emptyAutomationDraft,
  seedAccountPin,
  type AutomationAccountPin,
  type AutomationDraft,
} from "@/components/automation-section"

// The "New automation" / "Edit automation" form (EXP-583). Automations are
// their own rows now, so this is a plain owner-only tRPC form: pick the
// action, the when-part, the machine that runs it, and optionally pin the
// account (EXP-995: the agent rides the pick)/model/effort. No action editing
// happens here, and no run is started — the bound device watches its own
// synced rows and fires by itself.

// Same sentence the Automations tab shows on a locked row — one reason, one
// wording, wherever a required input blocks automating an action.
export const REQUIRED_INPUTS_HINT = `This action has required inputs, and an automated run has none to fill them with. Make the inputs optional to enable it.`

function hasRequiredInputs(action: Pick<SyncedAction, `inputs`>): boolean {
  return (action.inputs ?? []).some((def) => def.required)
}

export function AutomationDialog({
  open,
  onOpenChange,
  teamId,
  devices,
  automation,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  teamId: string
  /** The caller's machines; automation-capable ones are pickable. */
  devices: SteerDevice[]
  /** The row being edited; absent/null = create a new automation. */
  automation?: Automation | null
}) {
  const editing = automation != null

  const [actionId, setActionId] = useState(``)
  const [draft, setDraft] = useState<AutomationDraft>(emptyAutomationDraft)
  const [deviceId, setDeviceId] = useState<string | null>(null)
  // EXP-995: ONE pin — the agent and its profile — off the account picker.
  const [pin, setPin] = useState<AutomationAccountPin>({ agent: ``, account: `` })
  const [model, setModel] = useState(``)
  const [effort, setEffort] = useState(``)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Custom actions only — builtins are server-shipped prompts with required
  // inputs and no team row to target.
  const { data: actionRows } = useLiveQuery(
    (query) =>
      query.from({ a: actionCollection }).where(({ a }) => eq(a.teamId, teamId)),
    [teamId]
  )
  const actionOptions = useMemo(
    () =>
      [...((actionRows ?? []) as SyncedAction[])]
        .filter((action) => !isBuiltinActionId(action.id))
        .sort((a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name)),
    [actionRows]
  )

  const capableDevices = useMemo(() => automationDevices(devices), [devices])

  // Seed on OPEN only — a device connecting mid-dialog must never rewrite a
  // picked binding (the same rule every other dialog here follows).
  useEffect(() => {
    if (!open) return
    setActionId(automation?.actionId ?? ``)
    setDraft(
      draftFromTrigger(
        automation ? parseAutomationTrigger(automation.trigger) : null
      )
    )
    setDeviceId(
      automation?.deviceId ??
        // EXP-622: the caller's default machine, else the first capable one.
        defaultDeviceId(capableDevices) ??
        capableDevices[0]?.deviceId ??
        null
    )
    setPin({ agent: automation?.agent ?? ``, account: automation?.account ?? `` })
    setModel(automation?.model ?? ``)
    setEffort(automation?.effort ?? ``)
    setSubmitting(false)
    setError(null)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  const device = capableDevices.find(
    (candidate) => candidate.deviceId === deviceId
  )
  // EXP-615/995: no "Device default" pill — a bound machine that cannot run
  // the pinned agent (or none pinned yet) seeds the pin to that machine's
  // DEFAULT ACCOUNT, which names the agent, exactly like the composer.
  useEffect(() => {
    if (!open) return
    const next = seedAccountPin(device, pin)
    if (!next) return
    switchPin(next)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, device])
  const selectedAction = actionOptions.find((action) => action.id === actionId)
  const blockedByInputs = selectedAction
    ? hasRequiredInputs(selectedAction)
    : false

  // A model/effort belongs to ONE agent — another login of the same agent
  // keeps them, a different agent re-clamps them.
  const switchPin = (next: AutomationAccountPin) => {
    setPin(next)
    const clamped = clampAgentFields(next.agent, model, effort)
    setModel(clamped.model)
    setEffort(clamped.effort)
  }

  const canSubmit =
    Boolean(actionId) && Boolean(deviceId) && !blockedByInputs && !submitting

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!canSubmit || !deviceId) return
    setSubmitting(true)
    setError(null)
    const trigger: AutomationTrigger = draftToTrigger(draft)
    try {
      const { txId } = editing
        ? await trpc.automations.update.mutate(
            {
              id: automation.id,
              actionId,
              deviceId,
              trigger,
              agent: pin.agent === `` ? null : pin.agent,
              account: pin.account === `` ? null : pin.account,
              model: model === `` ? null : model,
              effort: effort === `` ? null : effort,
            },
            { context: { skipErrorToast: true } }
          )
        : await trpc.automations.create.mutate(
            {
              teamId,
              actionId,
              deviceId,
              trigger,
              agent: pin.agent === `` ? null : pin.agent,
              account: pin.account === `` ? null : pin.account,
              model: model === `` ? null : model,
              effort: effort === `` ? null : effort,
            },
            { context: { skipErrorToast: true } }
          )
      await automationCollection.utils.awaitTxId(txId)
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
          <DialogTitle>
            {editing ? `Edit automation` : `New automation`}
          </DialogTitle>
        </DialogHeader>

        <form onSubmit={submit} className="flex min-h-0 flex-1 flex-col gap-3">
          <DialogBody className="space-y-3">
            <div className="space-y-2">
              {/* EXP-616: the grouped-form row — "Action" leads, the picked
                  action trails. */}
              <GlassGroup>
                <Combobox
                  triggerVariant="row"
                  searchable={false}
                  mobileTitle="Action"
                  value={actionId === `` ? null : actionId}
                  onChange={(value) => {
                    if (value !== null) setActionId(value)
                  }}
                  triggerLabel="Select an action"
                  options={actionOptions.map((action) => {
                    const ActionIcon = getActionIcon(action)
                    return {
                      value: action.id,
                      disabled: hasRequiredInputs(action),
                      // EXP-697: the icon must flow INLINE with the name —
                      // preflight makes svg display:block, which pushed the
                      // name onto a second line inside the picker's
                      // truncating value span.
                      label: (
                        <>
                          <ActionIcon className="mr-1.5 inline size-4 align-text-bottom" />
                          {action.name}
                        </>
                      ),
                    }
                  })}
                />
              </GlassGroup>
              {actionOptions.length === 0 && (
                <p className="text-xs text-muted-foreground">
                  No custom actions yet. Create one first, then automate it.
                </p>
              )}
              {blockedByInputs && (
                <p className="text-xs text-muted-foreground">
                  {REQUIRED_INPUTS_HINT}
                </p>
              )}
            </div>

            <AutomationTriggerFields
              draft={draft}
              onChange={setDraft}
              teamId={teamId}
            />

            <AutomationDevicePicker
              deviceId={deviceId}
              devices={capableDevices}
              onChange={setDeviceId}
            />

            <AutomationLaunchFields
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
              {editing ? `Save changes` : `Create automation`}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
