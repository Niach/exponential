import { MonitorOff } from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
import { Label } from "@/components/ui/label"
import {
  GlassGroup,
  GlassPickerRow,
  GlassToggleRow,
  type GlassPickerOption,
} from "@/components/ui/glass-rows"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { ClaudeIcon, CodexIcon, PiIcon } from "@/components/icons/brand-icons"
import {
  agentAllowsBlankModel,
  agentEffortValues,
  agentModelValues,
  agentSupportsPlanMode,
  agentSupportsUltracode,
} from "@/lib/coding-launch-prefs"
import type { SteerDevice } from "@/lib/steer-devices"

// The options column of the unified launch dialog (EXP-257) — a
// presentational extraction of the Start-coding dialog's right half: device
// select, agent tab strip, model/effort selects, and the capability
// toggles. All state and the touched/clamp logic stay in the dialog shell.
// EXP-481 splits the agent strip + model/effort/toggles cluster into
// `AgentOptionsFields` so the device-settings dialog's defaults editor
// renders the identical controls without duplicating them.
// EXP-615 adds the `automation` variant of that cluster: the exact same
// strip and selects (the agent seeds to the bound device's default launch
// agent; blank model/effort store NULL), minus the run-time toggles — an
// unattended run never parks on plan mode.
// EXP-616 dresses the cluster in the iOS grouped-glass vocabulary: the agent
// capsule stays uncarded and flush, everything else is a grouped card of
// label-leading picker/toggle ROWS (`components/ui/glass-rows`). The "Agent"
// label LEADS the column so it sits on the same baseline as the left half's
// section label ("Issues"/the name field); the machine, the model and the
// effort then share ONE card (device first), and the run-time toggles follow
// as their own — so the caller hands its device picker over as `deviceRow` and
// this renders it as that card's first row.

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
 * which has nobody to steer a plan or a resume. */
interface LaunchToggleProps {
  ultracode: boolean
  onUltracodeChange: (value: boolean) => void
  planMode: boolean
  onPlanModeChange: (value: boolean) => void
  /** EXP-481: hidden entirely while a resume is armed — a resumed session
   * never re-enters plan mode (mirrors the desktop dialog). */
  planModeHidden?: boolean
  resumeRow?: ResumeRowProps | null
}

type AgentOptionsFieldsProps = {
  idPrefix: string
  /**
   * EXP-616: the caller's device picker row (a `GlassPickerRow`), rendered as
   * the FIRST row of the model/effort card. Absent where there is nothing to
   * pick: the device-settings defaults editor edits one machine's own
   * defaults, and a one-machine launch has no choice to offer, so neither
   * paints a device row at all.
   */
  deviceRow?: React.ReactNode
  /**
   * EXP-688: what to render UNDER the selected agent's toggles — the device
   * settings dialog puts that agent's account and usage there, so the tab you
   * are editing is the tab that tells you whose account it runs as. The
   * launch dialog passes nothing: a run in flight is no place to sign in.
   */
  renderAgentFooter?: (agent: string) => React.ReactNode
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
    deviceRow,
    renderAgentFooter,
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
  // validates the pair). An automation's strip seeds to the bound device's
  // default agent, so `` only happens while no device is bound yet.
  const pinned = !automation || agent !== ``
  const modelSentinel = CLI_DEFAULT_MODEL
  const effortSentinel = CLI_DEFAULT_EFFORT
  const sentinelLabel = `CLI default`
  const modelOptions: GlassPickerOption[] = [
    ...(automation || agentAllowsBlankModel(agent)
      ? [{ value: modelSentinel, label: sentinelLabel }]
      : []),
    ...(pinned
      ? agentModelValues(agent).map((value) => ({
          value,
          label: modelLabel(value),
        }))
      : []),
  ]
  const effortOptions: GlassPickerOption[] = [
    { value: effortSentinel, label: sentinelLabel },
    ...(pinned
      ? agentEffortValues(agent).map((value) => ({
          value,
          label: effortLabel(value),
        }))
      : []),
  ]
  return (
    <>
      {availableAgents.length > 1 && (
        <div className="space-y-2">
          <Label>Agent</Label>
          <Tabs value={agent} onValueChange={onAgentChange}>
            <TabsList className="w-full">
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
      {/* Where and what the agent runs with — ONE card: the machine leads,
          model and effort follow. */}
      <GlassGroup>
        {deviceRow}
        <GlassPickerRow
          label="Model"
          value={model === `` ? modelSentinel : model}
          onValueChange={(value) =>
            onModelChange(value === modelSentinel ? `` : value)
          }
          options={modelOptions}
          disabled={!pinned}
        />
        <GlassPickerRow
          label={
            agent === `pi`
              ? `Thinking`
              : agent === `codex`
                ? `Reasoning`
                : `Effort`
          }
          value={effortValue === `` ? effortSentinel : effortValue}
          onValueChange={(value) =>
            onEffortChange(automation && value === effortSentinel ? `` : value)
          }
          options={effortOptions}
          disabled={
            automation
              ? !pinned
              : toggles!.ultracode && agentSupportsUltracode(agent)
          }
        />
      </GlassGroup>
      {toggles && (
        <LaunchToggles {...toggles} idPrefix={idPrefix} agent={agent} />
      )}
      {renderAgentFooter?.(agent)}
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
  resumeRow,
}: LaunchToggleProps & { idPrefix: string; agent: string }) {
  // EXP-616: the second grouped card — one switch row per capability,
  // mirroring the iOS sheet's toggles section. The guard stays outside it so
  // an agent with no applicable capability paints no empty card.
  return (
    <>
      {(resumeRow ||
        agentSupportsUltracode(agent) ||
        (agentSupportsPlanMode(agent) && !planModeHidden)) && (
        <GlassGroup>
          {resumeRow && (
            <GlassToggleRow
              id={`${idPrefix}-resume`}
              label="Resume previous session"
              checked={resumeRow.checked}
              onCheckedChange={resumeRow.onChange}
              description={
                <span className="flex items-center gap-1">
                  <ResumeBranchIcon className="size-3 shrink-0" />
                  {`A worktree for ${resumeRow.identifier} already exists (${resumeRow.branch}).`}
                </span>
              }
            />
          )}
          {agentSupportsUltracode(agent) && (
            <GlassToggleRow
              id={`${idPrefix}-ultracode`}
              label="Dynamic workflows (ultracode)"
              checked={ultracode}
              onCheckedChange={onUltracodeChange}
            />
          )}
          {agentSupportsPlanMode(agent) && !planModeHidden && (
            <GlassToggleRow
              id={`${idPrefix}-plan-mode`}
              label="Plan mode"
              checked={planMode}
              onCheckedChange={onPlanModeChange}
            />
          )}
        </GlassGroup>
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
      <AgentOptionsFields
        idPrefix="start-coding"
        deviceRow={
          devices.length > 1 ? (
            /* EXP-615: "Device", byte-identical on all four clients — a
               machine here can equally be a headless CLI daemon. */
            <GlassPickerRow
              label="Device"
              value={device?.deviceId ?? ``}
              onValueChange={onDeviceChange}
              placeholder="Select a device"
              options={devices.map((candidate) => ({
                value: candidate.deviceId,
                // EXP-432: teammates' shared servers carry their owner.
                label: `${candidate.deviceLabel || candidate.deviceId}${
                  candidate.owner ? ` — ${candidate.owner.name}` : ``
                }`,
              }))}
            />
          ) : null
        }
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
        resumeRow={resumeRow}
      />
    </div>
  )
}
