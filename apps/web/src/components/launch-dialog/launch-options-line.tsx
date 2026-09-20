import {
  AccountPicker,
  Button,
  Combobox,
  GlassGroup,
  GlassToggleRow,
  type PickerOption,
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
  Label,
  Switch,
  conceptIcon,
  getDeviceIcon,
} from "@exp/ui"
import {
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
  agentSupportsSubagentModel,
  agentSupportsUltracode,
} from "@/lib/coding-launch-prefs"
import { contract } from "@exp/domain-contract"
import { healthBadgeLabel } from "@/lib/agent-usage"
import { accountOptionKey } from "@/lib/accounts/account-option"

// EXP-825 (variant B, decided with Danny 2026-09-10): ONE muted line under the
// composer card — Device, Agent, Model as inline pickers, the Plan switch,
// the Resume switch when a worktree is resumable (EXP-481), the Repository
// picker ONLY while no subject is picked (a chat's anchor, EXP-822), and a
// `⋯` popover with the rest: Effort, Ultracode (claude), MCP servers
// (EXP-792). The notes the dialog's option pane used to carry (not ready /
// no desktop / waiting / the batch guards) live on the same line.
//
// EXP-872: the Agent pick and the Account pick MERGED into THE account picker
// (@exp/ui `account-picker`): one flattened list of the machine's logins,
// brand mark + email, the device default first — picking a login implies
// its agent. The Device menu rows carry the machine's kind glyph like its
// trigger. The repository picker renders only while the team has SEVERAL
// repos (one repo is not a choice; there is no repo-less option any more),
// and the `⋯` overlay is the divided-rows shell with no card inside the
// popover's card (EXP-993/994).
//
// EXP-958: every word of the line is a `Combobox` in its `inline` variant —
// the primitive collapses a picker to plain text on its own once there is at
// most one thing it could say, so the line has no picker component of its
// own any more. None of these four ever reaches ZERO options: the whole line
// returns early without a device, Repository and Account render behind a
// count guard, and the model list is contract values plus the blank default.

const MoreIcon = conceptIcon(`ui-more`)
// EXP-862: a picker whose VALUE carries a glyph carries it on the menu rows
// too — here the machine's kind, the same pair the Devices list draws.

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

  // An empty string is no usable option identity; the blank "CLI default"
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
  const effortOptions: PickerOption[] = [
    { value: CLI_DEFAULT_EFFORT, label: `CLI default` },
    ...agentEffortValues(agent).map((value) => ({
      value,
      label: effortLabel(value),
    })),
  ]
  const accountOptions = launch.accountOptions.map((option) => ({
    key: accountOptionKey(option),
    agent: option.agent,
    email: option.email,
    // EXP-849: health beats everything else in the row — an expired
    // credential is the one thing worth knowing BEFORE the run starts on it.
    hint: healthBadgeLabel(option.health) ?? undefined,
    limits: option.limits,
  }))
  const showMcp = model.mcpServers !== null && model.mcpServers.length > 0

  return (
    <div className="flex flex-col gap-1 px-1 text-xs text-muted-foreground">
      <div
        className="flex flex-wrap items-center gap-x-3 gap-y-1"
        data-testid="agent-options-row"
      >
        <Combobox
          triggerVariant="inline"
          searchable={false}
          mobileTitle="Device"
          value={device?.deviceId ?? null}
          options={candidateDevices.map((candidate) => ({
            value: candidate.deviceId,
            // EXP-432: teammates' shared servers carry their owner.
            label: `${candidate.deviceLabel || candidate.deviceId}${
              candidate.owner ? ` — ${candidate.owner.name}` : ``
            }`,
            icon: getDeviceIcon(candidate),
          }))}
          onChange={(value) => {
            if (value !== null) launch.setDeviceId(value)
          }}
          width="sm"
        />
        {/* EXP-872: THE account picker — brand mark + email, the agent
            implied by the pick; one login collapses to plain text. */}
        <AccountPicker
          variant="inline"
          value={launch.accountKey ?? null}
          options={accountOptions}
          onChange={launch.setAccountKey}
          data-testid="agent-composer-account"
        />
        <Combobox
          triggerVariant="inline"
          searchable={false}
          mobileTitle="Model"
          value={launch.model === `` ? CLI_DEFAULT_MODEL : launch.model}
          options={modelOptions}
          onChange={(value) => {
            if (value !== null) {
              launch.setModel(value === CLI_DEFAULT_MODEL ? `` : value)
            }
          }}
          width="sm"
        />
        {subject === null && model.repoOptions.length > 1 && (
          /* EXP-993: a choice only when there IS one — several repos. One
             repo is the chat's anchor without a word said, and repo-less is
             not on offer. */
          <Combobox
            triggerVariant="inline"
            searchable={false}
            mobileTitle="Repository"
            value={model.repoId || null}
            options={model.repoOptions}
            onChange={(value) => {
              if (value !== null) model.setRepoId(value)
            }}
            width="sm"
          />
        )}
        {agentSupportsPlanMode(agent) && !model.resumeActive && (
          <Label className="cursor-pointer gap-1.5 font-normal">
            <span>Plan</span>
            <Switch
              size="sm"
              checked={launch.planMode}
              onCheckedChange={launch.setPlanMode}
              aria-label="Plan mode"
            />
          </Label>
        )}
        {model.resumeCandidate && (
          /* EXP-481: a resumed session never re-enters plan mode, so the Plan
             switch hides behind an armed resume (the desktop's clamp). */
          <Label
            className="gap-1.5 font-normal"
            title={`A worktree for ${model.resumeCandidate.identifier} already exists (${model.resumeCandidate.branch}).`}
          >
            <span>Resume</span>
            <Switch
              size="sm"
              checked={model.resume}
              onCheckedChange={model.setResume}
              aria-label="Resume previous session"
            />
          </Label>
        )}
        <MobilePopover>
          <MobilePopoverTrigger asChild>
            {/* EXP-862: a secondary icon button is GHOST — no circle, no
                border, a hover wash and a pointer. */}
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              className="-my-0.5 text-muted-foreground hover:text-foreground"
              title="More options"
              aria-label="More options"
            >
              <MoreIcon className="size-3.5" />
            </Button>
          </MobilePopoverTrigger>
          <MobilePopoverContent
            className="w-[20rem] p-0"
            align="start"
            mobileTitle="Options"
          >
            {/* EXP-994: the overlay IS the surface — the rows keep their
                hairlines and nothing draws a second card inside it. */}
            <div data-testid="agent-options-sheet">
              <GlassGroup bare>
                <Combobox
                  triggerVariant="row"
                  searchable={false}
                  mobileTitle={agent === `codex` ? `Reasoning` : `Effort`}
                  value={
                    launch.effortValue === `` ? CLI_DEFAULT_EFFORT : launch.effortValue
                  }
                  onChange={(value) => {
                    if (value !== null) launch.setEffortValue(value)
                  }}
                  options={effortOptions}
                  disabled={launch.ultracode && agentSupportsUltracode(agent)}
                />
                {agentSupportsSubagentModel(agent) && (
                  /* EXP-981: the model the run's SUBAGENTS get — claude only
                     (it is that CLI's env var), blank = the CLI's own
                     default, and then it never reaches the start payload. */
                  <Combobox
                    triggerVariant="row"
                    searchable={false}
                    mobileTitle="Subagent model"
                    value={
                      launch.subagentModel === ``
                        ? CLI_DEFAULT_MODEL
                        : launch.subagentModel
                    }
                    onChange={(value) => {
                      if (value !== null) {
                        launch.setSubagentModel(
                          value === CLI_DEFAULT_MODEL ? `` : value
                        )
                      }
                    }}
                    options={[
                      { value: CLI_DEFAULT_MODEL, label: `Default` },
                      ...contract.codingModel.values.map((value) => ({
                        value,
                        label: modelLabel(value),
                      })),
                    ]}
                  />
                )}
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
