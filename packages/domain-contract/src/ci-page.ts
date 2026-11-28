import fixture from "../fixtures/ci-page.json" with { type: "json" }
import { ciRunStatus, runnerOsLabel, type CiRunStatus } from "./ci-run"
import { formatCheckDuration, type CheckGlyph } from "./pr-checks"
import { MERGE_TRAIN_COPY, MERGE_TRAIN_LIMITS, ordinal } from "./merge-train"
import { REPO_CI_COPY } from "./repo-ci"

// #EXP-2172 (fixtures/ci-page.json): THE CI page (one root row after Devices,
// across every member team) and its badge. #EXP-2214 (Danny 2026-11-28, "the
// ci page makes no sense"): four tabs, one meaning each: the MERGE QUEUE
// (what is being tested, what is next, what was sent back, what landed),
// PULL REQUESTS (one row per PR, its runs folded under it), RELEASES (tags,
// manual, scheduled and default-branch runs) and RUNNERS. The TS reference;
// the desktop (domain::ci_page), iOS (ExpCore CiPage) and Android
// (domain/CiPage.kt) replay the same fixture.

export const CI_PAGE_COPY = fixture.copy
export const CI_PAGE_LIMITS = fixture.limits
export const CI_PAGE_TEST_IDS = fixture.testIds

export type CiSource = `exponential` | `github`
export type CiFilter = `all` | `running` | `failed` | `mine`
export type CiTab = `train` | `prs` | `releases` | `runners`
/** The tab order (the merge queue opens first). */
export const CI_TABS: readonly CiTab[] = [`train`, `prs`, `releases`, `runners`]
/** #EXP-2214: what a run is for (`ciRunKind`). */
export type CiRunKind = `train` | `pull_request` | `release` | `branch`

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
  /** #EXP-2214: the merge queue run it tests (`ci_runs.merge_train_id`). */
  mergeTrainId: string | null
}

export interface CiPageJob {
  /** #EXP-2214: the Sent back row's Open log names a job. */
  id: string
  runId: string
  check: string
  status: string
  conclusion: string | null
  runsOn: string[]
  runnerDeviceId: string | null
  provider: string
}

export interface CiPageRepo {
  id: string
  fullName: string
  /** #EXP-2214: a push to it is a release run (null = `main`). */
  defaultBranch: string | null
}

export interface CiPagePr {
  id: string
  number: number
  title: string
  repositoryId: string
  rootSessionId: string | null
  base: string
  /** #EXP-2214: open | draft | merged | closed (Open vs Merged or closed). */
  state: string
  mergeState: string | null
  /** What a `checks` PR waits for (merge-train.json `mergeWait`). */
  mergeWait: string | null
  /** person | bounces | timeout | no_checks | conflict. */
  mergeHold: string | null
  mergeNow: boolean
  mergeAttempts: number
  mergePosition: number | null
  mergeTrainId: string | null
}

/** #EXP-2214: one merge queue run (`merge_trains`, the synced columns). */
export interface CiPageTrain {
  id: string
  repositoryId: string
  base: string
  baseSha: string | null
  /** PR ids per unit (a stack bottom first). */
  units: string[][]
  state: string
  attempt: number
  bisectOf: string | null
  /** The bisect halves still to test after it. */
  pending: string[][][]
  untested: boolean
  testedSha: string | null
  createdAt: string
  finishedAt: string | null
}

/** #EXP-2214: one row of the queue's history (`merge_train_events`). */
export interface CiPageTrainEvent {
  id: string
  repositoryId: string
  pullRequestId: string
  mergeTrainId: string | null
  /** landed | failing | conflict | held | merge_now | resumed. */
  kind: string
  base: string
  attempt: number
  reason: string | null
  checks: string[]
  ciJobId: string | null
  jobUrl: string | null
  files: string[]
  sha: string | null
  byUserId: string | null
  createdAt: string
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
  /** #EXP-2214: the merge queue's runs and history. */
  trains: CiPageTrain[]
  trainEvents: CiPageTrainEvent[]
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

/** #EXP-2214 (ci-page.json `kindCases`): what a run is for. A merge queue
 *  run (its trigger, its train, an `exp-train/` ref); a pull request's (its
 *  PR, or the pull_request trigger); a RELEASE run (a tag, a manual or
 *  scheduled run, a push to the repository's default branch, `main` when
 *  unknown); anything else is a BRANCH run (a push to another branch). */
export function ciRunKind(
  run: Pick<CiPageRun, `trigger` | `mergeTrainId` | `ref` | `pullRequestId` | `repositoryId`>,
  repos: readonly CiPageRepo[]
): CiRunKind {
  if (run.trigger === `merge_train` || run.mergeTrainId || (run.ref ?? ``).startsWith(`exp-train/`)) return `train`
  if (run.pullRequestId || run.trigger === `pull_request`) return `pull_request`
  if (run.trigger === `tag` || run.trigger === `manual` || run.trigger === `schedule`) return `release`
  const branch = repos.find((repo) => repo.id === run.repositoryId)?.defaultBranch ?? `main`
  return run.trigger === `push` && run.ref === branch ? `release` : `branch`
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

/** A merge queue run is its batch: `Testing #a, #b together` (#EXP-2214:
 *  its train's units, else, running, the repo's testing PRs). */
function batchTitle(run: CiPageRun, input: Pick<CiPageInput, `pullRequests` | `trains`>): string | null {
  const pullRequests = input.pullRequests
  const train = run.mergeTrainId ? input.trains.find((t) => t.id === run.mergeTrainId) : undefined
  if (train) {
    const numbers = train.units.flat().flatMap((id) => {
      const pr = pullRequests.find((p) => p.id === id)
      return pr ? [pr.number] : []
    })
    if (numbers.length === 1) return CI_PAGE_COPY.batchOne.replace(`{number}`, `#${numbers[0]}`)
    if (numbers.length > 1) return CI_PAGE_COPY.batchTitle.replace(`{numbers}`, numbers.map((n) => `#${n}`).join(`, `))
  }
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
    // #EXP-2214: a tag run reads as its tag (`ios-v0.14.150`).
    title: (run.trigger === `tag` && run.ref ? run.ref : null) ?? run.title ?? batchTitle(run, input) ?? pr?.title ?? run.name,
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

/** The runs of the filter (#EXP-2214: of `kinds` when given), newest first,
 *  at most `limits.runs`. */
export function ciRunList(input: CiPageInput, filter: CiRunFilter, kinds: readonly CiRunKind[] | null = null): CiRunListRow[] {
  const rows: CiRunListRow[] = []
  for (const run of newestFirst(input.runs)) {
    if (kinds && !kinds.includes(ciRunKind(run, input.repos))) continue
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
export function ciRunGroups(input: CiPageInput, filter: CiRunFilter, kinds: readonly CiRunKind[] | null = null): CiRunGroup[] {
  const rows = ciRunList(input, filter, kinds)
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

// ── #EXP-2214 PULL REQUESTS TAB ─────────────────────────────────────────

export interface CiPill {
  text: string
  tone: CiStatusTone
}

/** One subject of the Pull requests tab: a pull request (its runs), or a
 *  branch without one (`Branches`). */
export interface CiSubjectRow {
  /** The PR's id, else `branch.<repositoryId>.<ref>`. */
  id: string
  pullRequestId: string | null
  /** `#214`; null for a branch. */
  identifier: string | null
  title: string
  status: CiRunStatus
  glyph: CheckGlyph
  tone: CiStatusTone
  /** Wide: `repo · ref · name #n` of the run that decides the status. */
  subtitle: string
  /** Phones: `repo · name #n`. */
  phoneSubtitle: string
  /** That run's duration, else copy.queued. */
  meta: string
  /** The PR's merge queue state (wide only); null for a branch. */
  pill: CiPill | null
  /** The run a press opens (the deciding one). */
  runId: string
  /** Its runs, newest first, at most `limits.subjectRuns` (folded). */
  runs: CiRunListRow[]
}

export interface CiSubjectGroup {
  /** open | branches | closed. */
  id: `open` | `branches` | `closed`
  title: string
  count: number
  /** Merged or closed starts folded. */
  open: boolean
  rows: CiSubjectRow[]
}

const STATUS_RANK: CiRunStatus[] = [`failed`, `running`, `queued`, `passed`, `cancelled`]

/** A PR's merge queue state as a pill (merge-train.json copy.pill). */
export function ciMergePill(pr: CiPagePr): CiPill | null {
  const pill = MERGE_TRAIN_COPY.pill
  const waits = CI_PAGE_COPY.queue.waits as Record<string, string>
  switch (pr.mergeState) {
    case `testing`:
      return { text: pill.testing, tone: `info` }
    case `queued`:
      return { text: pr.mergePosition ? pill.queuedAt.replace(`{position}`, ordinal(pr.mergePosition)) : pill.queued, tone: `neutral` }
    case `merged`:
      return { text: pill.merged, tone: `success` }
    case `conflict`:
      return { text: pill.conflict, tone: `danger` }
    case `failing`:
      return { text: pill.failing, tone: `danger` }
    case `held`:
      return { text: pill.onHold, tone: `neutral` }
    case `draft`:
      return { text: pill.draft, tone: `neutral` }
    case `checks`:
      return {
        text: (pr.mergeWait ? waits[pr.mergeWait] : null) ?? pill.checksRunning,
        tone: pr.mergeWait === `failing` ? `danger` : (pr.mergeWait ?? `checks`) === `checks` ? `info` : `neutral`,
      }
    default:
      return null
  }
}

/** THE PULL REQUESTS TAB (ci-page.json `prGroupsCases`): pull request runs
 *  grouped by PR, branch runs by (repository, ref). A subject's status rolls
 *  up the NEWEST run of each pipeline name (failed, running, queued, passed,
 *  cancelled: the first present wins; the newest run with it decides the
 *  row's subtitle, meta and press). Sections: Open (open + draft PRs, and a
 *  PR not synced), Branches, Merged or closed (folded); each newest first by
 *  its newest run, at most `limits.subjects`; an empty section is left out.
 *  The filter applies to the SUBJECT: running = its status running or
 *  queued, failed = failed, mine = one of its runs is the viewer's. */
export function ciPrGroups(input: CiPageInput, filter: CiRunFilter): CiSubjectGroup[] {
  const subjects = new Map<string, { pr: CiPagePr | null; ref: string | null; repositoryId: string; runs: CiPageRun[] }>()
  for (const run of newestFirst(input.runs)) {
    const kind = ciRunKind(run, input.repos)
    if (kind !== `pull_request` && kind !== `branch`) continue
    if (filter.repositoryId && run.repositoryId !== filter.repositoryId) continue
    const pr = run.pullRequestId ? (input.pullRequests.find((p) => p.id === run.pullRequestId) ?? null) : null
    const key = run.pullRequestId ?? `branch.${run.repositoryId}.${run.ref ?? ``}`
    const subject = subjects.get(key) ?? { pr, ref: run.ref, repositoryId: run.repositoryId, runs: [] }
    subject.runs.push(run)
    subjects.set(key, subject)
  }
  const groups: Record<CiSubjectGroup[`id`], CiSubjectRow[]> = { open: [], branches: [], closed: [] }
  for (const [key, subject] of subjects) {
    const newestPerName = new Map<string, CiPageRun>()
    for (const run of subject.runs) if (!newestPerName.has(run.name)) newestPerName.set(run.name, run)
    const latest = [...newestPerName.values()].map((run) => ({ run, status: statusOf(run, input.jobs) }))
    const status = STATUS_RANK.find((s) => latest.some((l) => l.status === s)) ?? `queued`
    const deciding = latest.find((l) => l.status === status)!.run
    if (filter.status === `running` && status !== `running` && status !== `queued`) continue
    if (filter.status === `failed` && status !== `failed`) continue
    if (filter.status === `mine` && !subject.runs.some((run) => ciIsMine(run, input.viewerId, input.pullRequests, input.sessions))) continue
    const row = ciRunRow(deciding, input)
    const prId = subject.runs[0]!.pullRequestId
    const branch = !prId
    const group: CiSubjectGroup[`id`] = branch ? `branches` : subject.pr && (subject.pr.state === `merged` || subject.pr.state === `closed`) ? `closed` : `open`
    if (groups[group].length >= CI_PAGE_LIMITS.subjects) continue
    groups[group].push({
      id: key,
      pullRequestId: subject.pr?.id ?? prId ?? null,
      identifier: subject.pr ? `#${subject.pr.number}` : null,
      title: subject.pr?.title ?? (branch ? (subject.ref ?? REPO_CI_COPY.runs.noRef) : row.title),
      status,
      ...CI_STATUS_TONES[status],
      subtitle: [row.repo, row.ref, row.workflow].join(` · `),
      phoneSubtitle: [row.repo, row.workflow].join(` · `),
      meta: row.phoneMeta,
      pill: subject.pr ? ciMergePill(subject.pr) : null,
      runId: deciding.id,
      runs: subject.runs.slice(0, CI_PAGE_LIMITS.subjectRuns).map((run) => ciRunRow(run, input)),
    })
  }
  const copy = CI_PAGE_COPY.prs
  return (
    [
      { id: `open`, title: copy.open, open: true },
      { id: `branches`, title: copy.branches, open: true },
      { id: `closed`, title: copy.closed, open: false },
    ] as const
  ).flatMap((g) => (groups[g.id].length === 0 ? [] : [{ ...g, count: groups[g.id].length, rows: groups[g.id] }]))
}

// ── #EXP-2214 MERGE QUEUE TAB ────────────────────────────────────────────

const LIVE_TRAIN = new Set([`dispatching`, `composing`, `testing`, `green`])

export interface CiQueueMember {
  pullRequestId: string
  identifier: string
  title: string
  author: CiPageUser | null
}

/** The run testing now (one Card). */
export interface CiQueueNow {
  trainId: string
  /** The PR's title, else `Testing #a, #b together`. */
  title: string
  /** `main @ 3db841e` (+ ` · re-run`, ` · split: 2 more to test`, ` · Merge
   *  now, untested`). */
  subtitle: string
  members: CiQueueMember[]
  status: CiRunStatus
  /** The run Open run opens (the deciding CI run of the train); null before
   *  one exists. */
  runId: string | null
  /** `Running for 2m 40s`, else copy.waitingForRunner. */
  elapsed: string
  /** `about 3 min left` | copy.queue.almostDone; null without history. */
  eta: string | null
  /** 0–99 against the typical run; null without history. */
  progress: number | null
}

export type CiQueueAction = `open-log` | `resume` | null

export interface CiQueueRow {
  id: string
  pullRequestId: string
  identifier: string
  title: string
  subtitle: string | null
  glyph: string
  tone: CiStatusTone | `warning`
  /** A pill (Up next / Not ready yet). */
  badge: CiPill | null
  /** History rows: when it happened (drawn relative). */
  at: string | null
  /** The row's button: Open log (a red check's job or the forge's page) or
   *  Resume (held after the tries). */
  action: CiQueueAction
  runId: string | null
  jobId: string | null
  url: string | null
  /** Up next: the ⋯ menu (Merge now, Hold). */
  menu: boolean
}

export interface CiQueueStats {
  inQueue: number
  landedToday: number
  sentBackToday: number
  /** The typical (median) merge queue run of the repository; null without
   *  `limits.etaSamples` finished green runs. */
  typicalRun: string | null
}

export interface CiQueueRepo {
  repositoryId: string
  title: string
  /** copy.queue.into over the base (`into main`). */
  caption: string
  stats: CiQueueStats
  testing: CiQueueNow[]
  upNext: CiQueueRow[]
  waiting: CiQueueRow[]
  sentBack: CiQueueRow[]
  landed: CiQueueRow[]
}

/** The finished green merge queue runs of a repository: per train, the
 *  span of its CI runs (first start → last completion), newest first, at
 *  most `limits.etaSamples * 2`. */
function trainDurations(input: Pick<CiPageInput, `runs` | `jobs`>, repositoryId: string): number[] {
  const byTrain = new Map<string, CiPageRun[]>()
  for (const run of input.runs) {
    if (run.repositoryId !== repositoryId || !run.mergeTrainId) continue
    byTrain.set(run.mergeTrainId, [...(byTrain.get(run.mergeTrainId) ?? []), run])
  }
  const spans: Array<{ end: number; ms: number }> = []
  for (const runs of byTrain.values()) {
    if (!runs.every((run) => statusOf(run, input.jobs) === `passed`)) continue
    const starts = runs.map((run) => msOf(run.startedAt))
    const ends = runs.map((run) => msOf(run.completedAt))
    if (starts.some((v) => v === null) || ends.some((v) => v === null)) continue
    const start = Math.min(...(starts as number[]))
    const end = Math.max(...(ends as number[]))
    spans.push({ end, ms: end - start })
  }
  return spans
    .sort((a, b) => b.end - a.end)
    .slice(0, CI_PAGE_LIMITS.etaSamples * 2)
    .map((s) => s.ms)
}

/** The median of the repository's finished green merge queue runs (the last
 *  `limits.etaSamples * 2`), null with fewer than `limits.etaSamples`. */
export function ciTypicalTrainMs(input: Pick<CiPageInput, `runs` | `jobs`>, repositoryId: string): number | null {
  const spans = trainDurations(input, repositoryId).sort((a, b) => a - b)
  if (spans.length < CI_PAGE_LIMITS.etaSamples) return null
  const mid = Math.floor(spans.length / 2)
  return spans.length % 2 === 1 ? spans[mid]! : Math.round((spans[mid - 1]! + spans[mid]!) / 2)
}

/** `trainEta` (ci-page.json `etaCases`): what is left of a run that has
 *  been going `elapsedMs` against the typical run: `about {n} min left`
 *  (n = whole minutes, rounded up) while more than a minute is left, else
 *  copy.queue.almostDone; progress = elapsed / typical in percent, 0–99. */
export function ciTrainEta(typicalMs: number | null, elapsedMs: number): { eta: string; progress: number } | null {
  if (typicalMs === null || typicalMs <= 0) return null
  const left = typicalMs - elapsedMs
  const copy = CI_PAGE_COPY.queue
  return {
    eta: left > 60_000 ? copy.left.replace(`{n}`, String(Math.ceil(left / 60_000))) : copy.almostDone,
    progress: Math.max(0, Math.min(99, Math.floor((elapsedMs / typicalMs) * 100))),
  }
}

function authorOf(pr: CiPagePr, input: Pick<CiPageInput, `sessions` | `users`>): CiPageUser | null {
  const starter = pr.rootSessionId ? input.sessions.find((s) => s.id === pr.rootSessionId)?.userId : undefined
  return starter ? (input.users.find((user) => user.id === starter) ?? null) : null
}

const listOf = (items: string[], max: number): string =>
  items.length <= max ? items.join(`, `) : `${items.slice(0, max).join(`, `)} +${items.length - max}`

/** THE MERGE QUEUE TAB (ci-page.json `queueCases`), per repository by name
 *  (one with nothing live and no history in `limits.historyHours` is left
 *  out):
 *  TESTING = its live trains (dispatching | composing | testing | green, not
 *  finished), oldest first: the members (units in order), the subtitle, the
 *  status of its CI runs (none yet = queued), Open run = the newest failing,
 *  else running, else newest run of the train; elapsed from the first run's
 *  start; the ETA against `ciTypicalTrainMs`.
 *  UP NEXT = its `queued` open PRs by position (then number): a Merge now
 *  reads copy.queue.mergeNow, else the position (`2nd in line`); the ⋯ menu.
 *  NOT READY YET = its open PRs in checks | held | conflict | failing by
 *  number, the pill = the wait (copy.queue.waits[merge_wait], held: the
 *  hold reason's copy.queue.holds, conflict / failing: copy.queue.waits);
 *  Resume on a PR held by the limits (not a person's Hold).
 *  SENT BACK = its failing and conflict events in `limits.historyHours`,
 *  newest first, at most `limits.history`: failing = copy.queue.failedIn
 *  over the checks (two, then +N) and the tries; Open log = the event's job
 *  (its run from the jobs), else its job URL, else none; conflict =
 *  copy.queue.conflictIn over the files' names (two, then +N). A conflict
 *  or failing PR with a Sent back row is not listed again in Not ready yet.
 *  LANDED = its landed events in the window, newest first, at most
 *  `limits.history`: `with #a, #b · sha7` (the train's other landed PRs),
 *  an untested train copy.queue.mergedNow.
 *  STATS: in the queue = testing members + up next; landed / sent back
 *  today (failing + conflict) by the viewer's local day; the typical run. */
export function ciQueue(input: CiPageInput): CiQueueRepo[] {
  const copy = CI_PAGE_COPY.queue
  const now = msOf(input.now) ?? 0
  const since = now - CI_PAGE_LIMITS.historyHours * 3_600_000
  const today = localDay(now, input.utcOffsetMinutes)
  const prOf = (id: string) => input.pullRequests.find((pr) => pr.id === id)
  const repoIds = new Set<string>()
  for (const t of input.trains) if (LIVE_TRAIN.has(t.state) && !t.finishedAt) repoIds.add(t.repositoryId)
  for (const pr of input.pullRequests) if (pr.state === `open` && pr.mergeState && [`queued`, `testing`, `held`, `conflict`, `failing`].includes(pr.mergeState)) repoIds.add(pr.repositoryId)
  for (const e of input.trainEvents) if ((msOf(e.createdAt) ?? 0) >= since) repoIds.add(e.repositoryId)
  const out: CiQueueRepo[] = []
  for (const repositoryId of repoIds) {
    const typical = ciTypicalTrainMs(input, repositoryId)
    const testing: CiQueueNow[] = input.trains
      .filter((t) => t.repositoryId === repositoryId && LIVE_TRAIN.has(t.state) && !t.finishedAt)
      .sort((a, b) => (msOf(a.createdAt) ?? 0) - (msOf(b.createdAt) ?? 0) || (a.id < b.id ? -1 : 1))
      .map((train) => {
        const members = train.units.flat().flatMap((id) => {
          const pr = prOf(id)
          return pr ? [{ pullRequestId: pr.id, identifier: `#${pr.number}`, title: pr.title, author: authorOf(pr, input) }] : []
        })
        const runs = newestFirst(input.runs.filter((run) => run.mergeTrainId === train.id))
        const statuses = runs.map((run) => ({ run, status: statusOf(run, input.jobs) }))
        const status = STATUS_RANK.find((s) => statuses.some((r) => r.status === s)) ?? `queued`
        const deciding = statuses.find((r) => r.status === status)?.run ?? null
        const starts = runs.flatMap((run) => (msOf(run.startedAt) === null ? [] : [msOf(run.startedAt)!]))
        const started = starts.length > 0 ? Math.min(...starts) : null
        const eta = started === null ? null : ciTrainEta(typical, now - started)
        const parts = [train.baseSha ? copy.base.replace(`{base}`, train.base).replace(`{sha}`, train.baseSha.slice(0, 7)) : train.base]
        if (train.attempt > 0) parts.push(copy.rerun)
        if (train.bisectOf && train.pending.length > 0) parts.push(copy.split.replace(`{count}`, String(train.pending.length)))
        if (train.untested) parts.push(copy.untested)
        return {
          trainId: train.id,
          title:
            members.length === 1
              ? members[0]!.title
              : CI_PAGE_COPY.batchTitle.replace(`{numbers}`, members.map((m) => m.identifier).join(`, `)),
          subtitle: parts.join(` · `),
          members,
          status,
          runId: deciding?.id ?? null,
          elapsed: started === null ? CI_PAGE_COPY.waitingForRunner : copy.runningFor.replace(`{duration}`, formatCheckDuration(now - started)),
          eta: eta?.eta ?? null,
          progress: eta?.progress ?? null,
        }
      })
    const open = input.pullRequests.filter((pr) => pr.repositoryId === repositoryId && pr.state === `open`)
    const upNext: CiQueueRow[] = open
      .filter((pr) => pr.mergeState === `queued`)
      .sort((a, b) => (a.mergePosition ?? Number.MAX_SAFE_INTEGER) - (b.mergePosition ?? Number.MAX_SAFE_INTEGER) || a.number - b.number)
      .map((pr) => ({
        id: pr.id,
        pullRequestId: pr.id,
        identifier: `#${pr.number}`,
        title: pr.title,
        subtitle: pr.mergeNow ? copy.mergeNow : pr.mergePosition ? copy.position.replace(`{ordinal}`, ordinal(pr.mergePosition)) : null,
        glyph: pr.mergeNow ? `zap` : `ci-queued`,
        tone: pr.mergeNow ? `warning` : `neutral`,
        badge: null,
        at: null,
        action: null,
        runId: null,
        jobId: null,
        url: null,
        menu: true,
      }))
    const waits = copy.waits as Record<string, string>
    const holds = copy.holds as Record<string, string>
    const waiting: CiQueueRow[] = open
      .filter((pr) => pr.mergeState === `checks` || pr.mergeState === `held` || pr.mergeState === `conflict` || pr.mergeState === `failing`)
      .sort((a, b) => a.number - b.number)
      .map((pr) => {
        const held = pr.mergeState === `held`
        const text = held ? (holds[pr.mergeHold ?? `person`] ?? holds.person!) : pr.mergeState === `checks` ? (waits[pr.mergeWait ?? `checks`] ?? waits.checks!) : waits[pr.mergeState!]!
        const tone: CiStatusTone = pr.mergeState === `conflict` || pr.mergeState === `failing` || (pr.mergeState === `checks` && pr.mergeWait === `failing`) ? `danger` : pr.mergeState === `checks` && (pr.mergeWait ?? `checks`) === `checks` ? `info` : `neutral`
        return {
          id: pr.id,
          pullRequestId: pr.id,
          identifier: `#${pr.number}`,
          title: pr.title,
          subtitle: null,
          glyph: held ? `ci-skipped` : tone === `danger` ? `ci-failed` : `ci-queued`,
          tone: tone === `info` ? `neutral` : tone,
          badge: { text, tone },
          at: null,
          action: held && pr.mergeHold !== null && pr.mergeHold !== `person` ? (`resume` as const) : null,
          runId: null,
          jobId: null,
          url: null,
          menu: false,
        }
      })
    const events = input.trainEvents
      .filter((e) => e.repositoryId === repositoryId && (msOf(e.createdAt) ?? 0) >= since)
      .sort((a, b) => (msOf(b.createdAt) ?? 0) - (msOf(a.createdAt) ?? 0) || (a.id < b.id ? -1 : 1))
    const tries = (n: number) => copy.tries.replace(`{n}`, String(n)).replace(`{max}`, String(MERGE_TRAIN_LIMITS.bounces))
    const sentBack: CiQueueRow[] = events
      .filter((e) => e.kind === `failing` || e.kind === `conflict`)
      .slice(0, CI_PAGE_LIMITS.history)
      .flatMap((e) => {
        const pr = prOf(e.pullRequestId)
        if (!pr) return []
        const job = e.ciJobId ? input.jobs.find((j) => j.id === e.ciJobId) : undefined
        const subtitle =
          e.kind === `failing`
            ? `${copy.failedIn.replace(`{checks}`, listOf(e.checks, 2))} · ${tries(e.attempt)}`
            : `${copy.conflictIn.replace(`{files}`, listOf(e.files.map((f) => f.split(`/`).pop() ?? f), 2))} · ${tries(e.attempt)}`
        const action: CiQueueAction = e.kind === `failing` && (job || e.jobUrl) ? `open-log` : null
        return [
          {
            id: e.id,
            pullRequestId: pr.id,
            identifier: `#${pr.number}`,
            title: pr.title,
            subtitle,
            glyph: e.kind === `conflict` ? `pr-conflict` : `ci-failed`,
            tone: `danger` as const,
            badge: null,
            at: e.createdAt,
            action,
            runId: action === `open-log` ? (job?.runId ?? null) : null,
            jobId: action === `open-log` ? (job?.id ?? null) : null,
            url: action === `open-log` && !job ? e.jobUrl : null,
            menu: false,
          },
        ]
      })
    // A PR sent back in the window is listed there, not again here.
    const sentBackPrs = new Set(sentBack.map((row) => row.pullRequestId))
    const notReady = waiting.filter((row) => {
      const pr = prOf(row.pullRequestId)!
      return !((pr.mergeState === `conflict` || pr.mergeState === `failing`) && sentBackPrs.has(pr.id))
    })
    const landedEvents = events.filter((e) => e.kind === `landed`)
    const landed: CiQueueRow[] = landedEvents.slice(0, CI_PAGE_LIMITS.history).flatMap((e) => {
      const pr = prOf(e.pullRequestId)
      if (!pr) return []
      const train = e.mergeTrainId ? input.trains.find((t) => t.id === e.mergeTrainId) : undefined
      const sha = e.sha ? e.sha.slice(0, 7) : null
      const others = landedEvents
        .filter((o) => o.mergeTrainId && o.mergeTrainId === e.mergeTrainId && o.pullRequestId !== e.pullRequestId)
        .flatMap((o) => {
          const p = prOf(o.pullRequestId)
          return p ? [p.number] : []
        })
        .sort((a, b) => a - b)
      const parts = train?.untested ? [copy.mergedNow] : others.length > 0 ? [copy.landedWith.replace(`{numbers}`, others.map((n) => `#${n}`).join(`, `))] : []
      if (sha) parts.push(sha)
      return [
        {
          id: e.id,
          pullRequestId: pr.id,
          identifier: `#${pr.number}`,
          title: pr.title,
          subtitle: parts.length > 0 ? parts.join(` · `) : null,
          glyph: train?.untested ? `zap` : `ci-passed`,
          tone: train?.untested ? (`warning` as const) : (`success` as const),
          badge: null,
          at: e.createdAt,
          action: null,
          runId: null,
          jobId: null,
          url: null,
          menu: false,
        },
      ]
    })
    const isToday = (e: CiPageTrainEvent) => localDay(msOf(e.createdAt) ?? 0, input.utcOffsetMinutes) === today
    const base = testing.length > 0 ? input.trains.find((t) => t.id === testing[0]!.trainId)!.base : (open.find((pr) => pr.mergeState === `queued`)?.base ?? events[0]?.base ?? open[0]?.base ?? `main`)
    out.push({
      repositoryId,
      title: repoName(input.repos, repositoryId),
      caption: copy.into.replace(`{base}`, base),
      stats: {
        inQueue: testing.reduce((sum, t) => sum + t.members.length, 0) + upNext.length,
        landedToday: landedEvents.filter(isToday).length,
        sentBackToday: events.filter((e) => (e.kind === `failing` || e.kind === `conflict`) && isToday(e)).length,
        typicalRun: typical === null ? null : formatCheckDuration(typical),
      },
      testing,
      upNext,
      waiting: notReady,
      sentBack,
      landed,
    })
  }
  return out.sort((a, b) => (a.title < b.title ? -1 : a.title > b.title ? 1 : a.repositoryId < b.repositoryId ? -1 : 1))
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

/** #EXP-2214 tab counts (after each label, muted; 0 = none): the merge
 *  queue's PRs (testing + up next), the Open pull requests, the release runs
 *  in progress, the runners. */
export function ciTabCounts(input: CiPageInput): Record<CiTab, number> {
  const all: CiRunFilter = { status: `all`, repositoryId: null, source: null }
  return {
    train: ciQueue(input).reduce((sum, repo) => sum + repo.stats.inQueue, 0),
    prs: ciPrGroups(input, all).find((group) => group.id === `open`)?.count ?? 0,
    releases: ciRunGroups(input, all, [`release`]).find((group) => group.id === `in-progress`)?.count ?? 0,
    runners: input.runners.length,
  }
}

// ── #EXP-2214 THE RUN PAGE'S CONTEXT ─────────────────────────────────────

export interface CiRunOutcome {
  pullRequestId: string
  identifier: string
  title: string
  text: string
  glyph: string
  tone: CiStatusTone | `warning`
  /** A failing PR's red job IN THIS RUN (Open log selects it). */
  jobId: string | null
}

export interface CiRunContext {
  kind: CiRunKind
  /** copy.kinds[kind]; a pull request run `Pull request #n`. */
  badge: string
  /** A merge queue run's batch title (`Testing #a, #b together`), else null
   *  (the run keeps its own title). */
  title: string | null
  /** A merge queue run: copy.queue.base over its train (`main @ 3db841e`). */
  caption: string | null
  /** A merge queue run: what happened to each PR of its train, in unit
   *  order (none for other kinds or an unknown train). */
  outcome: CiRunOutcome[]
}

/** `runContextCases`: what the run page says about WHY the run exists. A
 *  merge queue run lists its train's PRs: the PR's latest landed / failing /
 *  conflict event of that train decides (landed = copy.queue.outcome.landed
 *  over the sha, an untested train copy.queue.outcome.untested; failing =
 *  .failing over the checks and tries, Open log when its job is one of this
 *  run's; conflict = .conflict over the files), else still testing in it =
 *  .testing, else .requeued (an innocent unit of a red batch, a cancelled
 *  run). */
export function ciRunContext(
  run: CiPageRun,
  input: Pick<CiPageInput, `repos` | `pullRequests` | `trains` | `trainEvents` | `jobs` | `runs`>
): CiRunContext {
  const kind = ciRunKind(run, input.repos)
  const kinds = CI_PAGE_COPY.kinds as Record<CiRunKind, string>
  const pr = run.pullRequestId ? input.pullRequests.find((p) => p.id === run.pullRequestId) : undefined
  const badge = kind === `pull_request` && pr ? CI_PAGE_COPY.pullRequest.replace(`{number}`, String(pr.number)) : kinds[kind]
  const train = run.mergeTrainId ? input.trains.find((t) => t.id === run.mergeTrainId) : undefined
  if (kind !== `train` || !train) return { kind, badge, title: kind === `train` ? batchTitle(run, input) : null, caption: null, outcome: [] }
  const copy = CI_PAGE_COPY.queue
  const out = copy.outcome
  const tries = (n: number) => copy.tries.replace(`{n}`, String(n)).replace(`{max}`, String(MERGE_TRAIN_LIMITS.bounces))
  const outcome = train.units.flat().flatMap((id): CiRunOutcome[] => {
    const member = input.pullRequests.find((p) => p.id === id)
    if (!member) return []
    const event = input.trainEvents
      .filter((e) => e.pullRequestId === id && e.mergeTrainId === train.id && (e.kind === `landed` || e.kind === `failing` || e.kind === `conflict`))
      .sort((a, b) => (msOf(b.createdAt) ?? 0) - (msOf(a.createdAt) ?? 0))[0]
    const base = { pullRequestId: id, identifier: `#${member.number}`, title: member.title }
    if (event?.kind === `landed`) {
      return [{ ...base, text: train.untested ? out.untested : out.landed.replace(`{sha}`, (event.sha ?? ``).slice(0, 7)), glyph: train.untested ? `zap` : `ci-passed`, tone: train.untested ? `warning` : `success`, jobId: null }]
    }
    if (event?.kind === `failing`) {
      const job = event.ciJobId ? input.jobs.find((j) => j.id === event.ciJobId && j.runId === run.id) : undefined
      return [{ ...base, text: out.failing.replace(`{checks}`, listOf(event.checks, 2)).replace(`{tries}`, tries(event.attempt)), glyph: `ci-failed`, tone: `danger`, jobId: job?.id ?? null }]
    }
    if (event?.kind === `conflict`) {
      return [{ ...base, text: out.conflict.replace(`{files}`, listOf(event.files.map((f) => f.split(`/`).pop() ?? f), 2)).replace(`{tries}`, tries(event.attempt)), glyph: `pr-conflict`, tone: `danger`, jobId: null }]
    }
    if (member.mergeState === `testing` && member.mergeTrainId === train.id) return [{ ...base, text: out.testing, glyph: `ci-running`, tone: `info`, jobId: null }]
    return [{ ...base, text: out.requeued, glyph: `ci-queued`, tone: `neutral`, jobId: null }]
  })
  return {
    kind,
    badge,
    title: batchTitle(run, input),
    caption: train.baseSha ? copy.base.replace(`{base}`, train.base).replace(`{sha}`, train.baseSha.slice(0, 7)) : train.base,
    outcome,
  }
}
