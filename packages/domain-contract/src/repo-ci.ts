import fixture from "../fixtures/repo-ci.json" with { type: "json" }
import { ciRunStatus, type CiRunStatus } from "./ci-run"
import { formatCheckDuration, type CheckGlyph, type CheckTone } from "./pr-checks"

// #EXP-1617 E4 part 4 (fixtures/repo-ci.json): the repo page's Runs tab and
// the CI section of its Settings tab. The TS reference; desktop
// (domain::repo_ci), iOS (ExpCore RepoCi) and Android replay the fixture.

export const REPO_CI_COPY = fixture.copy
export const REPO_CI_TEST_IDS = fixture.testIds
export const REPO_CI_LIMITS = fixture.limits

export type CiTrigger = keyof typeof fixture.copy.triggers

export interface RepoRunInput {
  id: string
  number: number
  name: string
  sha: string
  ref: string | null
  trigger: string
  state: string
  startedAt: string | null
  completedAt: string | null
}

export interface RepoRunJobInput {
  runId: string
  status: string
  conclusion: string | null
}

export interface RepoRunRow {
  id: string
  identifier: string
  title: string
  status: CiRunStatus
  statusLabel: string
  glyph: CheckGlyph
  tone: CheckTone
  commit: string
  ref: string
  trigger: string
  /** Phones: `ref · sha7 · trigger`. */
  subtitle: string
  duration: string | null
  startedAt: string | null
}

const GLYPH: Record<CiRunStatus, { glyph: CheckGlyph; tone: CheckTone }> = {
  queued: { glyph: `ci-queued`, tone: `muted` },
  running: { glyph: `ci-running`, tone: `muted` },
  passed: { glyph: `ci-passed`, tone: `green` },
  failed: { glyph: `ci-failed`, tone: `red` },
  cancelled: { glyph: `ci-skipped`, tone: `muted` },
}

const msOf = (value: string | null): number | null => {
  if (!value) return null
  const ms = Date.parse(value)
  return Number.isNaN(ms) ? null : ms
}

const triggerLabel = (trigger: string): string => (REPO_CI_COPY.triggers as Record<string, string>)[trigger] ?? trigger

/** The Runs tab: newest first, at most `limits.runs`. */
export function runRows(runs: readonly RepoRunInput[], jobs: readonly RepoRunJobInput[], now: Date | string): RepoRunRow[] {
  const at = typeof now === `string` ? Date.parse(now) : now.getTime()
  return [...runs]
    .sort((a, b) => b.number - a.number)
    .slice(0, REPO_CI_LIMITS.runs)
    .map((run) => {
      const { status } = ciRunStatus(run.state, jobs.filter((job) => job.runId === run.id))
      const started = msOf(run.startedAt)
      const commit = run.sha.slice(0, 7)
      const ref = run.ref ?? REPO_CI_COPY.runs.noRef
      const trigger = triggerLabel(run.trigger)
      return {
        id: run.id,
        identifier: `#${run.number}`,
        title: run.name,
        status,
        statusLabel: REPO_CI_COPY.status[status],
        ...GLYPH[status],
        commit,
        ref,
        trigger,
        subtitle: `${ref} · ${commit} · ${trigger}`,
        duration: started === null ? null : formatCheckDuration((msOf(run.completedAt) ?? at) - started),
        startedAt: run.startedAt,
      }
    })
}

/** One row per pipeline name: its newest run's state. */
export function pipelineRows(
  runs: ReadonlyArray<Pick<RepoRunInput, `id` | `number` | `name` | `state`>>
): Array<{ name: string; runId: string; state: `pending` | `success` | `failure` }> {
  const newest = new Map<string, (typeof runs)[number]>()
  for (const run of runs) {
    const seen = newest.get(run.name)
    if (!seen || run.number > seen.number) newest.set(run.name, run)
  }
  return [...newest.values()]
    .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
    .map((run) => ({ name: run.name, runId: run.id, state: (run.state === `success` || run.state === `failure` ? run.state : `pending`) as `pending` | `success` | `failure` }))
}

/** Check names the required-checks field suggests: the checks of each
 *  pipeline's newest run, without the ones already required, sorted. */
export function requiredSuggestions(
  runs: ReadonlyArray<Pick<RepoRunInput, `id` | `number` | `name`>>,
  jobs: ReadonlyArray<{ runId: string; check: string }>,
  required: readonly string[]
): string[] {
  const newest = new Set(pipelineRows(runs.map((r) => ({ ...r, state: `pending` }))).map((p) => p.runId))
  const names = new Set(jobs.filter((job) => newest.has(job.runId)).map((job) => job.check))
  for (const name of required) names.delete(name)
  return [...names].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0))
}

/** A secret name the server stores (an env var name, ≤ 128). */
export function secretName(input: string): boolean {
  return input.length <= 128 && /^[A-Z_][A-Z0-9_]*$/.test(input)
}

/** The GitHub-only switches of the CI section, in order. */
export function repoCiSwitches(provider: string): Array<`githubStatuses` | `actionsRunners`> {
  return provider === `github` ? [`githubStatuses`, `actionsRunners`] : []
}
