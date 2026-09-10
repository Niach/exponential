// Start-coding launch option vocabulary + seeding (EXP-149; agent-aware
// since EXP-201). EXP-437 removed the localStorage last-used persistence:
// the dialog seeds from the SELECTED DEVICE's advertised per-agent defaults
// (`SteerDevice.launchDefaults`), falling back to static contract defaults
// when the device advertises nothing (older desktop build).
//
// Seed values are validated against the domain contract, so a device
// advertisement from an older/newer build can never seed the dialog with a
// value the server's zod checks would reject.

import { contract } from "@exp/domain-contract"

export interface CodingLaunchPrefs {
  /** Coding agent CLI (`claude`/`codex`/`pi`) — EXP-201. */
  agent: string
  model: string
  /** `""` = "CLI default" (omit the effort flag) — a valid value. */
  effort: string
  ultracode: boolean
  planMode: boolean
  /** EXP-481: resume the issue's existing worktree/agent session — SINGLE
   * issue starts only (the batch arm strips it before the mutation). */
  resume?: boolean
  /** EXP-792: the team MCP servers the run connects to (row ids). Omitted
   * when nothing is picked; the server refuses ids outside the team. */
  mcpServerIds?: string[]
  /** EXP-825 (EXP-747 B7): the agent account PROFILE the run launches on —
   * one of the device's reported `agentAccounts[agent].profiles` ids.
   * Omitted for the machine's ambient login (`SYSTEM_PROFILE_ID`). */
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

/** EXP-437: one agent's launch defaults as a device advertises them. Blank
 * `model`/`effort` = "CLI default / omit the flag"; absent booleans = false
 * (the desktop skip-serializes false). */
export interface AgentLaunchDefaults {
  model?: string
  effort?: string
  ultracode?: boolean
  planMode?: boolean
}

/** The model values pickable for `agent` (EXP-201). Blank ("CLI default") is
 * an extra valid choice for codex/pi; claude is explicit-always. */
export function agentModelValues(agent: string): readonly string[] {
  switch (agent) {
    case `codex`:
      return contract.codexModel.values
    case `pi`:
      return contract.piModel.values
    default:
      return contract.codingModel.values
  }
}

/** The effort/reasoning/thinking values for `agent` (blank = CLI default). */
export function agentEffortValues(agent: string): readonly string[] {
  switch (agent) {
    case `codex`:
      return contract.codexEffort.values
    case `pi`:
      return contract.piThinking.values
    default:
      return contract.codingEffort.values
  }
}

/** Whether a blank model (omit the flag) is valid for `agent`. */
export function agentAllowsBlankModel(agent: string): boolean {
  return agent !== `claude`
}

/** Ultracode is Claude-only; plan mode is claude + pi (EXP-441: pi via the
 * launcher-injected extension). EXP-690 retired the skip-permissions choice:
 * every launch bypasses the agent's permission prompts. */
export function agentSupportsUltracode(agent: string): boolean {
  return agent === `claude`
}

export function agentSupportsPlanMode(agent: string): boolean {
  return agent === `claude` || agent === `pi`
}

/** The default model choice for `agent` — first contract value for claude
 * (explicit-always), blank "CLI default" for codex/pi. */
export function defaultModelFor(agent: string): string {
  return agentAllowsBlankModel(agent) ? `` : contract.codingModel.values[0]
}

/** The static default agent when no device advertises one. */
export const DEFAULT_LAUNCH_AGENT: string = contract.codingAgent.values[0]

/**
 * EXP-437: the dialog's option seed for `agent` — the device's advertised
 * defaults for that agent, contract-validated and capability-clamped, with
 * static defaults for anything absent/invalid. `agentSeed(agent, null)` is
 * the static fallback (first model / CLI-default effort / toggles off) used
 * when the selected device advertises nothing (older desktop build).
 */
export function agentSeed(
  agent: string,
  defaults: AgentLaunchDefaults | null | undefined
): Omit<CodingLaunchPrefs, `agent`> {
  const models = agentModelValues(agent)
  const efforts = agentEffortValues(agent)
  const model =
    typeof defaults?.model === `string` &&
    (models.includes(defaults.model) ||
      (defaults.model === `` && agentAllowsBlankModel(agent)))
      ? defaults.model
      : defaultModelFor(agent)
  const effort =
    typeof defaults?.effort === `string` &&
    (defaults.effort === `` || efforts.includes(defaults.effort))
      ? defaults.effort
      : ``
  return {
    model,
    effort,
    ultracode: (defaults?.ultracode ?? false) && agentSupportsUltracode(agent),
    planMode: (defaults?.planMode ?? false) && agentSupportsPlanMode(agent),
  }
}
