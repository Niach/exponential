import fixture from "../fixtures/ci-page.json" with { type: "json" }
import { ciRunStatus, runnerOsLabel, type CiRunStatus } from "./ci-run"
import { formatCheckDuration, type CheckGlyph } from "./pr-checks"
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
  /** #EXP-2172 redesign: what the run is about (`ci_runs.title`). */
  title: string | null
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
  /** The people a run names (its author), from the users shape. */
  users: CiPageUser[]
  viewerId: string
  now: string
  /** The viewer's clock offset from UTC in minutes (Today / Yesterday). */
  utcOffsetMinutes: number
}

export interface CiPageUser {
  id: string
  name: string
  image: string | null
}

const msOf = (value: string | null | undefined): number | null => {
  if (!value) return null
  const ms = Date.parse(value)
  return Number.isNaN(ms) ? null : ms
}

/** The one status table (ci-page.json `statusTones`): running is the only
 *  blue, moving mark; queued and cancelled stay neutral. */
export type CiStatusTone = `neutral` | `info` | `success` | `danger`
export const CI_STATUS_TONES = fixture.statusTones as Record<CiRunStatus, { glyph: CheckGlyph; tone: CiStatusTone }>

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

export interface CiRunListRow {
  id: string
  status: CiRunStatus
  statusLabel: string
  glyph: CheckGlyph
  tone: CiStatusTone
  /** The commit / PR line (`title`), else the merge train batch, else the
   *  pipeline's name. */
  title: string
  /** `name #n`. */
  workflow: string
  repositoryId: string
  repository: string
  /** The repository's own name (after the owner). */
  repo: string
  ref: string
  sha: string
  trigger: string
  source: CiSource
  sourceLabel: string
  sourceIcon: `github` | `brand-exponential`
  /** null before it started. */
  duration: string | null
  /** When it started, else when it was queued (the age, relative). */
  at: string
  author: CiPageUser | null
  /** Wide: `workflow · repo · ref · sha · trigger`. */
  subtitle: string
  /** Phones: `repo · ref · workflow`. */
  phoneSubtitle: string
  /** Phones' meta: the duration, else copy.queued. */
  phoneMeta: string
}

export interface CiRunGroup {
  /** `in-progress`, else the local day `yyyy-mm-dd`. */
  id: string
  title: string
  /** In progress only. */
  count: number | null
  rows: CiRunListRow[]
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

/** A running merge train run is its batch: `Testing #a, #b together`. */
function batchTitle(run: CiPageRun, pullRequests: readonly CiPagePr[]): string | null {
  if (run.trigger !== `merge_train` || run.state !== `pending`) return null
  const members = pullRequests
    .filter((pr) => pr.repositoryId === run.repositoryId && pr.mergeState === `testing`)
    .map((pr) => pr.number)
    .sort((a, b) => a - b)
  if (members.length === 0) return null
  return CI_PAGE_COPY.batchTitle.replace(`{numbers}`, members.map((n) => `#${n}`).join(`, `))
}

/** One row of the list (both form factors' strings). */
export function ciRunRow(run: CiPageRun, input: CiPageInput): CiRunListRow {
  const at = msOf(input.now) ?? 0
  const status = statusOf(run, input.jobs)
  const source = ciSource(run.provider)
  const started = msOf(run.startedAt)
  const repository = repoName(input.repos, run.repositoryId)
  const repo = repository.split(`/`).pop() ?? repository
  const ref = run.ref ?? REPO_CI_COPY.runs.noRef
  const sha = run.sha.slice(0, 7)
  const workflow = `${run.name} #${run.number}`
  const trigger = triggerText(run, input.pullRequests)
  const pr = run.pullRequestId ? input.pullRequests.find((p) => p.id === run.pullRequestId) : undefined
  const duration = started === null ? null : formatCheckDuration((msOf(run.completedAt) ?? at) - started)
  return {
    id: run.id,
    status,
    statusLabel: REPO_CI_COPY.status[status],
    ...CI_STATUS_TONES[status],
    title: run.title ?? batchTitle(run, input.pullRequests) ?? pr?.title ?? run.name,
    workflow,
    repositoryId: run.repositoryId,
    repository,
    repo,
    ref,
    sha,
    trigger,
    source,
    sourceLabel: CI_PAGE_COPY.sources[source],
    sourceIcon: source === `github` ? `github` : `brand-exponential`,
    duration,
    at: run.startedAt ?? run.createdAt,
    author: run.createdByUserId ? (input.users.find((user) => user.id === run.createdByUserId) ?? null) : null,
    subtitle: [workflow, repo, ref, sha, trigger].join(` · `),
    phoneSubtitle: [repo, ref, workflow].join(` · `),
    phoneMeta: duration ?? CI_PAGE_COPY.queued,
  }
}

/** The Runs tab: every run of the filter, newest first, at most `limits.runs`. */
export function ciRunList(input: CiPageInput, filter: CiRunFilter): CiRunListRow[] {
  const rows: CiRunListRow[] = []
  for (const run of newestFirst(input.runs)) {
    const status = statusOf(run, input.jobs)
    const source = ciSource(run.provider)
    if (filter.repositoryId && run.repositoryId !== filter.repositoryId) continue
    if (filter.source && source !== filter.source) continue
    if (filter.status === `running` && status !== `running` && status !== `queued`) continue
    if (filter.status === `failed` && status !== `failed`) continue
    if (filter.status === `mine` && !ciIsMine(run, input.viewerId, input.pullRequests, input.sessions)) continue
    rows.push(ciRunRow(run, input))
    if (rows.length >= CI_PAGE_LIMITS.runs) break
  }
  return rows
}

const MONTHS = fixture.copy.months

/** The local day of an instant, `yyyy-mm-dd`, at the viewer's offset. */
function localDay(ms: number, offsetMinutes: number): string {
  return new Date(ms + offsetMinutes * 60_000).toISOString().slice(0, 10)
}

/** A day's section title: Today, Yesterday, else `27 Nov` (+ the year
 *  when it is not this year's). */
export function ciDayTitle(day: string, now: string, offsetMinutes: number): string {
  const nowMs = msOf(now) ?? 0
  const today = localDay(nowMs, offsetMinutes)
  if (day === today) return CI_PAGE_COPY.today
  if (day === localDay(nowMs - 86_400_000, offsetMinutes)) return CI_PAGE_COPY.yesterday
  const [y, m, d] = day.split(`-`)
  const label = `${Number(d)} ${MONTHS[Number(m) - 1]}`
  return y === today.slice(0, 4) ? label : `${label} ${y}`
}

/** The Runs tab's sections: In progress (running + queued, with a count),
 *  then one per local day, newest first. */
export function ciRunGroups(input: CiPageInput, filter: CiRunFilter): CiRunGroup[] {
  const rows = ciRunList(input, filter)
  const live = rows.filter((row) => row.status === `running` || row.status === `queued`)
  const groups: CiRunGroup[] = []
  if (live.length > 0) groups.push({ id: `in-progress`, title: CI_PAGE_COPY.inProgress, count: live.length, rows: live })
  for (const row of rows) {
    if (row.status === `running` || row.status === `queued`) continue
    const day = localDay(msOf(row.at) ?? 0, input.utcOffsetMinutes)
    const last = groups[groups.length - 1]
    if (last && last.id === day) last.rows.push(row)
    else groups.push({ id: day, title: ciDayTitle(day, input.now, input.utcOffsetMinutes), count: null, rows: [row] })
  }
  return groups
}

/** The repository pick: `All repositories`, then every repository that has
 *  a run, by name. */
export function ciRepoOptions(input: Pick<CiPageInput, `runs` | `repos`>): Array<{ label: string; value: string }> {
  const ids = [...new Set(input.runs.map((run) => run.repositoryId))]
  const named = ids.map((id) => ({ label: repoName(input.repos, id), value: id }))
  named.sort((a, b) => (a.label < b.label ? -1 : a.label > b.label ? 1 : a.value < b.value ? -1 : 1))
  return [{ label: CI_PAGE_COPY.repositories.all, value: `all` }, ...named]
}

/** A runner is online while its last beat is younger than this. */
export const RUNNER_ONLINE_MS: number = fixture.limits.runnerOnlineMs

function online(runner: CiPageRunner, now: number): boolean {
  const seen = msOf(runner.lastSeenAt)
  return seen !== null && now - seen < RUNNER_ONLINE_MS
}

/** The toolbar's runner chip (it opens the Runners tab): `{online} of
 *  {total} runners`, a green dot while one is online. Null without runners. */
export function ciRunnerChip(input: Pick<CiPageInput, `runners` | `now`>): { label: string; online: boolean } | null {
  if (input.runners.length === 0) return null
  const at = msOf(input.now) ?? 0
  const count = input.runners.filter((runner) => online(runner, at)).length
  return {
    label: CI_PAGE_COPY.runnerChip.replace(`{online}`, String(count)).replace(`{total}`, String(input.runners.length)),
    online: count > 0,
  }
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
