import { conceptIcon } from "@/lib/icons.generated"
import {
  GlassGroup,
  GlassPickerRow,
  GlassToggleRow,
  type GlassPickerOption,
} from "@/components/ui/glass-rows"
import { AgentPickerTabs } from "@/components/agent-picker"
import {
  agentAllowsBlankModel,
  agentEffortValues,
  agentModelValues,
  agentSupportsPlanMode,
  agentSupportsUltracode,
} from "@/lib/coding-launch-prefs"
import { deviceAgentNotReady, type SteerDevice } from "@/lib/steer-devices"

// The agent options cluster (EXP-257; the launch dialog's right half until
// EXP-825 retired that dialog for the Agent page composer, whose inline
// options line lives in `launch-options-line.tsx`). What remains here is
// shared by the device-settings defaults editor and the automations editor:
// EXP-481 split the agent strip + model/effort/toggles cluster into
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
// run-time toggles are the rows under it. EXP-862 dropped the account footer
// that used to close the card (accounts live on their own page, never in a
// launcher). The device/"Runs on" picker is NOT part of this card: the caller keeps
// it in its own group ABOVE.

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
  /** `` in the automation variant = device default. */
  agent: string
  availableAgents: string[]
  onAgentChange: (agent: string) => void
  model: string
  onModelChange: (model: string) => void
  effortValue: string
  onEffortChange: (effort: string) => void
  /**
   * EXP-749/EXP-773: the machine the run lands on. Pickers never FILTER on
   * what it can drive over ACP; an agent outside its reported set gets a
   * one-line "not ready" note under the strip instead (and the caller blocks
   * the start). Absent (or a build that never reported) = no note.
   */
  device?: SteerDevice
} & (
  | ({ variant?: `launch` } & LaunchToggleProps)
  | { variant: `automation` }
)

/** The agent strip + model/effort selects + capability toggles — shared
 * by the automation editor and the device-settings defaults editor.
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
    device,
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
        /* EXP-862: the shared picker's TABS variant — one strip, one set of
           marks and labels, wherever an agent is picked side by side. The
           card's own padding replaces the strip's page padding (EXP-694: the
           strip is the group's first ROW, not a control floating over it). */
        <AgentPickerTabs
          value={agent}
          agents={availableAgents}
          onChange={onAgentChange}
          className="px-2 py-2"
        />
      )}
      {deviceAgentNotReady(device, agent) && (
        /* EXP-773: with the PTY path gone this combination cannot start at
           all — the caller disables the start button on the same predicate. */
        <div className="px-4 py-2 text-[0.6875rem] text-muted-foreground">
          {`Not ready on ${device!.deviceLabel || device!.deviceId}. Run the doctor there.`}
        </div>
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
        label={agent === `codex` ? `Reasoning` : `Effort`}
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
    </GlassGroup>
  )
}
