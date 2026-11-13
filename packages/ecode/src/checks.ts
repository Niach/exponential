// eCode's checks (fixtures/checks.json is the spec): the rollup over a PR's
// check rows and the merge gate the required checks give. The same rules as
// domain-contract's pr-checks.ts (#EXP-1610), which Exponential's clients
// use; apps/web/src/lib/ecode/ecode-spec.test.ts replays this fixture on both.

import { isFailingConclusion, type CheckConclusion, type ChecksState, type MergeGate } from "./contract.generated"

export interface CheckRow {
  name: string
  status: string
  conclusion: string | null
}

export function isFailing(check: CheckRow): boolean {
  return check.status === `completed` && check.conclusion !== null && isFailingConclusion(check.conclusion as CheckConclusion)
}

/** none (no rows) · failure (any red; wins over pending) · pending (any not completed) · success. */
export function rollup(checks: readonly CheckRow[]): ChecksState {
  if (checks.length === 0) return `none`
  if (checks.some(isFailing)) return `failure`
  if (checks.some((check) => check.status !== `completed`)) return `pending`
  return `success`
}

export interface GateResult {
  gate: MergeGate
  failing: string[]
  pending: string[]
  missing: string[]
}

/** The merge gate over the REQUIRED names (none named = every row gates).
 *  A required name without a row is missing (= waiting). */
export function mergeGate(checks: readonly CheckRow[], required: readonly string[]): GateResult {
  const names = [...new Set(required)]
  const gated = names.length > 0 ? checks.filter((check) => names.includes(check.name)) : [...checks]
  const failing = gated.filter(isFailing).map((check) => check.name)
  const pending = gated.filter((check) => check.status !== `completed`).map((check) => check.name)
  const missing = names.filter((name) => !checks.some((check) => check.name === name))
  let gate: MergeGate
  if (failing.length > 0) gate = `blocked`
  else if (names.length === 0 && gated.length === 0) gate = `no_checks`
  else if (pending.length > 0 || missing.length > 0) gate = `waiting`
  else gate = `ready`
  return { gate, failing, pending, missing }
}

/** A job's check name: its name, plus the matrix values in parentheses. */
export function checkName(name: string, matrix: readonly string[]): string {
  return matrix.length === 0 ? name : `${name} (${matrix.join(`, `)})`
}
