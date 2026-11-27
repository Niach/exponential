import fixture from "../fixtures/ci-run.json" with { type: "json" }
import runnerFixture from "../fixtures/runner-settings.json" with { type: "json" }
import pageFixture from "../fixtures/ci-page.json" with { type: "json" }
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
/** #EXP-2172: THE status tones (ci-page.json `statusTones`): running is
 *  the only blue (`info`) mark. */
export type CiTone = `neutral` | `info` | `success` | `danger`
export type CiTraceStatus = `pending` | `running` | `done` | `skipped` | `error`

export interface CiJobLike {
  status: string
  conclusion: string | null
}

export const CI_STATUS_TONES = pageFixture.statusTones as Record<CiRunStatus, { glyph: CheckGlyph; tone: CiTone }>
const STATUS_TONE = (status: CiRunStatus): CiTone => CI_STATUS_TONES[status].tone

/** The run's status for its first tile. */
export function ciRunStatus(state: string, jobs: readonly CiJobLike[]): { status: CiRunStatus; tone: CiTone } {
  let status: CiRunStatus
  if (state === `pending`) status = jobs.some((job) => job.status === `in_progress`) ? `running` : `queued`
  else if (state === `success`) status = `passed`
  else {
    const failing = jobs.filter(isFailingCheck)
    status = failing.length > 0 && failing.every((job) => job.conclusion === `cancelled`) ? `cancelled` : `failed`
  }
  return { status, tone: STATUS_TONE(status) }
}

const msOf = (value: string | null | undefined): number | null => {
  if (!value) return null
  const ms = Date.parse(value)
  return Number.isNaN(ms) ? null : ms
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
  /** #EXP-2172: the run page's status (and its tone, `statusTones`). */
  status: CiRunStatus
  statusTone: CiTone
}

/** A job's status in the run page's terms. */
export function ciJobStatus(job: CiJobLike): CiRunStatus {
  if (job.status === `queued`) return `queued`
  if (job.status !== `completed`) return `running`
  if (isFailingCheck(job) && job.conclusion !== `cancelled`) return `failed`
  if (job.conclusion === `success`) return `passed`
  return `cancelled`
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
    status: ciJobStatus(job),
    statusTone: CI_STATUS_TONES[ciJobStatus(job)].tone,
  }))
}

/** The Jobs section's caption: `1 failed · 3 passed · 1 skipped` (the
 *  counts that are not zero, in this order). */
export function ciJobsCaption(jobs: readonly CiJobLike[]): string {
  const counts: Record<CiRunStatus, number> = { failed: 0, running: 0, queued: 0, passed: 0, cancelled: 0 }
  for (const job of jobs) counts[ciJobStatus(job)]++
  const parts = CI_RUN_COPY.jobsCaption as Record<CiRunStatus, string>
  return ([`failed`, `running`, `queued`, `passed`, `cancelled`] as const)
    .filter((status) => counts[status] > 0)
    .map((status) => parts[status].replace(`{count}`, String(counts[status])))
    .join(` · `)
}

/** The selected job's card line: `Failed after 8m 25s on mint`. */
export function ciJobSummary(job: CiJobRowInput & { runnerLabel: string | null }, now: Date | string): string {
  const copy = CI_RUN_COPY.jobSummary
  const status = ciJobStatus(job)
  const duration = checkDuration({ name: job.check, status: job.status, conclusion: job.conclusion, startedAt: job.startedAt, completedAt: job.completedAt }, now)
  if (status === `queued`) return copy.queued
  if (status === `cancelled`) return reasonText(job.reason) ?? copy.skipped
  const head = (copy as Record<string, string>)[status]!.replace(`{duration}`, duration ?? CI_RUN_COPY.none)
  return job.runnerLabel ? `${head}${copy.on.replace(`{runner}`, job.runnerLabel)}` : head
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

export const CI_RUN_FIX_LOG_LINES: number = fixture.fixLogLines

export type CiRunAction = `rerunFailed` | `rerunAll` | `cancel` | `openExternal`

export interface CiRunActions {
  /** THE one default button: Cancel while it runs or waits, Re-run failed
   *  after a failure, Re-run all after a pass; null when the run is not
   *  ours to change (a GitHub run without Actions: write). */
  primary: `rerunFailed` | `rerunAll` | `cancel` | null
  /** Fix with an agent (outline), after a failure. */
  fix: boolean
  /** The ⋯ menu, in order: Re-run all jobs (when it is not the primary),
   *  Open on GitHub. Empty = no menu. */
  menu: Array<`rerunAll` | `openExternal`>
}

/** Which actions the run offers (#EXP-2172, the approved redesign; a
 *  GitHub run re-runs and cancels only once its installation accepted
 *  Actions: write). */
export function ciRunActions(run: {
  state: string
  error: string | null
  provider?: string | null
  externalAccess?: string | null
  externalUrl?: string | null
}): CiRunActions {
  const github = run.provider === `github`
  const writable = !github || run.externalAccess === `write`
  const primary = !writable ? null : run.state === `pending` ? `cancel` : run.state === `failure` ? `rerunFailed` : `rerunAll`
  const menu: CiRunActions[`menu`] = []
  if (writable && primary !== `rerunAll`) menu.push(`rerunAll`)
  if (github && run.externalUrl) menu.push(`openExternal`)
  return { primary, fix: run.state === `failure`, menu }
}

export interface CiRunChip {
  id: `status` | `ref` | `sha` | `pr` | `author` | `duration` | `runner`
  label: string
  icon: string | null
  /** The status chip's dot (a colour token), else null. */
  dot: string | null
}

const DOT: Record<CiTone, string> = {
  success: `$color.success`,
  danger: `$color.destructive`,
  info: `$color.info`,
  neutral: `$color.mutedForeground`,
}

/** The chip strip under the title: status (with its dot) · ref → base ·
 *  sha · #PR · author · duration · runners; a chip without data is left out. */
export function ciRunChips(input: {
  run: { state: string; sha: string; ref: string | null; startedAt: string | null; completedAt: string | null }
  jobs: ReadonlyArray<CiJobLike & { runnerLabel: string | null }>
  pullRequest: { number: number; base: string } | null
  author: string | null
  now: Date | string
}): CiRunChip[] {
  const { status, tone } = ciRunStatus(input.run.state, input.jobs)
  const started = msOf(input.run.startedAt)
  const end = msOf(input.run.completedAt) ?? (typeof input.now === `string` ? Date.parse(input.now) : input.now.getTime())
  const runners = [...new Set(input.jobs.flatMap((job) => (job.runnerLabel ? [job.runnerLabel] : [])))]
  const chips: CiRunChip[] = [{ id: `status`, label: CI_RUN_COPY.status[status], icon: null, dot: DOT[tone] }]
  if (input.run.ref) {
    chips.push({ id: `ref`, label: input.pullRequest ? `${input.run.ref} → ${input.pullRequest.base}` : input.run.ref, icon: `git-branch`, dot: null })
  }
  chips.push({ id: `sha`, label: input.run.sha.slice(0, 7), icon: `hash`, dot: null })
  if (input.pullRequest) chips.push({ id: `pr`, label: `#${input.pullRequest.number}`, icon: `git-pull-request`, dot: null })
  if (input.author) chips.push({ id: `author`, label: input.author, icon: `user`, dot: null })
  if (started !== null) chips.push({ id: `duration`, label: formatCheckDuration(end - started), icon: `clock`, dot: null })
  if (runners.length > 0) chips.push({ id: `runner`, label: runners.join(`, `), icon: `server`, dot: null })
  return chips
}

/** The failure summary above the jobs, from the failed job's log as read:
 *  `Failed in {job}` and its first error line. Null until such a line is
 *  read (logs are read on demand; no test counting). */
export function ciFailureSummary(job: { check: string } | null, log: string | null): { title: string; message: string } | null {
  if (!job || !log) return null
  const view = ciLogView(log)
  const first = view.highlight[0]
  if (first === undefined) return null
  const line = view.code.split(`\n`)[first - 1] ?? ``
  return { title: CI_RUN_COPY.failure.title.replace(`{job}`, job.check), message: line.replace(/^Error: /, ``) }
}

/** The first message of Fix with an agent (the composer's text). */
export function ciFixPrompt(input: {
  run: { id: string; number: number; name: string; sha: string; ref: string | null }
  repository: string
  job: string | null
  log: string | null
}): string {
  const copy = CI_RUN_COPY.fixPrompt
  const ref = input.run.ref ?? CI_RUN_COPY.none
  const fill = (text: string) =>
    text
      .replaceAll(`{number}`, String(input.run.number))
      .replaceAll(`{name}`, input.run.name)
      .replaceAll(`{repository}`, input.repository)
      .replaceAll(`{ref}`, ref)
      .replaceAll(`{sha}`, input.run.sha.slice(0, 7))
      .replaceAll(`{runId}`, input.run.id)
      .replaceAll(`{job}`, input.job ?? ``)
  const lines = [fill(copy.head)]
  if (input.job) lines.push(fill(copy.job))
  lines.push(fill(copy.read))
  const code = input.log ? ciLogView(input.log).code : ``
  if (code !== ``) {
    const tail = code.split(`\n`).slice(-CI_RUN_FIX_LOG_LINES)
    lines.push(``, copy.tail, `\`\`\`text`, ...tail, `\`\`\``)
  }
  return lines.join(`\n`)
}

/** The specimen every platform's shot renders. */
export const CI_RUN_SPECIMEN = fixture.specimen

// ── the device's CI runner section (runner-settings.json) ────────────────

export const RUNNER_SETTINGS_COPY = runnerFixture.copy
export const RUNNER_SETTINGS_TEST_IDS = runnerFixture.testIds
export const RUNNER_DEFAULTS = runnerFixture.defaults
export const RUNNER_LIMITS = runnerFixture.limits
export const RUNNER_TIMEOUT_OPTIONS = runnerFixture.timeoutOptions
export const RUNNER_MEMORY_OPTIONS = runnerFixture.memoryOptions
export const RUNNER_SETTINGS_SPECIMEN = runnerFixture.specimen

const LABEL = /^[a-z0-9][a-z0-9._-]{0,31}$/

/** A typed label as stored, null = refused. */
export function normalizeRunnerLabel(input: string): string | null {
  const label = input.trim().toLowerCase()
  return LABEL.test(label) ? label : null
}

export type RunnerClient = `web` | `web-mobile` | `desktop` | `ios` | `android`
const PHONES = new Set<RunnerClient>([`ios`, `android`, `web-mobile`])

/** #EXP-2193: the desktop app on macOS or Windows gets no runner yet (a job
 *  could read the app's data there); a server daemon on any OS keeps it. */
export function runnerUnsupported(kind: string | null | undefined, platform: string | null | undefined): boolean {
  return kind === `desktop` && (platform === `macos` || platform === `darwin` || platform === `windows`)
}

/** The section's notice: an unsupported desktop, phones never run jobs, an
 *  old app cannot. */
export function runnerNotice(
  client: RunnerClient,
  caps: readonly string[],
  kind: string | null | undefined,
  platform: string | null | undefined
): `unsupported` | `phone` | `update` | null {
  if (runnerUnsupported(kind, platform)) return `unsupported`
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
/** A runner section's recent row (the run page's row without its status). */
export type RunnerRecentRow = Omit<CiJobRow, `status` | `statusTone`>

export function runnerRecentJobs(deviceId: string, jobs: readonly RunnerJobInput[], now: Date | string): RunnerRecentRow[] {
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
