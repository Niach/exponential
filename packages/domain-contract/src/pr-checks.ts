import contract from "../contract.json" with { type: "json" }
import uiFixture from "../fixtures/pr-checks.json" with { type: "json" }

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

/** #FEED-199 (`fixture.wake`): the state a `pr_checks` wake reports, or
 *  null = no wake yet. Gated = the base branch's `required` names when it
 *  names any (a required name without a row = not reported), else every
 *  row. A `cancelled` run is no outcome (a push/rebase superseded it).
 *  failure = a gated red other than cancelled (wins) · success = every gated
 *  check completed, none red or cancelled · else null. */
export function checksWakeState<T extends PrCheckLike>(
  checks: readonly T[],
  required: Iterable<string>
): { state: `success` | `failure`; gated: T[] } | null {
  const names = new Set(required)
  const gated = names.size > 0 ? checks.filter((check) => names.has(check.name)) : [...checks]
  if (gated.length === 0) return null
  if (gated.some((check) => isFailingCheck(check) && check.conclusion !== `cancelled`)) return { state: `failure`, gated }
  if (names.size > 0 && [...names].some((name) => !gated.some((check) => check.name === name))) return null
  if (gated.some((check) => check.status !== `completed` || check.conclusion === `cancelled`)) return null
  return { state: `success`, gated }
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

/** The gated checks (readiness, the wake): the required ones when the base
 *  branch names any, else every check (a repo without branch protection
 *  still has CI worth waiting for). */
export function gatedChecks<T extends PrCheckLike>(checks: readonly T[]): T[] {
  const required = checks.filter((check) => check.required)
  return required.length > 0 ? required : [...checks]
}

/** #EXP-1711 (merge-train.json READINESS): the aggregate over the GATED
 *  checks only — what holds a PR before the train; an informational leg
 *  (not required while the base names required ones) never does. */
export function gatedChecksState(checks: readonly PrCheckLike[]): PrChecksState {
  return aggregateChecksState(gatedChecks(checks))
}

/** #FEED-221: the same over the base branch's required NAMES when the forge
 *  answered them, so a row that lost its `required` flag cannot gate; a
 *  required name without a row = not reported yet. No names = the flags. */
export function requiredChecksState(checks: readonly PrCheckLike[], required: Iterable<string>): PrChecksState {
  const names = new Set(required)
  if (names.size === 0) return gatedChecksState(checks)
  const gated = checks.filter((check) => names.has(check.name))
  if (gated.some(isFailingCheck)) return `failure`
  if ([...names].some((name) => !gated.some((check) => check.name === name))) return `pending`
  return aggregateChecksState(gated)
}

const COPY = contract.prChecks

function list(names: readonly string[]): string {
  const shown = names.slice(0, 5).join(`, `)
  return names.length > 5 ? `${shown} +${names.length - 5}` : shown
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
  /** #FEED-198: why there are no rows; absent while rows exist or unread. */
  unknown?: PrChecksUnknown
  note?: string
}

export type PrChecksUnknown = keyof typeof uiFixture.unknownText

/** What `exponential_pr` returns as `checks` on every op. */
export function checksBrief(
  state: PrChecksState,
  checks: readonly PrCheckLike[],
  unknown?: PrChecksUnknown | null
): PrChecksBrief {
  return {
    state,
    total: checks.length,
    failing: checks
      .filter(isFailingCheck)
      .map((check) => ({ name: check.name, url: check.detailsUrl ?? null, required: Boolean(check.required) })),
    pending: checks.filter((check) => check.status !== `completed`).map((check) => check.name),
    ...(unknown && checks.length === 0 ? { unknown, note: uiFixture.unknownText[unknown] } : {}),
  }
}

// ── #EXP-1611: the UI ×4 (pr-checks.json MERGE PILL / CHECKS LIST) ────────

export const PR_CHECKS_COPY = uiFixture.copy
export const PR_CHECKS_TEST_IDS = uiFixture.testIds

export type CheckGlyph = `ci-queued` | `ci-running` | `ci-passed` | `ci-failed` | `ci-skipped`
export type CheckTone = `muted` | `green` | `red`

export interface PrCheckRowLike extends PrCheckLike {
  startedAt?: string | null
  completedAt?: string | null
}

export interface PrCheckListRow {
  name: string
  glyph: CheckGlyph
  tone: CheckTone
  duration: string | null
  url: string | null
}

/** `42s` · `3m 5s` · `1h 2m` (floored; negative = 0s). */
export function formatCheckDuration(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000))
  if (seconds < 60) return `${seconds}s`
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m ${seconds % 60}s`
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`
}

const msOf = (value: string | null | undefined): number | null => {
  if (!value) return null
  const ms = Date.parse(value)
  return Number.isNaN(ms) ? null : ms
}

/** completed = completedAt - startedAt; in_progress = now - startedAt;
 *  else null (queued, or no start). */
export function checkDuration(check: PrCheckRowLike, now: Date | string): string | null {
  const started = msOf(check.startedAt)
  if (started === null) return null
  if (check.status === `completed`) {
    const completed = msOf(check.completedAt)
    return completed === null ? null : formatCheckDuration(completed - started)
  }
  if (check.status === `in_progress`) {
    const at = typeof now === `string` ? Date.parse(now) : now.getTime()
    return formatCheckDuration(at - started)
  }
  return null
}

export function checkGlyph(check: Pick<PrCheckLike, `status` | `conclusion`>): {
  glyph: CheckGlyph
  tone: CheckTone
} {
  if (check.status === `queued`) return { glyph: `ci-queued`, tone: `muted` }
  if (check.status !== `completed`) return { glyph: `ci-running`, tone: `muted` }
  if (isFailingCheck(check)) return { glyph: `ci-failed`, tone: `red` }
  if (check.conclusion === `success`) return { glyph: `ci-passed`, tone: `green` }
  return { glyph: `ci-skipped`, tone: `muted` }
}

const GLYPH_ORDER: Record<CheckGlyph, number> = {
  "ci-failed": 0,
  "ci-running": 1,
  "ci-queued": 2,
  "ci-passed": 3,
  "ci-skipped": 4,
}

const byCodePoint = (a: string, b: string): number => (a < b ? -1 : a > b ? 1 : 0)

/** The `…` menu's Checks rows: failed, running, queued, passed, skipped;
 *  ties by name (code-point order). */
export function checksList(checks: readonly PrCheckRowLike[], now: Date | string): PrCheckListRow[] {
  return checks
    .map((check) => ({
      name: check.name,
      ...checkGlyph(check),
      duration: checkDuration(check, now),
      url: check.detailsUrl || null,
    }))
    .sort((a, b) => GLYPH_ORDER[a.glyph] - GLYPH_ORDER[b.glyph] || byCodePoint(a.name, b.name))
}

/** The Checks submenu entry's glyph + value; null = no rows (no entry). */
export function checksSummary(
  checks: readonly PrCheckLike[]
): { glyph: CheckGlyph; tone: CheckTone; value: string } | null {
  const state = aggregateChecksState(checks)
  const fill = (template: string, count: number) => template.replace(`{count}`, String(count))
  switch (state) {
    case `none`:
      return null
    case `failure`:
      return { glyph: `ci-failed`, tone: `red`, value: fill(PR_CHECKS_COPY.summary.failing, checks.filter(isFailingCheck).length) }
    case `pending`:
      return {
        glyph: `ci-running`,
        tone: `muted`,
        value: fill(PR_CHECKS_COPY.summary.running, checks.filter((check) => check.status !== `completed`).length),
      }
    case `success`:
      return { glyph: `ci-passed`, tone: `green`, value: fill(PR_CHECKS_COPY.summary.passed, checks.length) }
  }
}

