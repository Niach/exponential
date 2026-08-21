import { useEffect, useMemo, useRef, useState } from "react"
import { LoaderCircle, Sparkles } from "lucide-react"
import { MAX_ACTION_INPUT_TEXT, type BoardIcon } from "@exp/db-schema/domain"
import { builtinCreateAction } from "@/lib/builtin-actions"
import { missingRequiredInputs, buildInputsPayload } from "@/lib/action-inputs"
import { formatTriggerBlock } from "@/lib/action-triggers"
import {
  AutomationSection,
  draftToTrigger,
  emptyAutomationDraft,
  type AutomationDraft,
} from "@/components/automation-section"
import {
  agentSeed,
  agentSupportsPlanMode,
  agentSupportsSkipPermissions,
  agentSupportsUltracode,
  DEFAULT_LAUNCH_AGENT,
} from "@/lib/coding-launch-prefs"
import {
  deviceAgentIds,
  deviceAgentLaunchDefaults,
  deviceCanRunActionInputs,
  deviceCanRunActions,
  deviceDefaultAgent,
  deviceHasRunnableAgent,
  deviceIsOnline,
  type SteerDevice,
} from "@/lib/steer-devices"
import type { ActionRepoOption } from "@/components/action-editor-dialog"
import type { StartCodingOptions } from "@/components/launch-dialog/launch-dialog"
import {
  CLI_DEFAULT_EFFORT,
  LaunchOptionsPane,
} from "@/components/launch-dialog/launch-options-pane"
import { IconPicker } from "@/components/ui/icon-picker"
import { Button } from "@/components/ui/button"
import {
  Dialog,
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
import { Textarea } from "@/components/ui/textarea"

// The dedicated "New action" dialog (EXP-431): the builtin "Create action"
// run got its own creation-flavored surface instead of riding the generic
// launch dialog as just another list row. The description leads as a
// Textarea (the generic input renderer would give the locked `text` def a
// one-line field), repo + icon follow, and the right column reuses the
// launch dialog's options pane verbatim. Submitting is still an ordinary
// remote builtin run — the shell calls `remote.runAction` with the locked
// builtin id and the {description, repo?, icon?} inputs payload.

// Radix Select forbids an empty-string item value; the unset optional repo
// rides this sentinel inside the dialog only.
const NO_REPO = `none`

export function CreateActionDialog({
  open,
  onOpenChange,
  devices,
  starting,
  teamId,
  repos,
  initialDescription,
  initialIcon,
  onCreate,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  devices: SteerDevice[]
  starting: boolean
  teamId: string
  /** The team's connected repos, for the optional repo input. */
  repos: ActionRepoOption[]
  /** EXP-530 suggestion prefill — applied on OPEN only (the reset effect). */
  initialDescription?: string
  initialIcon?: string
  onCreate: (
    device: SteerDevice,
    options: StartCodingOptions,
    inputs?: Record<string, string>
  ) => void
}) {
  // Input defs come from the builtin factory so the payload keys, the
  // required flag, and the description placeholder can never drift from the
  // cross-client contract.
  const inputDefs = useMemo(() => builtinCreateAction(teamId).inputs, [teamId])
  const descriptionDef = inputDefs.find((def) => def.key === `description`)

  const [description, setDescription] = useState(``)
  const [repoId, setRepoId] = useState(``)
  const [icon, setIcon] = useState(``)
  // EXP-530: the Automation section's controlled draft. The configured
  // trigger is NOT sent to the server from here — it is appended to the
  // description as a machine-readable block the creator agent copies into
  // `exponential_actions_create`'s `trigger` field.
  const [automation, setAutomation] = useState<AutomationDraft>(
    emptyAutomationDraft
  )

  // Launch options — the same device-seeded cluster as the launch dialog
  // shell (a deliberate small duplication; the 677-line shell isn't worth a
  // hook extraction for one sibling).
  const [agent, setAgent] = useState<string>(DEFAULT_LAUNCH_AGENT)
  const [model, setModel] = useState(``)
  const [effortValue, setEffortValue] = useState(CLI_DEFAULT_EFFORT)
  const [ultracode, setUltracode] = useState(false)
  const [planMode, setPlanMode] = useState(false)
  const [skipPermissions, setSkipPermissions] = useState(false)
  const [deviceId, setDeviceId] = useState<string | null>(null)
  // EXP-437: the deviceId whose launch defaults last seeded the options —
  // re-polls with the same device must not stomp in-dialog edits.
  const seededDeviceRef = useRef<string | null>(null)

  // Seed fields + options on OPEN only — a desktop connecting mid-dialog
  // (the settle effect below) must never wipe a typed description. Options
  // start at static contract defaults; the device-seed effect below overlays
  // the selected machine's advertised defaults (EXP-437).
  useEffect(() => {
    if (!open) return
    setDescription(initialDescription ?? ``)
    setRepoId(``)
    setIcon(initialIcon ?? ``)
    setAutomation(emptyAutomationDraft())
    setDeviceId(null)
    seededDeviceRef.current = null
    const seed = agentSeed(DEFAULT_LAUNCH_AGENT, null)
    setAgent(DEFAULT_LAUNCH_AGENT)
    setModel(seed.model)
    setEffortValue(CLI_DEFAULT_EFFORT)
    setUltracode(seed.ultracode)
    setPlanMode(seed.planMode)
    setSkipPermissions(seed.skipPermissions)
  }, [open])

  // The builtin always needs the action-inputs cap (same gate the launch
  // dialog applies to it), so an outdated desktop can't be picked here.
  const candidateDevices = useMemo(
    () =>
      devices
        .filter(deviceIsOnline)
        .filter(deviceHasRunnableAgent)
        .filter(deviceCanRunActions)
        .filter(deviceCanRunActionInputs),
    [devices]
  )

  // Settle the device on open + whenever the candidate list changes; a
  // still-valid current choice is kept, else the first candidate wins.
  useEffect(() => {
    if (!open) return
    setDeviceId((current) =>
      current && candidateDevices.some((d) => d.deviceId === current)
        ? current
        : (candidateDevices[0]?.deviceId ?? null)
    )
  }, [open, candidateDevices])

  const device =
    candidateDevices.find((candidate) => candidate.deviceId === deviceId) ??
    candidateDevices[0]

  // Switching the agent tab re-seeds model/effort/toggles to the SELECTED
  // DEVICE's defaults for that agent (EXP-437; static when it advertises
  // none), capability-clamped.
  const switchAgent = (next: string) => {
    if (next === agent) return
    setAgent(next)
    const seed = agentSeed(next, deviceAgentLaunchDefaults(device, next))
    setModel(seed.model)
    setEffortValue(seed.effort === `` ? CLI_DEFAULT_EFFORT : seed.effort)
    setUltracode(seed.ultracode)
    setPlanMode(seed.planMode)
    setSkipPermissions(seed.skipPermissions)
  }

  // EXP-437: seed the launch options from the selected device's advertised
  // per-agent defaults — once a device settles after open, and again on
  // every actual device change (the ref latch skips same-device re-polls).
  useEffect(() => {
    if (!open || !device) return
    if (seededDeviceRef.current === device.deviceId) return
    seededDeviceRef.current = device.deviceId
    const available = deviceAgentIds(device)
    const next =
      deviceDefaultAgent(device) ??
      (available.includes(agent)
        ? agent
        : (available[0] ?? DEFAULT_LAUNCH_AGENT))
    const seed = agentSeed(next, deviceAgentLaunchDefaults(device, next))
    setAgent(next)
    setModel(seed.model)
    setEffortValue(seed.effort === `` ? CLI_DEFAULT_EFFORT : seed.effort)
    setUltracode(seed.ultracode)
    setPlanMode(seed.planMode)
    setSkipPermissions(seed.skipPermissions)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, device?.deviceId])

  // EXP-201: only agents the chosen device advertised are offerable; a
  // device change re-clamps a now-unavailable selection.
  const availableAgents = deviceAgentIds(device)
  const availableAgentsKey = availableAgents.join(`,`)
  useEffect(() => {
    if (!open) return
    if (!availableAgents.includes(agent)) {
      switchAgent(availableAgents[0] ?? `claude`)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, availableAgentsKey, agent])

  const inputValues = { description, repo: repoId, icon }

  // EXP-530: a configured trigger rides the description input as a trailing
  // machine-readable block — the combined value must still fit the server's
  // per-value text cap, so an overflow blocks submit with an inline message
  // instead of a server-side reject. The automation binds to the desktop
  // that runs the creation (EXP-574 follow-up — no second device picker).
  const trigger = draftToTrigger({
    ...automation,
    deviceId: device?.deviceId ?? null,
  })
  const descriptionWithTrigger = trigger
    ? `${description}${formatTriggerBlock(trigger)}`
    : description
  const triggerOverflow = descriptionWithTrigger.length > MAX_ACTION_INPUT_TEXT
  const submitBlocked =
    missingRequiredInputs(inputDefs, inputValues).length > 0 || triggerOverflow

  const submit = () => {
    if (!device || submitBlocked) return
    const options: StartCodingOptions = {
      agent,
      model,
      effort: effortValue === CLI_DEFAULT_EFFORT ? `` : effortValue,
      ultracode: ultracode && agentSupportsUltracode(agent),
      planMode: planMode && agentSupportsPlanMode(agent),
      skipPermissions: skipPermissions && agentSupportsSkipPermissions(agent),
    }
    onCreate(
      device,
      options,
      buildInputsPayload(inputDefs, {
        ...inputValues,
        description: descriptionWithTrigger,
      })
    )
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-3 sm:max-h-[85dvh] sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>New action</DialogTitle>
        </DialogHeader>
        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto sm:grid sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] sm:gap-5 sm:overflow-y-visible">
          <div className="flex shrink-0 flex-col gap-3 sm:min-h-0 sm:shrink sm:overflow-y-auto">
            <div className="space-y-2">
              <Label htmlFor="create-action-description">Description</Label>
              <Textarea
                id="create-action-description"
                autoFocus
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder={descriptionDef?.placeholder}
                className="min-h-28"
                // Client parity with the server's per-value cap, so a long
                // paste is refused at the field instead of at submit.
                maxLength={MAX_ACTION_INPUT_TEXT}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="create-action-repo">Repository (optional)</Label>
              <Select
                value={repoId || NO_REPO}
                onValueChange={(value) =>
                  setRepoId(value === NO_REPO ? `` : value)
                }
              >
                <SelectTrigger id="create-action-repo" className="w-full">
                  <SelectValue placeholder="Select a repository" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={NO_REPO}>None</SelectItem>
                  {repos.map((repo) => (
                    <SelectItem key={repo.id} value={repo.id}>
                      {repo.fullName}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label>Icon (optional)</Label>
              <IconPicker
                value={icon as BoardIcon | ``}
                onChange={setIcon}
                allowsNone
              />
            </div>
            <AutomationSection
              draft={automation}
              onChange={setAutomation}
              devices={devices}
              teamId={teamId}
              showDevicePicker={false}
            />
            {triggerOverflow && (
              <p className="text-xs text-destructive">
                Description plus the automation block exceeds the
                {` ${MAX_ACTION_INPUT_TEXT}`}-character input limit. Shorten
                the description.
              </p>
            )}
          </div>
          <LaunchOptionsPane
            devices={candidateDevices}
            device={device}
            onDeviceChange={setDeviceId}
            noDeviceNote={`No capable desktop online. This action needs a desktop app new enough to run action inputs.`}
            agent={agent}
            availableAgents={availableAgents}
            onAgentChange={switchAgent}
            model={model}
            onModelChange={setModel}
            effortValue={effortValue}
            onEffortChange={setEffortValue}
            ultracode={ultracode}
            onUltracodeChange={setUltracode}
            planMode={planMode}
            onPlanModeChange={setPlanMode}
            skipPermissions={skipPermissions}
            onSkipPermissionsChange={setSkipPermissions}
          />
        </div>
        <DialogFooter>
          <Button
            variant="ghost"
            onClick={() => onOpenChange(false)}
            disabled={starting}
          >
            Cancel
          </Button>
          <Button onClick={submit} disabled={starting || !device || submitBlocked}>
            {starting ? <LoaderCircle className="animate-spin" /> : <Sparkles />}
            Create
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
