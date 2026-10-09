import {
  AccountPicker,
  DevicePicker,
  Label,
  ModelPicker,
  RepositoryPicker,
  Switch,
  deviceReadinessBlocker,
} from "@exp/ui"
import { DeviceReadinessNotice } from "@/components/device-readiness-notice"
import {
  CLI_DEFAULT_MODEL,
  modelLabel,
} from "@/components/launch-dialog/launch-options-pane"
import type { LaunchComposerModel } from "@/hooks/use-launch-composer"
import { MAX_ISSUES_PER_RUN } from "@/hooks/use-launch-composer"
import {
  agentAllowsBlankModel,
  agentModelValues,
  agentSupportsPlanMode,
} from "@/lib/coding-launch-prefs"
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
// brand mark + email, the last used one first — picking a login implies
// its agent. The Device menu rows carry the machine's kind glyph like its
// trigger. The repository picker renders only while the team has SEVERAL
// repos (one repo is not a choice; there is no repo-less option any more),
// and the `⋯` overlay is the divided-rows shell with no card inside the
// popover's card (EXP-993/994).
//
// EXP-958 → the UI cleanup batch: every word of the line is a typed picker
// (`ModelPicker`, `RepositoryPicker` …) in its `inline` variant — the
// primitive collapses a picker to plain text on its own once there is at
// most one thing it could say, so the line has no picker component of its
// own. None of these four ever reaches ZERO options: the whole line
// returns early without a device, Repository and Account render behind a
// count guard, and the model list is contract values plus the blank default.
//
// EXP-1249: the `⋯` overflow is gone — Effort, Subagents, Ultracode, MCP
// servers and the new per-run Computer use live in the composer's ONE "+"
// menu (`composer-plus-menu.tsx`). The line is Device · Account · Model ·
// Repository · Plan · Resume and nothing else.

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
  const accountOptions = launch.accountOptions.map((option) => ({
    key: accountOptionKey(option),
    agent: option.agent,
    email: option.email,
    // EXP-849: health beats everything else in the row — an expired
    // credential is the one thing worth knowing BEFORE the run starts on it.
    hint: healthBadgeLabel(option.health) ?? undefined,
    limits: option.limits,
  }))
  // EXP-1030: the machines as THE device picker's rows (kind glyph + name,
  // EXP-432's owner suffix on a teammate's shared server).
  const deviceRows = candidateDevices.map((candidate) => ({
    id: candidate.deviceId,
    name: `${candidate.deviceLabel || candidate.deviceId}${
      candidate.owner ? ` — ${candidate.owner.name}` : ``
    }`,
    icon: candidate.icon,
    kind: candidate.kind,
  }))

  return (
    <div className="flex flex-col gap-1 px-1 text-xs text-muted-foreground">
      <div
        className="flex flex-wrap items-center gap-x-3 gap-y-1"
        data-testid="agent-options-row"
      >
        {/* One machine is not a choice: the inline word collapses to
            plain text on its own (the primitive's `inline` variant). */}
        <DevicePicker
          triggerVariant="inline"
          mobileTitle="Device"
          value={device?.deviceId ?? null}
          devices={deviceRows}
          onChange={launch.setDeviceId}
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
        <ModelPicker
          value={launch.model === `` ? CLI_DEFAULT_MODEL : launch.model}
          models={modelOptions}
          onChange={(value) =>
            launch.setModel(value === CLI_DEFAULT_MODEL ? `` : value)
          }
        />
        {subject === null && model.repoOptions.length > 1 && (
          /* EXP-993: a choice only when there IS one — several repos. One
             repo is the chat's anchor without a word said, and repo-less is
             not on offer. */
          <RepositoryPicker
            triggerVariant="inline"
            width="sm"
            value={model.repoId || null}
            repositories={model.repoOptions.map((option) => ({
              id: option.value,
              fullName: option.label,
            }))}
            onChange={model.setRepoId}
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
              aria-label="Resume previous run"
            />
          </Label>
        )}
        {/* EXP-773: a not-ready combination cannot start at all — the
            submit is disabled on the same predicate. EXP-1196: with a doctor
            report the failing ROW renders under the line instead. */}
        {launch.agentNotReady &&
          device &&
          !deviceReadinessBlocker(device.doctor, agent) && (
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
      {/* EXP-1196: the picked device's failing doctor row (Git, else the
          agent's own) with its one action, instead of a sentence. */}
      {device && deviceReadinessBlocker(device.doctor, agent) && (
        <DeviceReadinessNotice
          device={device}
          agent={agent}
          notReady={launch.agentNotReady}
          className="rounded-md bg-glass-row"
        />
      )}
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
