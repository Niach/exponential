import { useEffect, useMemo, useState } from "react"
import { LoaderCircle } from "lucide-react"
import {
  MAX_ACTION_INPUT_TEXT,
  type AutomationTrigger,
  type BoardIcon,
} from "@exp/db-schema/domain"
import { conceptIcon } from "@/lib/icons.generated"
import { builtinCreateAction } from "@/lib/builtin-actions"
import { missingRequiredInputs, buildInputsPayload } from "@/lib/action-inputs"
import { formatAutomationBlock, triggerSummary } from "@/lib/action-triggers"
import {
  AutomationAgentFields,
  AutomationDevicePicker,
  AutomationTriggerFields,
  automationDevices,
  clampAgentFields,
  draftFromTrigger,
  emptyAutomationDraft,
  draftToTrigger,
  type AutomationDraft,
} from "@/components/automation-section"
import {
  deviceAgentIds,
  deviceCanRunActionInputs,
  deviceCanRunActions,
  deviceDefaultAgent,
  deviceHasRunnableAgent,
  deviceIsOnline,
  type SteerDevice,
} from "@/lib/steer-devices"
import { cn } from "@/lib/utils"
import type { ActionRepoOption } from "@/components/action-editor-dialog"
import type { StartCodingOptions } from "@/components/launch-dialog/launch-dialog"
import { LaunchOptionsPane } from "@/components/launch-dialog/launch-options-pane"
import { useLaunchOptions } from "@/components/launch-dialog/use-launch-options"
import { IconPicker } from "@/components/ui/icon-picker"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
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
// launch dialog as just another list row. Icon + name lead, the description
// follows as a Textarea (the generic input renderer would give the locked
// `text` def a one-line field), then the repo, and the right column reuses the
// launch dialog's options pane verbatim. Submitting is still an ordinary
// remote builtin run — the shell calls `remote.runAction` with the locked
// builtin id and the {description, name?, repo?, icon?} inputs payload.
// EXP-615: the automation is a always-visible summary row that slides into its
// own detail view, so the suggestion-prefilled flow and the plain one are ONE
// layout (they were two before).

// Radix Select forbids an empty-string item value; the unset optional repo
// rides this sentinel inside the dialog only.
const NO_REPO = `none`

const AutomationIcon = conceptIcon(`action-automation`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)
const BackIcon = conceptIcon(`ui-back`)
const CreateIcon = conceptIcon(`action-create`)

/** Which half of the dialog body is on screen — the form, or the automation
 * detail it slides over. */
type DialogView = `form` | `automation`

export function CreateActionDialog({
  open,
  onOpenChange,
  devices,
  starting,
  teamId,
  repos,
  initialDescription,
  initialIcon,
  automationPrefill,
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
  /** EXP-583: an "Action + automation" suggestion's trigger — seeds the
   * automation row so the suggestion arrives with its schedule already
   * filled in. The dialog itself never talks to the automations API: the
   * choices ride the description as a machine-readable block the creator
   * agent copies into `exponential_automations_create`. */
  automationPrefill?: AutomationTrigger
  onCreate: (
    device: SteerDevice,
    options: StartCodingOptions,
    inputs?: Record<string, string>
  ) => void
}) {
  // Input defs come from the builtin factory so the payload keys, the
  // required flag, and the placeholders can never drift from the
  // cross-client contract.
  const inputDefs = useMemo(() => builtinCreateAction(teamId).inputs, [teamId])
  const descriptionDef = inputDefs.find((def) => def.key === `description`)
  const nameDef = inputDefs.find((def) => def.key === `name`)

  const [view, setView] = useState<DialogView>(`form`)
  const [description, setDescription] = useState(``)
  const [name, setName] = useState(``)
  const [repoId, setRepoId] = useState(``)
  const [icon, setIcon] = useState(``)
  // EXP-583: the automation's controlled state. Nothing here is written by
  // the dialog — it all rides the description block above.
  const [hasAutomation, setHasAutomation] = useState(false)
  const [automation, setAutomation] = useState<AutomationDraft>(
    emptyAutomationDraft
  )
  const [automationDeviceId, setAutomationDeviceId] = useState<string | null>(
    null
  )
  const [automationAgent, setAutomationAgent] = useState(``)
  const [automationModel, setAutomationModel] = useState(``)
  const [automationEffort, setAutomationEffort] = useState(``)

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

  // The same device-settle + device-seeded options cluster the launch dialog
  // runs (EXP-437/EXP-201).
  const launch = useLaunchOptions({ open, devices: candidateDevices })
  const device = launch.device

  // Seed fields on OPEN only — a desktop connecting mid-dialog (the settle
  // effect inside the hook) must never wipe a typed description.
  useEffect(() => {
    if (!open) return
    setView(`form`)
    setDescription(initialDescription ?? ``)
    setName(``)
    setRepoId(``)
    setIcon(initialIcon ?? ``)
    setHasAutomation(automationPrefill !== undefined)
    setAutomation(draftFromTrigger(automationPrefill ?? null))
    setAutomationDeviceId(null)
    setAutomationAgent(``)
    setAutomationModel(``)
    setAutomationEffort(``)
  }, [open])

  // The automation's own runner list — automation-capable machines, online or
  // not (a schedule catches up on reconnect), independent from the desktop
  // that runs the creator agent right now.
  const automationCandidates = useMemo(
    () => automationDevices(devices),
    [devices]
  )
  useEffect(() => {
    if (!open || !hasAutomation) return
    setAutomationDeviceId((current) =>
      current && automationCandidates.some((d) => d.deviceId === current)
        ? current
        : (automationCandidates[0]?.deviceId ?? null)
    )
  }, [open, hasAutomation, automationCandidates])
  const automationDevice = automationCandidates.find(
    (candidate) => candidate.deviceId === automationDeviceId
  )
  // EXP-615: no "Device default" agent pill — the strip seeds to the bound
  // device's default launch agent, exactly like the start-coding dialog.
  useEffect(() => {
    if (!open || !automationDevice) return
    if (
      automationAgent !== `` &&
      deviceAgentIds(automationDevice).includes(automationAgent)
    )
      return
    const next =
      deviceDefaultAgent(automationDevice) ??
      deviceAgentIds(automationDevice)[0] ??
      ``
    setAutomationAgent(next)
    const clamped = clampAgentFields(next, automationModel, automationEffort)
    setAutomationModel(clamped.model)
    setAutomationEffort(clamped.effort)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, automationDevice])
  const switchAutomationAgent = (next: string) => {
    setAutomationAgent(next)
    const clamped = clampAgentFields(next, automationModel, automationEffort)
    setAutomationModel(clamped.model)
    setAutomationEffort(clamped.effort)
  }

  const openAutomation = () => {
    setHasAutomation(true)
    setView(`automation`)
  }
  const removeAutomation = () => {
    setHasAutomation(false)
    setAutomation(emptyAutomationDraft())
    setAutomationAgent(``)
    setAutomationModel(``)
    setAutomationEffort(``)
    setView(`form`)
  }

  const inputValues = { description, name, repo: repoId, icon }

  // EXP-583: a configured automation rides the description input as a trailing
  // machine-readable block the creator agent copies into
  // `exponential_automations_create` — the combined value must still fit the
  // server's per-value text cap, so an overflow blocks submit with an inline
  // message instead of a server-side reject. Without a bindable machine there
  // is nothing to run it on, so the block is simply left off.
  const descriptionWithTrigger =
    hasAutomation && automationDeviceId
      ? `${description}${formatAutomationBlock({
          trigger: draftToTrigger(automation),
          deviceId: automationDeviceId,
          agent: automationAgent || undefined,
          model: automationModel || undefined,
          effort: automationEffort || undefined,
        })}`
      : description
  const triggerOverflow = descriptionWithTrigger.length > MAX_ACTION_INPUT_TEXT
  const submitBlocked =
    missingRequiredInputs(inputDefs, inputValues).length > 0 || triggerOverflow

  const submit = () => {
    if (!device || submitBlocked) return
    onCreate(
      device,
      launch.buildOptions(),
      buildInputsPayload(inputDefs, {
        ...inputValues,
        description: descriptionWithTrigger,
      })
    )
  }

  const automationSummary = hasAutomation
    ? [
        triggerSummary(draftToTrigger(automation)),
        automationDevice?.deviceLabel || automationDeviceId,
      ]
        .filter(Boolean)
        .join(` · `)
    : `No automation`

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/* Fixed panel height from `sm` up (EXP-615): the automation detail
          slides over the form inside ONE frame, so the dialog must not resize
          between the two. Mobile stays the full-screen page. */}
      <DialogContent className="gap-3 sm:h-[min(85dvh,34rem)] sm:max-h-[85dvh] sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>New action</DialogTitle>
        </DialogHeader>
        <div className="relative min-h-0 flex-1 overflow-hidden">
          <div
            // `inert` (not aria-hidden) keeps focus and the tab order out of
            // the off-screen half without hiding a focused element from AT.
            inert={view === `automation`}
            className={cn(
              `absolute inset-0 flex flex-col gap-3 overflow-y-auto transition-transform duration-200 ease-out sm:grid sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] sm:gap-5 sm:overflow-y-visible`,
              view === `automation` &&
                `pointer-events-none -translate-x-full opacity-0`
            )}
          >
            <div className="flex shrink-0 flex-col gap-3 sm:min-h-0 sm:shrink sm:overflow-y-auto">
              <div className="space-y-2">
                <Label htmlFor="create-action-name">Name (optional)</Label>
                <div className="flex items-center gap-2">
                  <IconPicker
                    id="create-action-icon"
                    value={icon as BoardIcon | ``}
                    onChange={setIcon}
                    allowsNone
                  />
                  <Input
                    id="create-action-name"
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    placeholder={nameDef?.placeholder}
                    maxLength={MAX_ACTION_INPUT_TEXT}
                  />
                </div>
              </div>
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
              <button
                type="button"
                onClick={openAutomation}
                className="flex w-full items-center gap-2 rounded-md border border-border px-3 py-2 text-left hover:bg-muted/50"
              >
                <AutomationIcon className="size-4 shrink-0 text-muted-foreground" />
                <span className="min-w-0 flex-1">
                  <span className="block text-sm">Automation</span>
                  <span className="block truncate text-xs text-muted-foreground">
                    {automationSummary}
                  </span>
                </span>
                <ChevronRightIcon className="size-4 shrink-0 text-muted-foreground" />
              </button>
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
              onDeviceChange={launch.setDeviceId}
              noDeviceNote={`No capable desktop online. This action needs a desktop app new enough to run action inputs.`}
              agent={launch.agent}
              availableAgents={launch.availableAgents}
              onAgentChange={launch.switchAgent}
              model={launch.model}
              onModelChange={launch.setModel}
              effortValue={launch.effortValue}
              onEffortChange={launch.setEffortValue}
              ultracode={launch.ultracode}
              onUltracodeChange={launch.setUltracode}
              planMode={launch.planMode}
              onPlanModeChange={launch.setPlanMode}
              skipPermissions={launch.skipPermissions}
              onSkipPermissionsChange={launch.setSkipPermissions}
            />
          </div>

          <div
            inert={view === `form`}
            className={cn(
              `absolute inset-0 flex flex-col gap-3 transition-transform duration-200 ease-out`,
              view === `form` && `pointer-events-none translate-x-full opacity-0`
            )}
          >
            <div className="flex shrink-0 items-center gap-2">
              <Button
                variant="ghost"
                size="icon"
                aria-label="Back"
                onClick={() => setView(`form`)}
              >
                <BackIcon />
              </Button>
              <span className="flex-1 text-sm font-medium">Automation</span>
              {hasAutomation && (
                <Button
                  variant="ghost"
                  size="sm"
                  className="text-destructive"
                  onClick={removeAutomation}
                >
                  Remove automation
                </Button>
              )}
            </div>
            <div className="min-h-0 flex-1 space-y-3 overflow-y-auto px-1">
              <AutomationTriggerFields
                draft={automation}
                onChange={setAutomation}
                teamId={teamId}
              />
              <AutomationDevicePicker
                id="create-action-automation-device"
                deviceId={automationDeviceId}
                devices={automationCandidates}
                onChange={setAutomationDeviceId}
              />
              <AutomationAgentFields
                idPrefix="create-action-automation"
                device={automationDevice}
                agent={automationAgent}
                onAgentChange={switchAutomationAgent}
                model={automationModel}
                onModelChange={setAutomationModel}
                effort={automationEffort}
                onEffortChange={setAutomationEffort}
              />
            </div>
          </div>
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
            {starting ? (
              <LoaderCircle className="animate-spin" />
            ) : (
              <CreateIcon />
            )}
            Create
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
