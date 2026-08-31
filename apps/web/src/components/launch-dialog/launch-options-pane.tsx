import { MonitorOff } from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
import {
  GlassGroup,
  GlassPickerRow,
  GlassTabsRow,
  GlassToggleRow,
  type GlassPickerOption,
} from "@/components/ui/glass-rows"
import { TabsTrigger } from "@/components/ui/tabs"
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
// EXP-616 dresses the cluster in the iOS grouped-glass vocabulary: rows of
// label-leading pickers and toggles (`components/ui/glass-rows`).
// EXP-694 collapses it into ONE card on every client (the Android device-edit
// stack is the reference): the agent strip is the group's EMBEDDED FIRST ROW
// — no "Agent" label above it, no floating capsule — and model, effort, the
// run-time toggles and whatever `renderAgentFooter` adds are the rows under
// it. The device/"Runs on" picker is NOT part of this card: the caller keeps
// it in its own group ABOVE (see `LaunchOptionsPane`).

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
   * EXP-688: what to render UNDER the selected agent's toggles — the device
   * settings dialog puts that agent's account and usage there, so the tab you
   * are editing is the tab that tells you whose account it runs as. The
   * launch dialog passes nothing: a run in flight is no place to sign in.
   * EXP-694: these are the card's FINAL ROWS, so what it returns must be
   * row-shaped (`px-4 py-3`), not a loose block.
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
    // EXP-694: ONE card — the agent strip is its first row, everything the
    // agent runs with follows underneath.
    <GlassGroup>
      {availableAgents.length > 1 && (
        <GlassTabsRow value={agent} onValueChange={onAgentChange}>
          {availableAgents.map((value) => {
            const AgentIcon = AGENT_ICONS[value]
            return (
              <TabsTrigger key={value} value={value}>
                {AgentIcon && <AgentIcon className="size-3.5" />}
                {AGENT_LABELS[value] ?? value}
              </TabsTrigger>
            )
          })}
        </GlassTabsRow>
      )}
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
      {/* EXP-694: the run-time switches are rows of the SAME card now — an
          automation never gets them (nobody is there to steer a plan), and a
          capability the agent lacks simply drops its row. */}
      {toggles?.resumeRow && (
        <GlassToggleRow
          id={`${idPrefix}-resume`}
          label="Resume previous session"
          checked={toggles.resumeRow.checked}
          onCheckedChange={toggles.resumeRow.onChange}
          description={
            <span className="flex items-center gap-1">
              <ResumeBranchIcon className="size-3 shrink-0" />
              {`A worktree for ${toggles.resumeRow.identifier} already exists (${toggles.resumeRow.branch}).`}
            </span>
          }
        />
      )}
      {toggles && agentSupportsUltracode(agent) && (
        <GlassToggleRow
          id={`${idPrefix}-ultracode`}
          label="Ultracode"
          checked={toggles.ultracode}
          onCheckedChange={toggles.onUltracodeChange}
        />
      )}
      {toggles && agentSupportsPlanMode(agent) && !toggles.planModeHidden && (
        <GlassToggleRow
          id={`${idPrefix}-plan-mode`}
          label="Plan mode"
          checked={toggles.planMode}
          onCheckedChange={toggles.onPlanModeChange}
        />
      )}
      {renderAgentFooter?.(agent)}
    </GlassGroup>
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
    // EXP-694: 8px between groups, the same rhythm the native sheets use.
    <div className="flex shrink-0 flex-col gap-2 sm:min-h-0 sm:shrink sm:overflow-y-auto">
      {devices.length > 1 && (
        /* EXP-694: WHERE it runs is its own card above WHAT it runs with —
           mirrors the automations sheet's "Runs on" group.
           EXP-615: "Device", byte-identical on all four clients — a machine
           here can equally be a headless CLI daemon. */
        <GlassGroup>
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
        </GlassGroup>
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
        resumeRow={resumeRow}
      />
    </div>
  )
}
