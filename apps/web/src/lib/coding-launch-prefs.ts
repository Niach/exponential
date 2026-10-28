// Start-coding launch option vocabulary + seeding (EXP-149; agent-aware
// since EXP-201). EXP-437 removed the localStorage last-used persistence:
// the dialog seeds from the SELECTED DEVICE's advertised per-agent defaults
// (`SteerDevice.launchDefaults`), falling back to static contract defaults
// when the device advertises nothing.
//
// Seed values are validated against the domain contract, so a device
// advertisement from an older/newer build can never seed the dialog with a
// value the server's zod checks would reject.

import { contract } from "@exp/domain-contract"

export interface CodingLaunchPrefs {
  /** Coding agent CLI (`claude`/`codex`) — EXP-201. */
  agent: string
  model: string
  /** EXP-981: claude only — the model the run's SUBAGENTS get (the device
   * sets it as the CLI's env at launch). Always explicit (contract
   * `deviceAgentDefaults.subagentModel` when the device names none). */
  subagentModel?: string
  /** The dispatcher root's model (threads + team agents), per agent; a
   * device setting, never a launch-payload field. */
  threadModel?: string
  /** `""` = "CLI default" (omit the effort flag) — a valid value. */
  effort: string
  ultracode: boolean
  /** EXP-481: resume the issue's existing worktree/agent session — SINGLE
   * issue starts only (the batch arm strips it before the mutation). */
  resume?: boolean
  /** EXP-792: the team MCP servers the run connects to (row ids). Omitted
   * when nothing is picked; the server refuses ids outside the team. */
  mcpServerIds?: string[]
  /** EXP-1249: computer use for THIS run (the composer's "+" toggle);
   * omitted = the device's `launch_defaults.computerUse`. Only sent to a
   * device advertising `contract.codingSession.computerUseCap`. */
  computerUse?: boolean
  /** EXP-825 (EXP-747 B7): the agent account PROFILE the run launches on —
   * one of the device's reported `agentAccounts[agent].profiles` ids.
   * Omitted = the machine's last used login. */
  account?: string
}

// EXP-792: the MCP server pick IS persisted, unlike model/effort (which the
// device advertises): a machine knows nothing about which team servers a
// person wants on a run, so the last pick per TEAM is the only sensible seed
// after the team's `enabledByDefault` rows. localStorage, tolerant of
// anything stored by an older build.
const MCP_PICK_KEY = `exp:mcp-server-pick:v1`

function readMcpPicks(): Record<string, string[]> {
  if (typeof localStorage === `undefined`) return {}
  try {
    const raw = localStorage.getItem(MCP_PICK_KEY)
    if (!raw) return {}
    const parsed: unknown = JSON.parse(raw)
    if (typeof parsed !== `object` || parsed === null || Array.isArray(parsed)) {
      return {}
    }
    const out: Record<string, string[]> = {}
    for (const [teamId, ids] of Object.entries(parsed)) {
      if (Array.isArray(ids)) {
        out[teamId] = ids.filter((id): id is string => typeof id === `string`)
      }
    }
    return out
  } catch {
    return {}
  }
}

/** The last MCP server pick for `teamId`, or null when none was ever saved
 * (the caller then seeds from the team's `enabledByDefault` rows). */
export function loadMcpServerPick(teamId: string): string[] | null {
  return readMcpPicks()[teamId] ?? null
}

export function saveMcpServerPick(teamId: string, ids: string[]): void {
  if (typeof localStorage === `undefined`) return
  try {
    const picks = readMcpPicks()
    picks[teamId] = [...ids]
    localStorage.setItem(MCP_PICK_KEY, JSON.stringify(picks))
  } catch {
    // Quota or a privacy mode: the pick just does not survive a reload.
  }
}

/** EXP-437: one agent's launch defaults as a device advertises them. Every
 * model is explicit (an absent or blank one reads as the contract default);
 * a blank `effort` = "CLI default / omit the flag"; absent booleans = false
 * (the desktop skip-serializes false). */
export interface AgentLaunchDefaults {
  model?: string
  /** EXP-981: claude only. */
  subagentModel?: string
  /** The dispatcher root's model (threads + team agents). */
  threadModel?: string
  effort?: string
  ultracode?: boolean
  /** EXP-1005: claude only; absent = ON. */
  autoRotateAccounts?: boolean
}

/** The model values pickable for `agent` (EXP-201). Every model is explicit:
 * no blank "CLI default" choice, codex included. */
export function agentModelValues(agent: string): readonly string[] {
  switch (agent) {
    case `codex`:
      return contract.codexModel.values
    default:
      return contract.codingModel.values
  }
}

/** The effort/reasoning/thinking values for `agent` (blank = CLI default). */
export function agentEffortValues(agent: string): readonly string[] {
  switch (agent) {
    case `codex`:
      return contract.codexEffort.values
    default:
      return contract.codingEffort.values
  }
}

/** Ultracode and plan mode are Claude-only (EXP-849 dropped pi; S-3: the
 * UI has no plan switch, the server still validates a device's default with
 * this). EXP-690
 * retired the skip-permissions choice: every launch bypasses the agent's
 * permission prompts. */
export function agentSupportsUltracode(agent: string): boolean {
  return agent === `claude`
}

export function agentSupportsPlanMode(agent: string): boolean {
  return agent === `claude`
}

/** EXP-1005: auto-rotating accounts is a claude-only device setting (codex
 * keeps one login per session by contract). */
export function agentSupportsAccountRotation(agent: string): boolean {
  return agent === `claude`
}

/** EXP-981: the subagent model is a claude CLI env var — nothing else reads
 * it, so the picker hides for every other agent (the ultracode rule). */
export function agentSupportsSubagentModel(agent: string): boolean {
  return agent === `claude`
}

/** The default model for `agent` — contract `deviceAgentDefaults` (claude
 * `fable`, codex `gpt-5.6-sol`). */
export function defaultModelFor(agent: string): string {
  return agent === `codex`
    ? contract.deviceAgentDefaults.codexModel
    : contract.deviceAgentDefaults.model
}

/** The default dispatcher (thread) model for `agent` — contract
 * `deviceAgentDefaults` (claude `haiku`, codex `gpt-5.6-luna`). */
export function defaultThreadModelFor(agent: string): string {
  return agent === `codex`
    ? contract.deviceAgentDefaults.codexThreadModel
    : contract.deviceAgentDefaults.threadModel
}

/** The default subagent model (claude only): contract `opus`. */
export const DEFAULT_SUBAGENT_MODEL: string = contract.deviceAgentDefaults.subagentModel

/** The static default agent when no device advertises one. */
export const DEFAULT_LAUNCH_AGENT: string = contract.codingAgent.values[0]

/**
 * EXP-437: the dialog's option seed for `agent` — the device's advertised
 * defaults for that agent, contract-validated and capability-clamped, with
 * static defaults for anything absent/invalid. `agentSeed(agent, null)` is
 * the static fallback (first model / CLI-default effort / toggles off) used
 * when the selected device advertises nothing.
 */
export function agentSeed(
  agent: string,
  defaults: AgentLaunchDefaults | null | undefined
): Omit<CodingLaunchPrefs, `agent`> {
  const models = agentModelValues(agent)
  const efforts = agentEffortValues(agent)
  const model =
    typeof defaults?.model === `string` && models.includes(defaults.model)
      ? defaults.model
      : defaultModelFor(agent)
  const threadModel =
    typeof defaults?.threadModel === `string` && models.includes(defaults.threadModel)
      ? defaults.threadModel
      : defaultThreadModelFor(agent)
  const effort =
    typeof defaults?.effort === `string` &&
    (defaults.effort === `` || efforts.includes(defaults.effort))
      ? defaults.effort
      : ``
  // EXP-981: claude's subagent model is always one of ITS models (never the
  // picked agent's), explicit (contract default `opus`); other agents none.
  const subagentModel = !agentSupportsSubagentModel(agent)
    ? ``
    : typeof defaults?.subagentModel === `string` &&
        contract.codingModel.values.includes(defaults.subagentModel)
      ? defaults.subagentModel
      : DEFAULT_SUBAGENT_MODEL
  return {
    model,
    subagentModel,
    threadModel,
    effort,
    ultracode: (defaults?.ultracode ?? false) && agentSupportsUltracode(agent),
  }
}
