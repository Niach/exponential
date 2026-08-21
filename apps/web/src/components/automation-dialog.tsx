import { useEffect, useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { LoaderCircle } from "lucide-react"
import type { AutomationTrigger } from "@exp/db-schema/domain"
import type { Automation, SyncedAction } from "@/db/schema"
import { actionCollection, automationCollection } from "@/lib/collections"
import { isBuiltinActionId } from "@/lib/builtin-actions"
import { parseAutomationTrigger } from "@/lib/action-triggers"
import { getActionIcon } from "@/lib/board-icons"
import type { SteerDevice } from "@/lib/steer-devices"
import { trpc } from "@/lib/trpc-client"
import {
  AutomationAgentFields,
  AutomationDevicePicker,
  AutomationTriggerFields,
  automationDevices,
  clampAgentFields,
  draftFromTrigger,
  draftToTrigger,
  emptyAutomationDraft,
  type AutomationDraft,
} from "@/components/automation-section"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

// The "New automation" / "Edit automation" form (EXP-583). Automations are
// their own rows now, so this is a plain owner-only tRPC form: pick the
// action, the when-part, the machine that runs it, and optionally pin the
// agent/model/effort. No action editing happens here, and no run is started —
// the bound device watches its own synced rows and fires by itself.

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
  const [agent, setAgent] = useState(``)
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
    setDeviceId(automation?.deviceId ?? capableDevices[0]?.deviceId ?? null)
    setAgent(automation?.agent ?? ``)
    setModel(automation?.model ?? ``)
    setEffort(automation?.effort ?? ``)
    setSubmitting(false)
    setError(null)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  const device = capableDevices.find(
    (candidate) => candidate.deviceId === deviceId
  )
  const selectedAction = actionOptions.find((action) => action.id === actionId)
  const blockedByInputs = selectedAction
    ? hasRequiredInputs(selectedAction)
    : false

  const switchAgent = (next: string) => {
    setAgent(next)
    const clamped = clampAgentFields(next, model, effort)
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
      const { txid } = editing
        ? await trpc.automations.update.mutate(
            {
              id: automation.id,
              actionId,
              deviceId,
              trigger,
              agent: agent === `` ? null : agent,
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
              agent: agent === `` ? null : agent,
              model: model === `` ? null : model,
              effort: effort === `` ? null : effort,
            },
            { context: { skipErrorToast: true } }
          )
      await automationCollection.utils.awaitTxId(txid)
      onOpenChange(false)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-h-[85dvh] sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {editing ? `Edit automation` : `New automation`}
          </DialogTitle>
        </DialogHeader>

        <form onSubmit={submit} className="flex min-h-0 flex-1 flex-col gap-4">
          <DialogBody className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="automation-action">Action</Label>
              <Select value={actionId} onValueChange={setActionId}>
                <SelectTrigger id="automation-action" className="w-full">
                  <SelectValue placeholder="Select an action" />
                </SelectTrigger>
                <SelectContent>
                  {actionOptions.map((action) => {
                    const ActionIcon = getActionIcon(action)
                    return (
                      <SelectItem
                        key={action.id}
                        value={action.id}
                        disabled={hasRequiredInputs(action)}
                      >
                        <ActionIcon className="size-4 shrink-0" />
                        {action.name}
                      </SelectItem>
                    )
                  })}
                </SelectContent>
              </Select>
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

            <AutomationAgentFields
              device={device}
              agent={agent}
              onAgentChange={switchAgent}
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
            <Button
              type="button"
              variant="ghost"
              onClick={() => onOpenChange(false)}
              disabled={submitting}
            >
              Cancel
            </Button>
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
