import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  GlassGroup,
  GlassPickerRow,
  GlassToggleRow,
  type GlassPickerOption,
} from "@/components/ui/glass-rows"
import {
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
} from "@/components/mobile-popover"
import { Switch } from "@/components/ui/switch"
import {
  AGENT_LABELS,
  CLI_DEFAULT_EFFORT,
  CLI_DEFAULT_MODEL,
  effortLabel,
  modelLabel,
} from "@/components/launch-dialog/launch-options-pane"
import { McpServerPicker } from "@/components/launch-dialog/mcp-server-picker"
import type { LaunchComposerModel } from "@/hooks/use-launch-composer"
import { MAX_ISSUES_PER_RUN } from "@/hooks/use-launch-composer"
import {
  agentAllowsBlankModel,
  agentEffortValues,
  agentModelValues,
  agentSupportsPlanMode,
  agentSupportsUltracode,
} from "@/lib/coding-launch-prefs"
import { NO_REPO } from "@/lib/chat-repo"
import { conceptIcon } from "@/lib/icons.generated"

// EXP-825 (variant B, decided with Danny 2026-09-10): ONE muted line under the
// composer card — Device, Agent, Model as inline pickers, the Plan switch,
// the Resume switch when a worktree is resumable (EXP-481), the Repository
// picker ONLY while no subject is picked (a chat's anchor, EXP-822), and a
// `⋯` popover with the rest: Effort, Ultracode (claude), MCP servers
// (EXP-792), Account (the device's agent profiles, EXP-747 B7). The notes
// the dialog's option pane used to carry (not ready / no desktop / waiting
// / the batch guards) live on the same line.

const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const MoreIcon = conceptIcon(`ui-more`)

export function LaunchOptionsLine({ model }: { model: LaunchComposerModel }) {
  const { launch, candidateDevices, subject } = model
  const { agent, device } = launch

  if (candidateDevices.length === 0) {
    return (
      <p className="px-1 text-xs text-muted-foreground" data-testid="agent-options-row">
        No desktop online. Open the Exponential desktop app to start a run.
      </p>
    )
  }

  // Radix Select forbids an empty-string item value; the blank "CLI default"
  // model/effort rides the pane's sentinels inside the pickers only.
  const modelOptions = [
    ...(agentAllowsBlankModel(agent)
      ? [{ value: CLI_DEFAULT_MODEL, label: `CLI default` }]
      : []),
    ...agentModelValues(agent).map((value) => ({
      value,
      label: modelLabel(value),
    })),
  ]
  const effortOptions: GlassPickerOption[] = [
    { value: CLI_DEFAULT_EFFORT, label: `CLI default` },
    ...agentEffortValues(agent).map((value) => ({
      value,
      label: effortLabel(value),
    })),
  ]
  const showAccount = launch.accountProfiles.length >= 2
  const showMcp = model.mcpServers !== null && model.mcpServers.length > 0

  return (
    <div className="flex flex-col gap-1 px-1 text-xs text-muted-foreground">
      <div
        className="flex flex-wrap items-center gap-x-3 gap-y-1"
        data-testid="agent-options-row"
      >
        <InlinePicker
          label="Device"
          value={device?.deviceId ?? ``}
          options={candidateDevices.map((candidate) => ({
            value: candidate.deviceId,
            // EXP-432: teammates' shared servers carry their owner.
            label: `${candidate.deviceLabel || candidate.deviceId}${
              candidate.owner ? ` — ${candidate.owner.name}` : ``
            }`,
          }))}
          onChange={launch.setDeviceId}
        />
        <InlinePicker
          label="Agent"
          value={agent}
          options={launch.availableAgents.map((value) => ({
            value,
            label: AGENT_LABELS[value] ?? value,
          }))}
          onChange={launch.switchAgent}
        />
        <InlinePicker
          label="Model"
          value={launch.model === `` ? CLI_DEFAULT_MODEL : launch.model}
          options={modelOptions}
          onChange={(value) =>
            launch.setModel(value === CLI_DEFAULT_MODEL ? `` : value)
          }
        />
        {subject === null && model.repoOptions.length > 0 && (
          <InlinePicker
            label="Repository"
            value={model.repoId || NO_REPO}
            options={model.repoOptions}
            onChange={(value) => model.setRepoId(value === NO_REPO ? `` : value)}
          />
        )}
        {agentSupportsPlanMode(agent) && !model.resumeActive && (
          <label className="flex items-center gap-1.5">
            <span>Plan</span>
            <Switch
              size="sm"
              checked={launch.planMode}
              onCheckedChange={launch.setPlanMode}
              aria-label="Plan mode"
            />
          </label>
        )}
        {model.resumeCandidate && (
          /* EXP-481: a resumed session never re-enters plan mode, so the Plan
             switch hides behind an armed resume (the desktop's clamp). */
          <label
            className="flex items-center gap-1.5"
            title={`A worktree for ${model.resumeCandidate.identifier} already exists (${model.resumeCandidate.branch}).`}
          >
            <span>Resume</span>
            <Switch
              size="sm"
              checked={model.resume}
              onCheckedChange={model.setResume}
              aria-label="Resume previous session"
            />
          </label>
        )}
        <MobilePopover>
          <MobilePopoverTrigger asChild>
            <button
              type="button"
              className="flex items-center outline-none hover:text-foreground focus-visible:text-foreground"
              title="More options"
              aria-label="More options"
            >
              <MoreIcon className="size-3.5" />
            </button>
          </MobilePopoverTrigger>
          <MobilePopoverContent
            className="w-[20rem] p-2"
            align="start"
            mobileTitle="Options"
          >
            <div data-testid="agent-options-sheet">
              <GlassGroup>
                <GlassPickerRow
                  label={agent === `codex` ? `Reasoning` : `Effort`}
                  value={
                    launch.effortValue === `` ? CLI_DEFAULT_EFFORT : launch.effortValue
                  }
                  onValueChange={launch.setEffortValue}
                  options={effortOptions}
                  disabled={launch.ultracode && agentSupportsUltracode(agent)}
                />
                {agentSupportsUltracode(agent) && (
                  <GlassToggleRow
                    id="agent-composer-ultracode"
                    label="Ultracode"
                    checked={launch.ultracode}
                    onCheckedChange={launch.setUltracode}
                  />
                )}
                {showMcp && (
                  /* EXP-792: WHICH team servers the run connects to — the
                     picker greys rows the machine is not ready for. */
                  <div className="flex items-center gap-3 px-4 py-3">
                    <span className="flex-1 text-sm text-foreground">
                      MCP servers
                    </span>
                    <McpServerPicker
                      servers={model.mcpServers!}
                      selectedIds={launch.mcpServerIds}
                      onToggle={launch.toggleMcpServer}
                      device={device}
                      now={model.mcpNow}
                    />
                  </div>
                )}
                {showAccount && (
                  <GlassPickerRow
                    label="Account"
                    value={launch.account ?? ``}
                    onValueChange={launch.setAccount}
                    placeholder="Active profile"
                    options={launch.accountProfiles.map((profile) => ({
                      value: profile.id,
                      label: profile.active
                        ? `${profile.label} (active)`
                        : profile.label,
                    }))}
                  />
                )}
              </GlassGroup>
            </div>
          </MobilePopoverContent>
        </MobilePopover>
        {launch.agentNotReady && device && (
          /* EXP-773: with the PTY path gone this combination cannot start at
             all — the submit is disabled on the same predicate. */
          <span>
            {`Not ready on ${device.deviceLabel || device.deviceId}. Run the doctor there.`}
          </span>
        )}
        {/* EXP-836: a play button named a machine this composer cannot start
            on — say which and why, instead of quietly using the default. */}
        {model.deviceRequestNote && (
          <span className="text-amber-500">{model.deviceRequestNote}</span>
        )}
        {/* The desktop inserts the row when the launcher spins up; the page
            flips to the live view the moment it syncs. */}
        {model.sentTo && <span>{`Waiting for ${model.sentTo}…`}</span>}
      </div>
      {model.overCap && (
        <p className="text-destructive">
          {`At most ${MAX_ISSUES_PER_RUN} issues per run. Split the batch.`}
        </p>
      )}
      {model.spansRepos && (
        <p className="text-destructive">
          Pick issues from a single repository per run.
        </p>
      )}
      {model.spansBranches && (
        <p className="text-destructive">
          Pick issues that branch from a single base branch per run.
        </p>
      )}
      {model.costHint && <p>Large batches are token-expensive.</p>}
    </div>
  )
}

/** A picker as one word of the muted line under the composer: the current
 * value plus a chevron, no chrome. One option renders as plain text. */
export function InlinePicker({
  label,
  value,
  options,
  onChange,
}: {
  label: string
  value: string
  options: { value: string; label: string }[]
  onChange: (value: string) => void
}) {
  if (options.length === 0) return null
  const current = options.find((option) => option.value === value)
  if (options.length === 1) {
    return <span title={label}>{current?.label ?? options[0]!.label}</span>
  }
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        className="flex items-center gap-0.5 outline-none hover:text-foreground focus-visible:text-foreground"
        title={label}
        aria-label={label}
      >
        {current?.label ?? label}
        <ChevronDownIcon className="size-3" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        {options.map((option) => (
          <DropdownMenuItem
            key={option.value}
            onSelect={() => onChange(option.value)}
          >
            {option.label}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
