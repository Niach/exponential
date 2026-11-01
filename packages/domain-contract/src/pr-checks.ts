import contract from "../contract.json" with { type: "json" }

// EXP-1610 (fixtures/pr-checks.json): CI on a pull request, provider-neutral.
// The pure half every writer and reader shares: the aggregate state, the
// GitHub status mapping, the merge gate, and the texts an agent reads. The
// server (lib/integrations/pr-checks.ts) writes the rows; the clients
// (#EXP-1611) read the synced rows and replay the same fixture.

export type PrChecksState = `none` | `pending` | `success` | `failure`
export type PrCheckStatus = `queued` | `in_progress` | `completed`

export interface PrCheckLike {
  name: string
  status: string
  conclusion: string | null
  required?: boolean
  detailsUrl?: string | null
}

const FAILING = new Set<string>(contract.prChecks.failingConclusions)

export const PR_CHECKS_QUEUED_FLAG_MINUTES: number = contract.prChecks.queuedFlagMinutes

export function isFailingCheck(check: Pick<PrCheckLike, `status` | `conclusion`>): boolean {
  return check.status === `completed` && check.conclusion !== null && FAILING.has(check.conclusion)
}

/** The aggregate over a PR's checks at its head: none (no rows) · failure
 *  (any completed red one; wins over pending) · pending (any not completed)
 *  · success. */
export function aggregateChecksState(checks: readonly PrCheckLike[]): PrChecksState {
  if (checks.length === 0) return `none`
  if (checks.some(isFailingCheck)) return `failure`
  if (checks.some((check) => check.status !== `completed`)) return `pending`
  return `success`
}

/** GitHub's check-run / workflow-run status → contract prCheckStatus. */
export function checkStatusFromGithub(status: string | null | undefined): PrCheckStatus {
  if (status === `completed`) return `completed`
  if (status === `in_progress`) return `in_progress`
  // queued, waiting, requested, pending, unknown
  return `queued`
}

const CONCLUSIONS = new Set<string>(contract.prCheckConclusion.values)

/** GitHub's conclusion → contract prCheckConclusion (null while running,
 *  and for anything outside the contract). */
export function checkConclusionFromGithub(
  status: PrCheckStatus,
  conclusion: string | null | undefined
): string | null {
  if (status !== `completed` || !conclusion) return null
  return CONCLUSIONS.has(conclusion) ? conclusion : `neutral`
}

/** The checks the merge gate looks at: the required ones when the base
 *  branch names any, else every check (a repo without branch protection
 *  still has CI worth waiting for). */
export function gatedChecks<T extends PrCheckLike>(checks: readonly T[]): T[] {
  const required = checks.filter((check) => check.required)
  return required.length > 0 ? required : [...checks]
}

export type MergeGateVerdict =
  | { ok: true }
  | { ok: false; reason: `pending` | `failing`; names: string[] }

/** MERGE_CHECK_GATE: refuse a merge while a gated check is red (first) or
 *  still running. No checks known = nothing to wait for. */
export function mergeGate(checks: readonly PrCheckLike[]): MergeGateVerdict {
  const gated = gatedChecks(checks)
  const failing = gated.filter(isFailingCheck).map((check) => check.name)
  if (failing.length > 0) return { ok: false, reason: `failing`, names: failing }
  const pending = gated.filter((check) => check.status !== `completed`).map((check) => check.name)
  if (pending.length > 0) return { ok: false, reason: `pending`, names: pending }
  return { ok: true }
}

const COPY = contract.prChecks

function list(names: readonly string[]): string {
  const shown = names.slice(0, 5).join(`, `)
  return names.length > 5 ? `${shown} +${names.length - 5}` : shown
}

/** The merge gate's refusal (an agent's `pr merge` result). */
export function mergeGateText(prNumber: number, verdict: Exclude<MergeGateVerdict, { ok: true }>): string {
  return verdict.reason === `failing`
    ? `PR #${prNumber} has failing checks: ${list(verdict.names)}. Read them, fix and push; merge again when CI is green (force: true merges anyway, only when a person says so).`
    : `PR #${prNumber} still has checks running: ${list(verdict.names)}. Do not poll: Exponential wakes this session when CI finishes. Merge then (force: true merges anyway, only when a person says so).`
}

/** The waking line when a PR's CI leaves pending ("CI on #N: …"). */
export function checksWakeText(
  prNumber: number,
  state: `success` | `failure`,
  failing: ReadonlyArray<{ name: string; detailsUrl?: string | null }>
): string {
  if (state === `success`) return `CI on #${prNumber}: success.`
  const named = failing.slice(0, 5).map((check) => (check.detailsUrl ? `${check.name} (${check.detailsUrl})` : check.name))
  const more = failing.length > 5 ? ` +${failing.length - 5} more` : ``
  return `CI on #${prNumber}: failure (${named.join(`, `)}${more}).`
}

/** The once-only line for a check queued too long (no runner took it). */
export function checksQueuedText(prNumber: number, names: readonly string[]): string {
  return `CI on #${prNumber}: ${list(names)} queued for ${COPY.queuedFlagMinutes}+ minutes; no runner picked ${names.length === 1 ? `it` : `them`} up. Tell a person if it stays queued.`
}

export interface PrChecksBrief {
  state: PrChecksState
  total: number
  failing: Array<{ name: string; url: string | null; required: boolean }>
  pending: string[]
}

/** What `exponential_pr` returns as `checks` on every op. */
export function checksBrief(state: PrChecksState, checks: readonly PrCheckLike[]): PrChecksBrief {
  return {
    state,
    total: checks.length,
    failing: checks
      .filter(isFailingCheck)
      .map((check) => ({ name: check.name, url: check.detailsUrl ?? null, required: Boolean(check.required) })),
    pending: checks.filter((check) => check.status !== `completed`).map((check) => check.name),
  }
}
