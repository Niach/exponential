import { MonitorOff } from "lucide-react"
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

function modelLabel(value: string): string {
  return MODEL_LABELS[value] ?? value.charAt(0).toUpperCase() + value.slice(1)
}

function effortLabel(value: string): string {
  return value === `xhigh` ? `XHigh` : modelLabel(value)
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
  skipPermissions,
  onSkipPermissionsChange,
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
  skipPermissions: boolean
  onSkipPermissionsChange: (value: boolean) => void
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
          <Label htmlFor="start-coding-device">Desktop</Label>
          <Select value={device?.deviceId ?? ``} onValueChange={onDeviceChange}>
            <SelectTrigger id="start-coding-device" className="w-full">
              <SelectValue placeholder="Select a desktop" />
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
      <div className="grid grid-cols-2 gap-3">
        <div className="space-y-2">
          <Label htmlFor="start-coding-model">Model</Label>
          <Select
            value={model === `` ? CLI_DEFAULT_MODEL : model}
            onValueChange={(value) =>
              onModelChange(value === CLI_DEFAULT_MODEL ? `` : value)
            }
          >
            <SelectTrigger id="start-coding-model" className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {agentAllowsBlankModel(agent) && (
                <SelectItem value={CLI_DEFAULT_MODEL}>CLI default</SelectItem>
              )}
              {agentModelValues(agent).map((value) => (
                <SelectItem key={value} value={value}>
                  {modelLabel(value)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-2">
          <Label htmlFor="start-coding-effort">
            {agent === `pi`
              ? `Thinking`
              : agent === `codex`
                ? `Reasoning`
                : `Effort`}
          </Label>
          <Select
            value={effortValue}
            onValueChange={onEffortChange}
            disabled={ultracode && agentSupportsUltracode(agent)}
          >
            <SelectTrigger id="start-coding-effort" className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={CLI_DEFAULT_EFFORT}>CLI default</SelectItem>
              {agentEffortValues(agent).map((value) => (
                <SelectItem key={value} value={value}>
                  {effortLabel(value)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>
      {(agentSupportsUltracode(agent) ||
        agentSupportsPlanMode(agent) ||
        agentSupportsSkipPermissions(agent)) && (
        <div className="space-y-2">
          {agentSupportsUltracode(agent) && (
            <div className="flex items-center gap-2">
              <Checkbox
                id="start-coding-ultracode"
                checked={ultracode}
                onCheckedChange={(value) => onUltracodeChange(value === true)}
              />
              <Label htmlFor="start-coding-ultracode" className="font-normal">
                Dynamic workflows (ultracode)
              </Label>
            </div>
          )}
          {agentSupportsPlanMode(agent) && (
            <div className="flex items-center gap-2">
              <Checkbox
                id="start-coding-plan-mode"
                checked={planMode}
                onCheckedChange={(value) => onPlanModeChange(value === true)}
              />
              <Label htmlFor="start-coding-plan-mode" className="font-normal">
                Plan mode
              </Label>
            </div>
          )}
          {agentSupportsSkipPermissions(agent) && (
            <div className="flex items-center gap-2">
              <Checkbox
                id="start-coding-skip-permissions"
                checked={skipPermissions}
                onCheckedChange={(value) =>
                  onSkipPermissionsChange(value === true)
                }
              />
              <Label
                htmlFor="start-coding-skip-permissions"
                className="font-normal"
              >
                Skip permissions
              </Label>
            </div>
          )}
        </div>
      )}
    </div>
  )
}
