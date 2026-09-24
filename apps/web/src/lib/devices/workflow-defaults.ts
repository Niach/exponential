import { contract } from "@exp/domain-contract"
import type { DeviceWorkflowDefaults } from "@exp/db-schema/domain"

import { agentModelValues } from "@/lib/coding-launch-prefs"

/**
 * EXP-1020: a device's WORKFLOW model defaults, resolved for the agent its
 * default account implies — the pair the "Workflow settings" sub-shell edits
 * and a new workflow is seeded from (`launch_defaults.workflow`,
 * `DeviceWorkflowDefaults`).
 *
 * `model` is the cheap one (leaf nodes and the subagents inside them),
 * `strongModel` the expensive one (contract, integration and `risk: high`
 * nodes, and every agent review).
 *
 * The two vocabularies do not overlap, so a stored name only counts for the
 * agent it belongs to: a device that was on claude and moved to codex reads
 * as codex's defaults rather than showing `opus` in a codex picker.
 */
export function workflowDefaultsFor(
  agent: string,
  stored: Partial<DeviceWorkflowDefaults> | null | undefined
): DeviceWorkflowDefaults {
  const fallback = workflowFallbackFor(agent)
  const models = agentModelValues(agent)
  const pick = (value: unknown, otherwise: string) =>
    typeof value === `string` && models.includes(value) ? value : otherwise
  return {
    model: pick(stored?.model, fallback.model),
    strongModel: pick(stored?.strongModel, fallback.strongModel),
  }
}

/** Contract `workflowLaunch`'s per-agent pair; claude's for anything else. */
export function workflowFallbackFor(agent: string): DeviceWorkflowDefaults {
  return agent === `codex`
    ? {
        model: contract.workflowLaunch.codexModel,
        strongModel: contract.workflowLaunch.codexStrongModel,
      }
    : {
        model: contract.workflowLaunch.claudeModel,
        strongModel: contract.workflowLaunch.claudeStrongModel,
      }
}

/** The sub-shell row's trailing summary: `opus · fable`. */
export function workflowDefaultsSummary(defaults: DeviceWorkflowDefaults): string {
  return `${defaults.model} · ${defaults.strongModel}`
}
