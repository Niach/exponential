import { MonitorOff } from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
import { Checkbox } from "@/components/ui/checkbox"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { ClaudeIcon, CodexIcon, PiIcon } from "@/components/icons/brand-icons"
import {
  agentAllowsBlankModel,
  agentEffortValues,
  agentModelValues,
  agentSupportsPlanMode,
  agentSupportsSkipPermissions,
  agentSupportsUltracode,
} from "@/lib/coding-launch-prefs"
import type { SteerDevice } from "@/lib/steer-devices"

// The options column of the unified launch dialog (EXP-257) — a
// presentational extraction of the Start-coding dialog's right half: device
// select, agent tab strip, model/effort selects, and the three capability
// toggles. All state and the touched/clamp logic stay in the dialog shell.
// EXP-481 splits the agent strip + model/effort/toggles cluster into
// `AgentOptionsFields` so the device-settings dialog's defaults editor
// renders the identical controls without duplicating them.
// EXP-615 adds the `automation` variant of that cluster: the same strip and
// selects, but every choice may stay unpinned ("Device default" — the
// automations row stores NULL and the bound machine launches with its own
// configured defaults), so there are no run-time toggles.

export const AGENT_LABELS: Record<string, string> = {
  claude: `Claude Code`,
  codex: `Codex`,
  pi: `pi`,
}

// Each agent's brand mark for the tab strip — mirrors the desktop IDE's
// icon + label pill tabs (EXP-213).
const AGENT_ICONS: Record<
  string,
  (props: React.SVGProps<SVGSVGElement>) => React.JSX.Element
> = {
  claude: ClaudeIcon,
  codex: CodexIcon,
  pi: PiIcon,
}

const ResumeBranchIcon = conceptIcon(`ui-branch`)

// Radix Select forbids an empty-string item value; the blank "CLI default"
// model/effort rides this sentinel inside the dialog only.
export const CLI_DEFAULT_EFFORT = `cli-default`
export const CLI_DEFAULT_MODEL = `cli-default`

// The automation variant's twin sentinel: blank agent/model/effort = "run with
// whatever the bound device is configured to launch with" (the row stores
// NULL).
export const DEVICE_DEFAULT = `device-default`

// Display labels derive from the contract values (same rule as the iOS and
// Android sheets), so a new contract value can never render unlabeled; the
// multi-word slugs get explicit labels.
const MODEL_LABELS: Record<string, string> = {
  "gpt-5.6-sol": `GPT-5.6 Sol`,
  "gpt-5.6-terra": `GPT-5.6 Terra`,
  "gpt-5.6-luna": `GPT-5.6 Luna`,
  "grok-4.5": `Grok 4.5`,
}

export function modelLabel(value: string): string {
  return MODEL_LABELS[value] ?? value.charAt(0).toUpperCase() + value.slice(1)
}

export function effortLabel(value: string): string {
  return value === `xhigh` ? `XHigh` : modelLabel(value)
}

/** EXP-481: the "Resume previous session" row's inputs — rendered only when
 * the shell computed an eligible worktree for (device, issue, agent). */
export interface ResumeRowProps {
  checked: boolean
  onChange: (value: boolean) => void
  identifier: string
  branch: string
}

/** The launch variant's run-time switches — never rendered for an automation,
 * which has nobody to answer a permission prompt. */
interface LaunchToggleProps {
  ultracode: boolean
  onUltracodeChange: (value: boolean) => void
  planMode: boolean
  onPlanModeChange: (value: boolean) => void
  /** EXP-481: hidden entirely while a resume is armed — a resumed session
   * never re-enters plan mode (mirrors the desktop dialog). */
  planModeHidden?: boolean
  skipPermissions: boolean
  onSkipPermissionsChange: (value: boolean) => void
  resumeRow?: ResumeRowProps | null
}

type AgentOptionsFieldsProps = {
  idPrefix: string
  /** `` in the automation variant = device default. */
  agent: string
  availableAgents: string[]
  onAgentChange: (agent: string) => void
  model: string
  onModelChange: (model: string) => void
  effortValue: string
  onEffortChange: (effort: string) => void
} & (
  | ({ variant?: `launch` } & LaunchToggleProps)
  | { variant: `automation` }
)

/** The agent strip + model/effort selects + capability toggles — shared
 * verbatim by the launch dialog and the device-settings defaults editor.
 * `idPrefix` keeps element ids unique when both render at once. */
export function AgentOptionsFields(props: AgentOptionsFieldsProps) {
  const {
    idPrefix,
    agent,
    availableAgents,
    onAgentChange,
    model,
    onModelChange,
    effortValue,
    onEffortChange,
  } = props
  const automation = props.variant === `automation`
  const toggles = props.variant === `automation` ? null : props
  // A model or effort is only meaningful against a pinned agent (the server
  // validates the pair), so both stay locked on "Device default" until one is.
  const pinned = !automation || agent !== ``
  const modelSentinel = automation ? DEVICE_DEFAULT : CLI_DEFAULT_MODEL
  const effortSentinel = automation ? DEVICE_DEFAULT : CLI_DEFAULT_EFFORT
  const sentinelLabel = automation ? `Device default` : `CLI default`
  return (
    <>
      {(automation || availableAgents.length > 1) && (
        <div className="space-y-2">
          <Label>Agent</Label>
          <Tabs
            value={agent === `` ? DEVICE_DEFAULT : agent}
            onValueChange={(value) =>
              onAgentChange(
                automation && value === DEVICE_DEFAULT ? `` : value
              )
            }
          >
            <TabsList className="w-full">
              {automation && (
                <TabsTrigger value={DEVICE_DEFAULT} className="flex-1">
                  Device default
                </TabsTrigger>
              )}
              {availableAgents.map((value) => {
                const AgentIcon = AGENT_ICONS[value]
                return (
                  <TabsTrigger key={value} value={value} className="flex-1">
                    {AgentIcon && <AgentIcon className="size-3.5" />}
                    {AGENT_LABELS[value] ?? value}
                  </TabsTrigger>
                )
              })}
            </TabsList>
          </Tabs>
        </div>
      )}
      <div className="grid grid-cols-2 gap-3">
        <div className="space-y-2">
          <Label htmlFor={`${idPrefix}-model`}>Model</Label>
          <Select
            value={model === `` ? modelSentinel : model}
            onValueChange={(value) =>
              onModelChange(value === modelSentinel ? `` : value)
            }
            disabled={!pinned}
          >
            <SelectTrigger id={`${idPrefix}-model`} className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {(automation || agentAllowsBlankModel(agent)) && (
                <SelectItem value={modelSentinel}>{sentinelLabel}</SelectItem>
              )}
              {pinned &&
                agentModelValues(agent).map((value) => (
                  <SelectItem key={value} value={value}>
                    {modelLabel(value)}
                  </SelectItem>
                ))}
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-2">
          <Label htmlFor={`${idPrefix}-effort`}>
            {agent === `pi`
              ? `Thinking`
              : agent === `codex`
                ? `Reasoning`
                : `Effort`}
          </Label>
          <Select
            value={effortValue === `` ? effortSentinel : effortValue}
            onValueChange={(value) =>
              onEffortChange(
                automation && value === effortSentinel ? `` : value
              )
            }
            disabled={
              automation
                ? !pinned
                : toggles!.ultracode && agentSupportsUltracode(agent)
            }
          >
            <SelectTrigger id={`${idPrefix}-effort`} className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={effortSentinel}>{sentinelLabel}</SelectItem>
              {pinned &&
                agentEffortValues(agent).map((value) => (
                  <SelectItem key={value} value={value}>
                    {effortLabel(value)}
                  </SelectItem>
                ))}
            </SelectContent>
          </Select>
        </div>
      </div>
      {toggles && (
        <LaunchToggles {...toggles} idPrefix={idPrefix} agent={agent} />
      )}
    </>
  )
}

function LaunchToggles({
  idPrefix,
  agent,
  ultracode,
  onUltracodeChange,
  planMode,
  onPlanModeChange,
  planModeHidden = false,
  skipPermissions,
  onSkipPermissionsChange,
  resumeRow,
}: LaunchToggleProps & { idPrefix: string; agent: string }) {
  return (
    <>
      {(resumeRow ||
        agentSupportsUltracode(agent) ||
        (agentSupportsPlanMode(agent) && !planModeHidden) ||
        agentSupportsSkipPermissions(agent)) && (
        <div className="space-y-2">
          {resumeRow && (
            <div className="space-y-1">
              <div className="flex items-center gap-2">
                <Checkbox
                  id={`${idPrefix}-resume`}
                  checked={resumeRow.checked}
                  onCheckedChange={(value) =>
                    resumeRow.onChange(value === true)
                  }
                />
                <Label htmlFor={`${idPrefix}-resume`} className="font-normal">
                  Resume previous session
                </Label>
              </div>
              <p className="flex items-center gap-1 pl-6 text-xs text-muted-foreground">
                <ResumeBranchIcon className="size-3 shrink-0" />A worktree for{` `}
                {resumeRow.identifier} already exists ({resumeRow.branch}).
              </p>
            </div>
          )}
          {agentSupportsUltracode(agent) && (
            <div className="flex items-center gap-2">
              <Checkbox
                id={`${idPrefix}-ultracode`}
                checked={ultracode}
                onCheckedChange={(value) => onUltracodeChange(value === true)}
              />
              <Label htmlFor={`${idPrefix}-ultracode`} className="font-normal">
                Dynamic workflows (ultracode)
              </Label>
            </div>
          )}
          {agentSupportsPlanMode(agent) && !planModeHidden && (
            <div className="flex items-center gap-2">
              <Checkbox
                id={`${idPrefix}-plan-mode`}
                checked={planMode}
                onCheckedChange={(value) => onPlanModeChange(value === true)}
              />
              <Label htmlFor={`${idPrefix}-plan-mode`} className="font-normal">
                Plan mode
              </Label>
            </div>
          )}
          {agentSupportsSkipPermissions(agent) && (
            <div className="flex items-center gap-2">
              <Checkbox
                id={`${idPrefix}-skip-permissions`}
                checked={skipPermissions}
                onCheckedChange={(value) =>
                  onSkipPermissionsChange(value === true)
                }
              />
              <Label
                htmlFor={`${idPrefix}-skip-permissions`}
                className="font-normal"
              >
                Skip permissions
              </Label>
            </div>
          )}
        </div>
      )}
    </>
  )
}

export function LaunchOptionsPane({
  devices,
  device,
  onDeviceChange,
  noDeviceNote,
  agent,
  availableAgents,
  onAgentChange,
  model,
  onModelChange,
  effortValue,
  onEffortChange,
  ultracode,
  onUltracodeChange,
  planMode,
  onPlanModeChange,
  planModeHidden,
  skipPermissions,
  onSkipPermissionsChange,
  resumeRow,
}: {
  /** The tab's CANDIDATE devices (capability-filtered by the shell). */
  devices: SteerDevice[]
  device: SteerDevice | undefined
  onDeviceChange: (deviceId: string) => void
  /** Rendered when the candidate list is empty (e.g. no actions-capable
   * desktop online). */
  noDeviceNote: string
  agent: string
  availableAgents: string[]
  onAgentChange: (agent: string) => void
  model: string
  onModelChange: (model: string) => void
  effortValue: string
  onEffortChange: (effort: string) => void
  ultracode: boolean
  onUltracodeChange: (value: boolean) => void
  planMode: boolean
  onPlanModeChange: (value: boolean) => void
  planModeHidden?: boolean
  skipPermissions: boolean
  onSkipPermissionsChange: (value: boolean) => void
  /** EXP-481: rendered when the shell computed a resumable worktree. */
  resumeRow?: ResumeRowProps | null
}) {
  if (devices.length === 0) {
    return (
      <div className="flex shrink-0 flex-col gap-3 sm:min-h-0 sm:shrink sm:overflow-y-auto">
        <div className="flex items-start gap-2 py-2 text-sm text-muted-foreground">
          <MonitorOff className="mt-0.5 size-4 shrink-0" />
          {noDeviceNote}
        </div>
      </div>
    )
  }

  return (
    <div className="flex shrink-0 flex-col gap-3 sm:min-h-0 sm:shrink sm:overflow-y-auto">
      {devices.length > 1 && (
        <div className="space-y-2">
          {/* EXP-615: "Device", byte-identical on all four clients — a
              machine here can equally be a headless CLI daemon. */}
          <Label htmlFor="start-coding-device">Device</Label>
          <Select value={device?.deviceId ?? ``} onValueChange={onDeviceChange}>
            <SelectTrigger id="start-coding-device" className="w-full">
              <SelectValue placeholder="Select a device" />
            </SelectTrigger>
            <SelectContent>
              {devices.map((candidate) => (
                <SelectItem key={candidate.deviceId} value={candidate.deviceId}>
                  {candidate.deviceLabel || candidate.deviceId}
                  {/* EXP-432: teammates' shared servers carry their owner. */}
                  {candidate.owner ? ` — ${candidate.owner.name}` : ``}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      )}
      <AgentOptionsFields
        idPrefix="start-coding"
        agent={agent}
        availableAgents={availableAgents}
        onAgentChange={onAgentChange}
        model={model}
        onModelChange={onModelChange}
        effortValue={effortValue}
        onEffortChange={onEffortChange}
        ultracode={ultracode}
        onUltracodeChange={onUltracodeChange}
        planMode={planMode}
        onPlanModeChange={onPlanModeChange}
        planModeHidden={planModeHidden}
        skipPermissions={skipPermissions}
        onSkipPermissionsChange={onSkipPermissionsChange}
        resumeRow={resumeRow}
      />
    </div>
  )
}
