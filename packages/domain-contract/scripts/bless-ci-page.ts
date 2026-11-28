// #EXP-2172: write fixtures/ci-page.json's cases from the TS reference
// (src/ci-page.ts). Run after a deliberate rule change and REVIEW the diff:
//   bun packages/domain-contract/scripts/bless-ci-page.ts
// #EXP-2214: the scenario (the demo team: runs of every kind, the merge
// queue's trains and history) is authored IN the fixture (`scenario`); this
// writes only the expectations.
import { readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import {
  ciBadge,
  ciDayTitle,
  ciPrGroups,
  ciQueue,
  ciRepoOptions,
  ciRunGroups,
  ciRunContext,
  ciRunKind,
  ciRunnerChip,
  ciRunnerRows,
  ciTabCounts,
  ciTrainEta,
  ciTypicalTrainMs,
  type CiPageInput,
  type CiPageJob,
  type CiPageRun,
  type CiRunFilter,
} from "../src/ci-page"

const path = join(import.meta.dir, `../fixtures/ci-page.json`)
const fixture = JSON.parse(readFileSync(path, `utf8`))

const NOW = `2026-11-27T10:30:00.000Z`
const ME = `u-me`
const run = (r: Partial<CiPageRun> & Pick<CiPageRun, `id` | `repositoryId` | `number` | `name` | `state`>): CiPageRun => ({
  sha: ((r.number * 2654435761) >>> 0).toString(16).padStart(8, `0`).repeat(5),
  ref: `main`,
  trigger: `push`,
  pullRequestId: null,
  createdByUserId: null,
  provider: `exponential`,
  startedAt: null,
  completedAt: null,
  createdAt: r.startedAt ?? NOW,
  title: null,
  mergeTrainId: null,
  ...r,
})
const job = (runId: string, status: string, conclusion: string | null, extra: Partial<CiPageJob> = {}): CiPageJob => ({
  id: `${runId}-${status}-${conclusion ?? `none`}`,
  runId,
  check: `gate`,
  status,
  conclusion,
  runsOn: [],
  runnerDeviceId: null,
  provider: `exponential`,
  ...extra,
})

export const SCENARIO: CiPageInput = fixture.scenario

const filters = [
  { name: `all`, filter: { status: `all`, repositoryId: null, source: null } },
  { name: `running = running and queued`, filter: { status: `running`, repositoryId: null, source: null } },
  { name: `failed`, filter: { status: `failed`, repositoryId: null, source: null } },
  { name: `mine: started by me or my PR's chat`, filter: { status: `mine`, repositoryId: null, source: null } },
  { name: `one repository`, filter: { status: `all`, repositoryId: `repo-mobile`, source: null } },
  { name: `GitHub Actions only`, filter: { status: `all`, repositoryId: null, source: `github` } },
  { name: `ours, failed`, filter: { status: `failed`, repositoryId: null, source: `exponential` } },
] as const

fixture.runGroupsCases = filters.map((c) => ({ name: c.name, filter: c.filter, expected: ciRunGroups(SCENARIO, c.filter) }))
fixture.dayTitleCases = [
  { day: `2026-11-27`, now: NOW, utcOffsetMinutes: 60 },
  { day: `2026-11-26`, now: NOW, utcOffsetMinutes: 60 },
  { day: `2026-11-20`, now: NOW, utcOffsetMinutes: 60 },
  { day: `2025-12-31`, now: NOW, utcOffsetMinutes: 60 },
  { day: `2026-11-28`, now: `2026-11-27T23:30:00.000Z`, utcOffsetMinutes: 60 },
].map((c) => ({ ...c, expected: ciDayTitle(c.day, c.now, c.utcOffsetMinutes) }))
fixture.repoOptions = ciRepoOptions(SCENARIO)
fixture.runnerChip = ciRunnerChip(SCENARIO)
fixture.tabCounts = ciTabCounts(SCENARIO)
delete fixture.trainCases
// #EXP-2214: what every scenario run is for, plus the edges.
const kindRepos = [{ id: `repo-x`, fullName: `acme/x`, defaultBranch: `master` }]
const kindRun = (r: Partial<CiPageRun>) => ({ trigger: `push`, mergeTrainId: null, ref: `master`, pullRequestId: null, repositoryId: `repo-x`, ...r })
fixture.kindCases = [
  ...SCENARIO.runs.map((r) => ({ name: `scenario ${r.id}`, run: { trigger: r.trigger, mergeTrainId: r.mergeTrainId, ref: r.ref, pullRequestId: r.pullRequestId, repositoryId: r.repositoryId }, repos: SCENARIO.repos })),
  { name: `a push to the default branch (master) is a release`, run: kindRun({}), repos: kindRepos },
  { name: `a push to main where master is the default is a branch run`, run: kindRun({ ref: `main` }), repos: kindRepos },
  { name: `an unknown repository's default is main`, run: kindRun({ ref: `main`, repositoryId: `repo-gone` }), repos: kindRepos },
  { name: `a push to an exp-train ref is the merge queue's (GitHub)`, run: kindRun({ ref: `exp-train/0b6c2a1e-5f1d-4d55-9d43-1b1b1b1b1b1b` }), repos: kindRepos },
  { name: `a manual run on a branch is a release run`, run: kindRun({ trigger: `manual`, ref: `exp/APP-9` }), repos: kindRepos },
  { name: `a push with a pull request is the PR's`, run: kindRun({ ref: `exp/APP-9`, pullRequestId: `pr-1` }), repos: kindRepos },
  { name: `a bare commit (no ref) push is a branch run`, run: kindRun({ ref: null }), repos: kindRepos },
].map((c) => ({ ...c, expected: ciRunKind(c.run as CiPageRun, c.repos) }))
const all: CiRunFilter = { status: `all`, repositoryId: null, source: null }
fixture.releaseGroupsCases = [
  { name: `release runs: tags, manual, schedule, default-branch pushes`, filter: all },
  { name: `release runs of one repository`, filter: { ...all, repositoryId: `repo-site` } },
].map((c) => ({ ...c, expected: ciRunGroups(SCENARIO, c.filter, [`release`]) }))
fixture.prGroupsCases = [
  { name: `all`, filter: all },
  { name: `failed`, filter: { ...all, status: `failed` } },
  { name: `running`, filter: { ...all, status: `running` } },
  { name: `mine`, filter: { ...all, status: `mine` } },
  { name: `one repository`, filter: { ...all, repositoryId: `repo-mobile` } },
].map((c) => ({ ...c, expected: ciPrGroups(SCENARIO, c.filter as CiRunFilter) }))
fixture.queueCases = [
  { name: `the scenario`, input: null as null | Partial<CiPageInput> },
  { name: `an hour later: the window drops nothing yet, the run is long overdue`, input: { now: `2026-11-27T11:30:00.000Z` } },
  { name: `three days later: only the live queue is left`, input: { now: `2026-11-30T10:30:00.000Z` } },
  { name: `nothing at all`, input: { trains: [], trainEvents: [], pullRequests: [] } },
].map((c) => ({ ...c, expected: ciQueue({ ...SCENARIO, ...(c.input ?? {}) }) }))
fixture.runContextCases = [`r91`, `r90`, `r87`, `r483`, `g1207`, `r479`, `g1206`].map((id) => ({
  run: id,
  expected: ciRunContext(SCENARIO.runs.find((r) => r.id === id)!, SCENARIO),
}))
fixture.typicalTrainMs = { "repo-engine": ciTypicalTrainMs(SCENARIO, `repo-engine`), "repo-mobile": ciTypicalTrainMs(SCENARIO, `repo-mobile`) }
fixture.etaCases = [
  { typicalMs: null, elapsedMs: 60_000 },
  { typicalMs: 420_000, elapsedMs: 100_000 },
  { typicalMs: 420_000, elapsedMs: 360_000 },
  { typicalMs: 420_000, elapsedMs: 400_000 },
  { typicalMs: 420_000, elapsedMs: 900_000 },
  { typicalMs: 0, elapsedMs: 10 },
].map((c) => ({ ...c, expected: ciTrainEta(c.typicalMs, c.elapsedMs) }))
fixture.runners = ciRunnerRows(SCENARIO)

// The badge, over small worlds of its own.
const mine = (id: string, state: string, at: string, extra: Partial<CiPageRun> = {}) =>
  run({ id, repositoryId: `repo-engine`, number: Number(id.replace(/\D/g, ``)) || 1, name: `gate`, state, createdByUserId: ME, startedAt: at, ...extra })
const badgeWorld = (runs: CiPageRun[], jobs: CiPageJob[] = []) => ({ runs, jobs, pullRequests: SCENARIO.pullRequests, sessions: SCENARIO.sessions, viewerId: ME })
const badgeCases = [
  { name: `the scenario: one failure of mine (#482 via my PR's chat), #57 cleared by #58`, input: badgeWorld(SCENARIO.runs, SCENARIO.jobs) },
  { name: `nothing of mine: none`, input: badgeWorld([run({ id: `x1`, repositoryId: `repo-engine`, number: 1, name: `gate`, state: `failure`, createdByUserId: `u-other`, startedAt: `2026-11-27T10:00:00.000Z` })]) },
  { name: `a failure, then a green re-run of the same pipeline and ref: cleared`, input: badgeWorld([mine(`r1`, `failure`, `2026-11-27T09:00:00.000Z`), mine(`r2`, `success`, `2026-11-27T10:00:00.000Z`)]) },
  {
    name: `a failure whose re-run is running: muted 1`,
    input: badgeWorld([mine(`r1`, `failure`, `2026-11-27T09:00:00.000Z`), mine(`r2`, `pending`, `2026-11-27T10:00:00.000Z`)], [job(`r2`, `in_progress`, null)]),
  },
  {
    name: `two failures on two refs count two, a running one elsewhere stays out`,
    input: badgeWorld(
      [
        mine(`r1`, `failure`, `2026-11-27T09:00:00.000Z`, { ref: `a` }),
        mine(`r2`, `failure`, `2026-11-27T09:10:00.000Z`, { ref: `b` }),
        mine(`r3`, `pending`, `2026-11-27T09:20:00.000Z`, { ref: `c` }),
      ],
      [job(`r3`, `queued`, null)]
    ),
  },
  { name: `a cancelled run is no failure`, input: badgeWorld([mine(`r1`, `failure`, `2026-11-27T09:00:00.000Z`)], [job(`r1`, `completed`, `cancelled`)]) },
]
fixture.badgeCases = badgeCases.map((c) => ({ ...c, expected: ciBadge(c.input) }))

writeFileSync(path, `${JSON.stringify(fixture, null, 2)}\n`)
console.log(`wrote ${path}`)
