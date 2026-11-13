import fixture from "../fixtures/ci-run.json" with { type: "json" }
import runnerFixture from "../fixtures/runner-settings.json" with { type: "json" }
import { checkDuration, checkGlyph, formatCheckDuration, isFailingCheck, type CheckGlyph, type CheckTone } from "./pr-checks"

// #EXP-1617 / #EXP-1804 (fixtures/ci-run.json, fixtures/runner-settings.json):
// the pure half of the CI run page and of a device's CI runner section. The
// TS reference; the desktop (crates/domain ci_run.rs), iOS (ExpCore) and
// Android replay the same fixtures.

export const CI_RUN_COPY = fixture.copy
export const CI_RUN_TEST_IDS = fixture.testIds
export const CI_RUN_LIVE_TAIL_MS: number = fixture.liveTailMs
export const CI_RUN_LOG_MAX_LINES = fixture.logMaxLines

export type CiRunStatus = `queued` | `running` | `passed` | `failed` | `cancelled`
export type CiTone = `neutral` | `primary` | `success` | `danger`
export type CiTraceStatus = `pending` | `running` | `done` | `skipped` | `error`

export interface CiJobLike {
  status: string
  conclusion: string | null
}

const STATUS_TONE: Record<CiRunStatus, CiTone> = {
  queued: `neutral`,
  running: `primary`,
  passed: `success`,
  failed: `danger`,
  cancelled: `neutral`,
}

/** The run's status for its first tile. */
export function ciRunStatus(state: string, jobs: readonly CiJobLike[]): { status: CiRunStatus; tone: CiTone } {
  let status: CiRunStatus
  if (state === `pending`) status = jobs.some((job) => job.status === `in_progress`) ? `running` : `queued`
  else if (state === `success`) status = `passed`
  else {
    const failing = jobs.filter(isFailingCheck)
    status = failing.length > 0 && failing.every((job) => job.conclusion === `cancelled`) ? `cancelled` : `failed`
  }
  return { status, tone: STATUS_TONE[status] }
}

export interface CiTile {
  label: string
  value: string
  tone: CiTone
}

const msOf = (value: string | null | undefined): number | null => {
  if (!value) return null
  const ms = Date.parse(value)
  return Number.isNaN(ms) ? null : ms
}

/** Status, Duration, Runner, Commit. */
export function ciRunTiles(
  run: { state: string; sha: string; startedAt: string | null; completedAt: string | null },
  jobs: ReadonlyArray<CiJobLike & { runnerLabel: string | null }>,
  now: Date | string
): CiTile[] {
  const copy = CI_RUN_COPY
  const { status, tone } = ciRunStatus(run.state, jobs)
  const started = msOf(run.startedAt)
  const end = msOf(run.completedAt) ?? (typeof now === `string` ? Date.parse(now) : now.getTime())
  const runners = [...new Set(jobs.flatMap((job) => (job.runnerLabel ? [job.runnerLabel] : [])))]
  return [
    { label: copy.tiles.status, value: copy.status[status], tone },
    { label: copy.tiles.duration, value: started === null ? copy.none : formatCheckDuration(end - started), tone: `neutral` },
    { label: copy.tiles.runner, value: runners.length > 0 ? runners.join(`, `) : copy.none, tone: `neutral` },
    { label: copy.tiles.commit, value: run.sha.slice(0, 7), tone: `neutral` },
  ]
}

export interface CiJobRowInput extends CiJobLike {
  id: string
  check: string
  position: number
  reason: string | null
  startedAt: string | null
  completedAt: string | null
}

export interface CiJobRow {
  id: string
  title: string
  subtitle: string | null
  glyph: CheckGlyph
  tone: CheckTone
  duration: string | null
}

const reasonText = (reason: string | null): string | null =>
  reason ? ((CI_RUN_COPY.reasons as Record<string, string>)[reason] ?? null) : null

const byPosition = <T extends { position: number }>(jobs: readonly T[]): T[] => [...jobs].sort((a, b) => a.position - b.position)

/** The jobs in planner order. */
export function ciJobRows(jobs: readonly CiJobRowInput[], now: Date | string): CiJobRow[] {
  return byPosition(jobs).map((job) => ({
    id: job.id,
    title: job.check,
    subtitle: job.status === `completed` ? reasonText(job.reason) : null,
    ...checkGlyph(job),
    duration: checkDuration({ name: job.check, status: job.status, conclusion: job.conclusion, startedAt: job.startedAt, completedAt: job.completedAt }, now),
  }))
}

/** The selected job: the requested one, else the first failing, running, any. */
export function ciDefaultJob(
  jobs: ReadonlyArray<CiJobLike & { id: string; position: number }>,
  requested: string | null | undefined
): string | null {
  const ordered = byPosition(jobs)
  if (requested && ordered.some((job) => job.id === requested)) return requested
  return (ordered.find(isFailingCheck) ?? ordered.find((job) => job.status === `in_progress`) ?? ordered[0])?.id ?? null
}

export interface CiStepLike {
  name: string
  status: string
  conclusion: string | null
  startedAt: number | null
  completedAt: number | null
}

export interface CiTraceItem {
  title: string
  status: CiTraceStatus
  description: string | null
}

function traceStatus(step: CiStepLike): CiTraceStatus {
  if (step.status === `queued`) return `pending`
  if (step.status !== `completed`) return `running`
  if (step.conclusion === `success`) return `done`
  if (step.conclusion === `skipped` || step.conclusion === `neutral`) return `skipped`
  return `error`
}

/** A job's steps as trace items. */
export function ciSteps(steps: readonly CiStepLike[], now: Date | string): CiTraceItem[] {
  const at = typeof now === `string` ? Date.parse(now) : now.getTime()
  return steps.map((step) => {
    const end = step.status === `completed` ? step.completedAt : step.status === `in_progress` ? at : null
    return {
      title: step.name,
      status: traceStatus(step),
      description: step.startedAt !== null && end !== null ? formatCheckDuration(end - step.startedAt) : null,
    }
  })
}

/** The log as shown: the runner's markers become plain lines; error lines
 *  are highlighted (1-based, displayed lines). */
export function ciLogView(text: string): { code: string; highlight: number[] } {
  const lines = text.split(`\n`)
  if (lines.length > 0 && lines[lines.length - 1] === ``) lines.pop()
  const out: string[] = []
  const highlight: number[] = []
  for (const line of lines) {
    if (line.startsWith(`##[endgroup]`)) continue
    if (line.startsWith(`##[group]`)) out.push(`▸ ${line.slice(`##[group]`.length)}`)
    else if (line.startsWith(`##[error]`)) {
      out.push(`Error: ${line.slice(`##[error]`.length)}`)
      highlight.push(out.length)
    } else if (line.startsWith(`##[warning]`)) out.push(`Warning: ${line.slice(`##[warning]`.length)}`)
    else out.push(line)
  }
  return { code: out.join(`\n`), highlight }
}

/** Which buttons the run shows. */
export function ciRunActions(run: { state: string; error: string | null }): { rerunFailed: boolean; cancel: boolean } {
  return { rerunFailed: run.state === `failure`, cancel: run.state === `pending` }
}

/** The specimen every platform's shot renders. */
export const CI_RUN_SPECIMEN = fixture.specimen

// ── the device's CI runner section (runner-settings.json) ────────────────

export const RUNNER_SETTINGS_COPY = runnerFixture.copy
export const RUNNER_SETTINGS_TEST_IDS = runnerFixture.testIds
export const RUNNER_DEFAULTS = runnerFixture.defaults
export const RUNNER_LIMITS = runnerFixture.limits
export const RUNNER_TIMEOUT_OPTIONS = runnerFixture.timeoutOptions
export const RUNNER_SETTINGS_SPECIMEN = runnerFixture.specimen

const LABEL = /^[a-z0-9][a-z0-9._-]{0,31}$/

/** A typed label as stored, null = refused. */
export function normalizeRunnerLabel(input: string): string | null {
  const label = input.trim().toLowerCase()
  return LABEL.test(label) ? label : null
}

export type RunnerClient = `web` | `web-mobile` | `desktop` | `ios` | `android`
const PHONES = new Set<RunnerClient>([`ios`, `android`, `web-mobile`])

/** The section's notice: phones never run jobs; an old app cannot. */
export function runnerNotice(client: RunnerClient, caps: readonly string[]): `phone` | `update` | null {
  if (PHONES.has(client)) return `phone`
  if (!caps.includes(`ci-runner`)) return `update`
  return null
}

/** The OS label a runner always has. */
export function runnerOsLabel(platform: string | null | undefined): string | null {
  if (platform === `linux`) return `linux`
  if (platform === `macos` || platform === `darwin`) return `mac`
  if (platform === `windows`) return `windows`
  return null
}

export interface RunnerJobInput extends CiJobLike {
  id: string
  runnerDeviceId: string | null
  check: string
  runNumber: number
  repo: string
  startedAt: string | null
  completedAt: string | null
}

/** The runner's newest jobs. */
export function runnerRecentJobs(deviceId: string, jobs: readonly RunnerJobInput[], now: Date | string): CiJobRow[] {
  return jobs
    .filter((job) => job.runnerDeviceId === deviceId)
    .sort((a, b) => (msOf(b.startedAt) ?? -Infinity) - (msOf(a.startedAt) ?? -Infinity))
    .slice(0, RUNNER_LIMITS.recent)
    .map((job) => ({
      id: job.id,
      title: job.check,
      subtitle: RUNNER_SETTINGS_COPY.runSubtitle.replace(`{number}`, String(job.runNumber)).replace(`{repo}`, job.repo),
      ...checkGlyph(job),
      duration: checkDuration({ name: job.check, status: job.status, conclusion: job.conclusion, startedAt: job.startedAt, completedAt: job.completedAt }, now),
    }))
}
