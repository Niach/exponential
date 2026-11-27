import fixture from "../fixtures/ci-page.json" with { type: "json" }
import { ciRunStatus, runnerOsLabel, type CiRunStatus } from "./ci-run"
import { formatCheckDuration, type CheckGlyph, type CheckTone } from "./pr-checks"
import { MERGE_TRAIN_COPY, ordinal } from "./merge-train"
import { REPO_CI_COPY } from "./repo-ci"

// #EXP-2172 (fixtures/ci-page.json): THE CI page (one root row after Devices,
// across every member team) and its badge: our runs and GitHub Actions runs
// in one list, the merge train's current batches, the runner slots. The TS
// reference; the desktop (domain::ci_page), iOS (ExpCore CiPage) and Android
// (domain/CiPage.kt) replay the same fixture.

export const CI_PAGE_COPY = fixture.copy
export const CI_PAGE_LIMITS = fixture.limits
export const CI_PAGE_TEST_IDS = fixture.testIds

export type CiSource = `exponential` | `github`
export type CiFilter = `all` | `running` | `failed` | `mine`
export type CiTab = `runs` | `train` | `runners`

export interface CiPageRun {
  id: string
  repositoryId: string
  number: number
  name: string
  sha: string
  ref: string | null
  trigger: string
  state: string
  pullRequestId: string | null
  createdByUserId: string | null
  provider: string
  startedAt: string | null
  completedAt: string | null
  createdAt: string
}

export interface CiPageJob {
  runId: string
  status: string
  conclusion: string | null
  runsOn: string[]
  runnerDeviceId: string | null
  provider: string
}

export interface CiPageRepo {
  id: string
  fullName: string
}

export interface CiPagePr {
  id: string
  number: number
  title: string
  repositoryId: string
  rootSessionId: string | null
  base: string
  mergeState: string | null
  mergePosition: number | null
  mergeTrainId: string | null
}

/** A session row: its starter (`userId`). */
export interface CiPageSession {
  id: string
  userId: string
}

export interface CiPageRunner {
  id: string
  label: string
  platform: string | null
  labels: string[]
  slots: number
  lastSeenAt: string | null
}

export interface CiPageInput {
  runs: CiPageRun[]
  jobs: CiPageJob[]
  repos: CiPageRepo[]
  pullRequests: CiPagePr[]
  sessions: CiPageSession[]
  runners: CiPageRunner[]
  viewerId: string
  now: string
}

const msOf = (value: string | null | undefined): number | null => {
  if (!value) return null
  const ms = Date.parse(value)
  return Number.isNaN(ms) ? null : ms
}

const GLYPH: Record<CiRunStatus, { glyph: CheckGlyph; tone: CheckTone }> = {
  queued: { glyph: `ci-queued`, tone: `muted` },
  running: { glyph: `ci-running`, tone: `muted` },
  passed: { glyph: `ci-passed`, tone: `green` },
  failed: { glyph: `ci-failed`, tone: `red` },
  cancelled: { glyph: `ci-skipped`, tone: `muted` },
}

/** A run's source; anything but `github` is ours. */
export function ciSource(provider: string): CiSource {
  return provider === `github` ? `github` : `exponential`
}

/** When a run counts as started for the order (its start, else its row). */
function runAt(run: Pick<CiPageRun, `startedAt` | `createdAt`>): number {
  return msOf(run.startedAt) ?? msOf(run.createdAt) ?? 0
}

/** Newest first; ties by id so every platform orders alike. */
function newestFirst<T extends Pick<CiPageRun, `id` | `startedAt` | `createdAt`>>(runs: readonly T[]): T[] {
  return [...runs].sort((a, b) => runAt(b) - runAt(a) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
}

function statusOf(run: CiPageRun, jobs: readonly CiPageJob[]): CiRunStatus {
  return ciRunStatus(
    run.state,
    jobs.filter((job) => job.runId === run.id)
  ).status
}

/** `mine`: the viewer started the run (a push, a re-run, a GitHub actor
 *  linked to the viewer), or its pull request's chat is the viewer's. */
export function ciIsMine(
  run: Pick<CiPageRun, `createdByUserId` | `pullRequestId`>,
  viewerId: string,
  pullRequests: readonly CiPagePr[],
  sessions: readonly CiPageSession[]
): boolean {
  if (run.createdByUserId === viewerId) return true
  if (!run.pullRequestId) return false
  const pr = pullRequests.find((p) => p.id === run.pullRequestId)
  if (!pr?.rootSessionId) return false
  return sessions.some((s) => s.id === pr.rootSessionId && s.userId === viewerId)
}

export interface CiRunListRow {
  id: string
  identifier: string
  title: string
  status: CiRunStatus
  statusLabel: string
  glyph: CheckGlyph
  tone: CheckTone
  repositoryId: string
  repository: string
  source: CiSource
  sourceLabel: string
  ref: string
  trigger: string
  duration: string | null
  startedAt: string | null
  /** Phones: `repository · ref · source`. */
  subtitle: string
}

export interface CiRunFilter {
  status: CiFilter
  repositoryId: string | null
  source: CiSource | null
}

function triggerText(run: CiPageRun, pullRequests: readonly CiPagePr[]): string {
  if (run.trigger === `pull_request` && run.pullRequestId) {
    const pr = pullRequests.find((p) => p.id === run.pullRequestId)
    if (pr) return CI_PAGE_COPY.pullRequest.replace(`{number}`, String(pr.number))
  }
  return (CI_PAGE_COPY.triggers as Record<string, string>)[run.trigger] ?? run.trigger
}

export function repoName(repos: readonly CiPageRepo[], id: string): string {
  return repos.find((repo) => repo.id === id)?.fullName ?? CI_PAGE_COPY.unknownRepository
}

/** The Runs tab: every run of the filter, newest first, at most `limits.runs`. */
export function ciRunList(input: CiPageInput, filter: CiRunFilter): CiRunListRow[] {
  const at = msOf(input.now) ?? 0
  const rows: CiRunListRow[] = []
  for (const run of newestFirst(input.runs)) {
    const status = statusOf(run, input.jobs)
    const source = ciSource(run.provider)
    if (filter.repositoryId && run.repositoryId !== filter.repositoryId) continue
    if (filter.source && source !== filter.source) continue
    if (filter.status === `running` && status !== `running` && status !== `queued`) continue
    if (filter.status === `failed` && status !== `failed`) continue
    if (filter.status === `mine` && !ciIsMine(run, input.viewerId, input.pullRequests, input.sessions)) continue
    const started = msOf(run.startedAt)
    const repository = repoName(input.repos, run.repositoryId)
    const ref = run.ref ?? REPO_CI_COPY.runs.noRef
    const sourceLabel = CI_PAGE_COPY.sources[source]
    rows.push({
      id: run.id,
      identifier: `#${run.number}`,
      title: `#${run.number} ${run.name}`,
      status,
      statusLabel: REPO_CI_COPY.status[status],
      ...GLYPH[status],
      repositoryId: run.repositoryId,
      repository,
      source,
      sourceLabel,
      ref,
      trigger: triggerText(run, input.pullRequests),
      duration: started === null ? null : formatCheckDuration((msOf(run.completedAt) ?? at) - started),
      startedAt: run.startedAt,
      subtitle: `${repository} · ${ref} · ${sourceLabel}`,
    })
    if (rows.length >= CI_PAGE_LIMITS.runs) break
  }
  return rows
}

/** The repository pick: `All repositories`, then every repository that has
 *  a run, by name. */
export function ciRepoOptions(input: Pick<CiPageInput, `runs` | `repos`>): Array<{ label: string; value: string }> {
  const ids = [...new Set(input.runs.map((run) => run.repositoryId))]
  const named = ids.map((id) => ({ label: repoName(input.repos, id), value: id }))
  named.sort((a, b) => (a.label < b.label ? -1 : a.label > b.label ? 1 : a.value < b.value ? -1 : 1))
  return [{ label: CI_PAGE_COPY.repositories.all, value: `all` }, ...named]
}

export interface CiPageTile {
  label: string
  value: string
  tone: `neutral` | `danger`
  delta: string | null
}

/** A runner is online while its last beat is younger than this. */
export const RUNNER_ONLINE_MS: number = fixture.limits.runnerOnlineMs

function online(runner: CiPageRunner, now: number): boolean {
  const seen = msOf(runner.lastSeenAt)
  return seen !== null && now - seen < RUNNER_ONLINE_MS
}

/** Running, Queued, Failed today (the last 24 hours; delta = the newest
 *  failure's repository name, red), Runners (online of total). */
export function ciPageTiles(input: CiPageInput): CiPageTile[] {
  const at = msOf(input.now) ?? 0
  let running = 0
  let queued = 0
  const failed: CiPageRun[] = []
  for (const run of input.runs) {
    const status = statusOf(run, input.jobs)
    if (status === `running`) running++
    else if (status === `queued`) queued++
    else if (status === `failed`) {
      const done = msOf(run.completedAt)
      if (done !== null && at - done < 24 * 3600_000) failed.push(run)
    }
  }
  failed.sort((a, b) => (msOf(b.completedAt) ?? 0) - (msOf(a.completedAt) ?? 0))
  const newest = failed[0]
  const repo = newest ? repoName(input.repos, newest.repositoryId) : null
  const copy = CI_PAGE_COPY.tiles
  return [
    { label: copy.running, value: String(running), tone: `neutral`, delta: null },
    { label: copy.queued, value: String(queued), tone: `neutral`, delta: null },
    {
      label: copy.failedToday,
      value: String(failed.length),
      tone: failed.length > 0 ? `danger` : `neutral`,
      delta: repo ? (repo.split(`/`).pop() ?? repo) : null,
    },
    {
      label: copy.runners,
      value: copy.runnersValue
        .replace(`{online}`, String(input.runners.filter((runner) => online(runner, at)).length))
        .replace(`{total}`, String(input.runners.length)),
      tone: `neutral`,
      delta: null,
    },
  ]
}

/** nav-badges.json destination `ci`: the viewer's runs, the newest per
 *  (repository, pipeline, ref): failed ones = `attention` (a green re-run
 *  clears it), else the running / queued ones = `muted`, else none. */
export function ciBadge(
  input: Pick<CiPageInput, `runs` | `jobs` | `pullRequests` | `sessions` | `viewerId`>
): { count: number; tone: `attention` | `muted` } {
  const seen = new Set<string>()
  let failed = 0
  let live = 0
  for (const run of newestFirst(input.runs)) {
    if (!ciIsMine(run, input.viewerId, input.pullRequests, input.sessions)) continue
    const key = `${run.repositoryId}\u0000${run.name}\u0000${run.ref ?? ``}`
    if (seen.has(key)) continue
    seen.add(key)
    const status = statusOf(run, input.jobs)
    if (status === `failed`) failed++
    else if (status === `running` || status === `queued`) live++
  }
  return failed > 0 ? { count: failed, tone: `attention` } : { count: live, tone: `muted` }
}

export interface CiTrainRow {
  id: string
  identifier: string
  title: string
  subtitle: string | null
  glyph: CheckGlyph
  badge: string
  /** The PR a press opens (a batch: its first member). */
  pullRequestId: string
}

export interface CiTrainGroup {
  repositoryId: string
  title: string
  caption: string
  rows: CiTrainRow[]
}

const TRAIN_WAITING = new Set([`queued`, `checks`, `held`, `conflict`, `failing`])

/** The merge train per repository: its testing batches (one row each), then
 *  (`queue`: the Merge train tab) the queued PRs by position and the ones
 *  waiting on their own checks or held. Repositories by name. */
export function ciTrainGroups(input: Pick<CiPageInput, `pullRequests` | `repos`>, queue: boolean): CiTrainGroup[] {
  const copy = CI_PAGE_COPY.train
  const pill = MERGE_TRAIN_COPY.pill
  const waiting = { checks: pill.checksRunning, held: pill.onHold, conflict: pill.conflict, failing: pill.failing }
  const byRepo = new Map<string, CiPagePr[]>()
  for (const pr of input.pullRequests) {
    if (pr.mergeState !== `testing` && !(queue && pr.mergeState && TRAIN_WAITING.has(pr.mergeState))) continue
    byRepo.set(pr.repositoryId, [...(byRepo.get(pr.repositoryId) ?? []), pr])
  }
  const groups: CiTrainGroup[] = []
  for (const [repositoryId, prs] of byRepo) {
    const rows: CiTrainRow[] = []
    const batches = new Map<string, CiPagePr[]>()
    for (const pr of prs.filter((p) => p.mergeState === `testing`).sort((a, b) => a.number - b.number)) {
      const key = pr.mergeTrainId ?? pr.id
      batches.set(key, [...(batches.get(key) ?? []), pr])
    }
    for (const [key, members] of [...batches].sort((a, b) => a[1][0]!.number - b[1][0]!.number)) {
      rows.push({
        id: `batch.${key}`,
        identifier: `#${members[0]!.number}`,
        title:
          members.length === 1
            ? members[0]!.title
            : copy.batch.replace(`{count}`, String(members.length)),
        subtitle: members.length === 1 ? null : members.map((m) => `#${m.number} ${m.title}`).join(` · `),
        glyph: `ci-running`,
        badge: pill.testing,
        pullRequestId: members[0]!.id,
      })
    }
    if (queue) {
      const queued = prs
        .filter((p) => p.mergeState === `queued`)
        .sort((a, b) => (a.mergePosition ?? Number.MAX_SAFE_INTEGER) - (b.mergePosition ?? Number.MAX_SAFE_INTEGER) || a.number - b.number)
      for (const pr of queued) {
        rows.push({
          id: pr.id,
          identifier: `#${pr.number}`,
          title: pr.title,
          subtitle: null,
          glyph: `ci-queued`,
          badge: pr.mergePosition ? pill.queuedAt.replace(`{position}`, ordinal(pr.mergePosition)) : pill.queued,
          pullRequestId: pr.id,
        })
      }
      for (const pr of prs.filter((p) => p.mergeState !== `queued` && p.mergeState !== `testing`).sort((a, b) => a.number - b.number)) {
        const state = pr.mergeState as `checks` | `held` | `conflict` | `failing`
        rows.push({
          id: pr.id,
          identifier: `#${pr.number}`,
          title: pr.title,
          subtitle: null,
          glyph: state === `checks` ? `ci-queued` : state === `held` ? `ci-skipped` : `ci-failed`,
          badge: waiting[state],
          pullRequestId: pr.id,
        })
      }
    }
    if (rows.length === 0) continue
    const base = prs[0]!.base
    groups.push({ repositoryId, title: repoName(input.repos, repositoryId), caption: base, rows })
  }
  return groups.sort((a, b) => (a.title < b.title ? -1 : a.title > b.title ? 1 : a.repositoryId < b.repositoryId ? -1 : 1))
}

export interface CiRunnerRow {
  id: string
  title: string
  /** The labels it takes jobs for, OS first. */
  subtitle: string
  online: boolean
  /** `{busy} of {slots} busy` online; null offline (the meta is then
   *  copy.runners.offlineSince over `offlineSince`). */
  busy: string | null
  /** Queued jobs of ours this runner could take; null when none. */
  queue: string | null
  /** The last beat of an offline runner (ISO); null online or never seen. */
  offlineSince: string | null
}

/** A runner's labels: its OS (runner-settings.json `runnerOsLabel`), then
 *  its own, lower-cased (the dispatcher's rule). */
export function ciRunnerLabels(runner: Pick<CiPageRunner, `platform` | `labels`>): string[] {
  const os = runnerOsLabel(runner.platform)
  return [...new Set([...(os ? [os] : []), ...runner.labels.map((label) => label.toLowerCase())])]
}

/** The Runners tab: every runner device of the viewer's teams, online ones
 *  first, then by name. Busy = our jobs running on it; queue = our queued
 *  jobs whose `runsOn` it covers (no lanes: slots only). */
export function ciRunnerRows(input: Pick<CiPageInput, `runners` | `jobs` | `now`>): CiRunnerRow[] {
  const at = msOf(input.now) ?? 0
  const copy = CI_PAGE_COPY.runners
  const rows = input.runners.map((runner): CiRunnerRow => {
    const labels = ciRunnerLabels(runner)
    const isOnline = online(runner, at)
    const busy = input.jobs.filter((job) => job.status === `in_progress` && job.runnerDeviceId === runner.id).length
    const queued = input.jobs.filter(
      (job) =>
        ciSource(job.provider) === `exponential` &&
        job.status === `queued` &&
        job.runnerDeviceId === null &&
        job.runsOn.every((label) => labels.includes(label.toLowerCase()))
    ).length
    return {
      id: runner.id,
      title: runner.label,
      subtitle: labels.length > 0 ? labels.join(`, `) : copy.noLabels,
      online: isOnline,
      busy: isOnline ? copy.busy.replace(`{busy}`, String(busy)).replace(`{slots}`, String(runner.slots)) : null,
      queue: queued > 0 ? copy.queue.replace(`{count}`, String(queued)) : null,
      offlineSince: isOnline ? null : runner.lastSeenAt,
    }
  })
  return rows.sort((a, b) => Number(b.online) - Number(a.online) || (a.title < b.title ? -1 : a.title > b.title ? 1 : a.id < b.id ? -1 : 1))
}

/** Tab counts: the train's rows, the runners. */
export function ciTabCounts(input: CiPageInput): { train: number; runners: number } {
  return {
    train: ciTrainGroups(input, true).reduce((sum, group) => sum + group.rows.length, 0),
    runners: input.runners.length,
  }
}
