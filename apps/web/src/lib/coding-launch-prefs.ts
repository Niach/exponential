// Last-used Start-coding dialog options (EXP-149; agent-aware since EXP-201).
// Per-device via guarded localStorage (`safeLocalStorage` — degrades to "no
// persistence").
//
// Values are validated against the domain contract on read, so a stale entry
// from an older build can never seed the dialog with a value the server's
// zod checks would reject.

import { contract } from "@exp/domain-contract"
import { safeLocalStorage } from "@/lib/local-storage"

export interface CodingLaunchPrefs {
  /** Coding agent CLI (`claude`/`codex`/`pi`) — EXP-201. */
  agent: string
  model: string
  /** `""` = "CLI default" (omit the effort flag) — a valid stored value. */
  effort: string
  ultracode: boolean
  planMode: boolean
  /** Full permission bypass instead of the agent's guarded auto mode. */
  skipPermissions: boolean
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

/** Ultracode + plan mode are Claude-only; pi has no permission system. */
export function agentSupportsUltracode(agent: string): boolean {
  return agent === `claude`
}

export function agentSupportsPlanMode(agent: string): boolean {
  return agent === `claude`
}

export function agentSupportsSkipPermissions(agent: string): boolean {
  return agent !== `pi`
}

/** The default model choice for `agent` — first contract value for claude
 * (explicit-always), blank "CLI default" for codex/pi. */
export function defaultModelFor(agent: string): string {
  return agentAllowsBlankModel(agent) ? `` : contract.codingModel.values[0]
}

export const DEFAULT_CODING_LAUNCH_PREFS: CodingLaunchPrefs = {
  agent: contract.codingAgent.values[0],
  model: contract.codingModel.values[0],
  effort: ``,
  ultracode: false,
  // Plan mode defaults OFF for remote starts: the session runs on an
  // unattended desktop, and only web steer can approve a plan remotely.
  planMode: false,
  skipPermissions: false,
}

const STORAGE_KEY = `exp.codingLaunchOptions`

export function readCodingLaunchPrefs(): CodingLaunchPrefs {
  const store = safeLocalStorage()
  if (!store) return DEFAULT_CODING_LAUNCH_PREFS
  try {
    const raw = store.getItem(STORAGE_KEY)
    if (!raw) return DEFAULT_CODING_LAUNCH_PREFS
    const parsed: unknown = JSON.parse(raw)
    if (typeof parsed !== `object` || parsed === null) {
      return DEFAULT_CODING_LAUNCH_PREFS
    }
    const record = parsed as Record<string, unknown>
    const agent =
      typeof record.agent === `string` &&
      contract.codingAgent.values.includes(record.agent)
        ? record.agent
        : DEFAULT_CODING_LAUNCH_PREFS.agent
    const models = agentModelValues(agent)
    const efforts = agentEffortValues(agent)
    return {
      agent,
      model:
        typeof record.model === `string` &&
        (models.includes(record.model) ||
          (record.model === `` && agentAllowsBlankModel(agent)))
          ? record.model
          : defaultModelFor(agent),
      effort:
        typeof record.effort === `string` &&
        (record.effort === `` || efforts.includes(record.effort))
          ? record.effort
          : DEFAULT_CODING_LAUNCH_PREFS.effort,
      ultracode:
        typeof record.ultracode === `boolean` && agentSupportsUltracode(agent)
          ? record.ultracode
          : DEFAULT_CODING_LAUNCH_PREFS.ultracode,
      planMode:
        typeof record.planMode === `boolean` && agentSupportsPlanMode(agent)
          ? record.planMode
          : DEFAULT_CODING_LAUNCH_PREFS.planMode,
      skipPermissions:
        typeof record.skipPermissions === `boolean` &&
        agentSupportsSkipPermissions(agent)
          ? record.skipPermissions
          : DEFAULT_CODING_LAUNCH_PREFS.skipPermissions,
    }
  } catch {
    return DEFAULT_CODING_LAUNCH_PREFS
  }
}

export function rememberCodingLaunchPrefs(prefs: CodingLaunchPrefs): void {
  const store = safeLocalStorage()
  if (!store) return
  try {
    store.setItem(STORAGE_KEY, JSON.stringify(prefs))
  } catch {
    // Quota/privacy failures just mean no persistence — never block a start.
  }
}
