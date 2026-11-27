// #EXP-2172: write fixtures/ci-page.json's cases from the TS reference
// (src/ci-page.ts). Run after a deliberate rule change and REVIEW the diff:
//   bun packages/domain-contract/scripts/bless-ci-page.ts
import { readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import {
  ciBadge,
  ciDayTitle,
  ciRepoOptions,
  ciRunGroups,
  ciRunnerChip,
  ciRunnerRows,
  ciTabCounts,
  ciTrainGroups,
  type CiPageInput,
  type CiPageJob,
  type CiPageRun,
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
  ...r,
})
const job = (runId: string, status: string, conclusion: string | null, extra: Partial<CiPageJob> = {}): CiPageJob => ({
  runId,
  status,
  conclusion,
  runsOn: [],
  runnerDeviceId: null,
  provider: `exponential`,
  ...extra,
})

/** The page's demo team: two repositories of ours, one GitHub one. */
export const SCENARIO: CiPageInput = {
  viewerId: ME,
  now: NOW,
  utcOffsetMinutes: 60,
  users: [
    { id: ME, name: `Mara Lind`, image: null },
    { id: `u-other`, name: `Jonas Berg`, image: null },
  ],
  repos: [
    { id: `repo-engine`, fullName: `acme/engine` },
    { id: `repo-mobile`, fullName: `acme/mobile` },
    { id: `repo-site`, fullName: `acme/site` },
  ],
  sessions: [
    { id: `s-mine`, userId: ME },
    { id: `s-other`, userId: `u-other` },
  ],
  pullRequests: [
    { id: `pr-211`, number: 211, title: `Fix bisect on a red batch`, repositoryId: `repo-engine`, rootSessionId: `s-other`, base: `main`, mergeState: `testing`, mergePosition: null, mergeTrainId: `train-91` },
    { id: `pr-212`, number: 212, title: `Runner labels`, repositoryId: `repo-engine`, rootSessionId: `s-other`, base: `main`, mergeState: `testing`, mergePosition: null, mergeTrainId: `train-91` },
    { id: `pr-213`, number: 213, title: `Log tail`, repositoryId: `repo-engine`, rootSessionId: `s-mine`, base: `main`, mergeState: `testing`, mergePosition: null, mergeTrainId: `train-91` },
    { id: `pr-214`, number: 214, title: `Reduce cold start below 400 ms`, repositoryId: `repo-engine`, rootSessionId: `s-mine`, base: `main`, mergeState: `checks`, mergePosition: null, mergeTrainId: null },
    { id: `pr-215`, number: 215, title: `Cache the planner`, repositoryId: `repo-engine`, rootSessionId: `s-other`, base: `main`, mergeState: `queued`, mergePosition: 1, mergeTrainId: null },
    { id: `pr-40`, number: 40, title: `Dark splash`, repositoryId: `repo-mobile`, rootSessionId: null, base: `main`, mergeState: `held`, mergePosition: null, mergeTrainId: null },
    { id: `pr-41`, number: 41, title: `Old`, repositoryId: `repo-mobile`, rootSessionId: null, base: `main`, mergeState: `merged`, mergePosition: null, mergeTrainId: null },
  ],
  runs: [
    run({ id: `r483`, title: `Repack once per push, not per ref`, repositoryId: `repo-engine`, number: 483, name: `gate`, state: `pending`, ref: `exp/APP-32`, trigger: `pull_request`, pullRequestId: `pr-214`, startedAt: `2026-11-27T10:26:48.000Z` }),
    run({ id: `r91`, repositoryId: `repo-engine`, number: 91, name: `Merge train`, state: `pending`, ref: `main`, trigger: `merge_train`, startedAt: `2026-11-27T10:28:20.000Z` }),
    run({ id: `g1207`, title: `Bump the widget to 2.4.1`, repositoryId: `repo-mobile`, number: 1207, name: `build-ios`, state: `pending`, ref: `release/2.4`, trigger: `tag`, provider: `github`, createdByUserId: ME, createdAt: `2026-11-27T10:29:00.000Z` }),
    run({ id: `r482`, title: `Mirror prunes deleted branches`, repositoryId: `repo-engine`, number: 482, name: `gate`, state: `failure`, ref: `exp/APP-33`, trigger: `pull_request`, pullRequestId: `pr-213`, startedAt: `2026-11-27T09:59:20.000Z`, completedAt: `2026-11-27T10:12:00.000Z` }),
    run({ id: `g1206`, title: `Offline queue survives a cold start`, repositoryId: `repo-mobile`, number: 1206, name: `test`, state: `success`, provider: `github`, startedAt: `2026-11-27T09:22:54.000Z`, completedAt: `2026-11-27T09:30:00.000Z` }),
    run({ id: `g57`, title: `Pricing page copy`, repositoryId: `repo-site`, number: 57, name: `deploy`, state: `failure`, provider: `github`, createdByUserId: ME, startedAt: `2026-11-26T08:27:57.000Z`, completedAt: `2026-11-26T08:30:00.000Z` }),
    run({ id: `g58`, title: `Pricing page copy, take two`, repositoryId: `repo-site`, number: 58, name: `deploy`, state: `success`, provider: `github`, createdByUserId: ME, startedAt: `2026-11-27T08:27:57.000Z`, completedAt: `2026-11-27T08:30:00.000Z` }),
    run({ id: `r481`, title: `Repack once per push, not per ref`, repositoryId: `repo-engine`, number: 481, name: `gate`, state: `failure`, ref: `exp/APP-32`, trigger: `pull_request`, pullRequestId: `pr-214`, startedAt: `2026-11-27T07:30:00.000Z`, completedAt: `2026-11-27T07:30:24.000Z` }),
    run({ id: `r480`, repositoryId: `repo-gone`, number: 480, name: `nightly`, state: `success`, ref: null, trigger: `schedule`, startedAt: `2026-11-27T01:30:00.000Z`, completedAt: `2026-11-27T01:44:12.000Z` }),
  ],
  jobs: [
    job(`r483`, `in_progress`, null, { runsOn: [`linux`], runnerDeviceId: `dev-mint` }),
    job(`r483`, `queued`, null, { runsOn: [`linux`] }),
    job(`r483`, `queued`, null, { runsOn: [`mac`] }),
    job(`r91`, `in_progress`, null, { runsOn: [`linux`], runnerDeviceId: `dev-mint` }),
    job(`g1207`, `queued`, null, { runsOn: [`macos-latest`], provider: `github` }),
    job(`r482`, `completed`, `success`),
    job(`r482`, `completed`, `failure`),
    job(`g1206`, `completed`, `success`, { provider: `github` }),
    job(`g57`, `completed`, `failure`, { provider: `github` }),
    job(`g58`, `completed`, `success`, { provider: `github` }),
    job(`r481`, `completed`, `cancelled`),
    job(`r480`, `completed`, `success`),
  ],
  runners: [
    { id: `dev-mint`, label: `mint`, platform: `linux`, labels: [`GPU`], slots: 2, lastSeenAt: `2026-11-27T10:29:40.000Z` },
    { id: `dev-mac`, label: `Alex's MacBook Pro`, platform: `macos`, labels: [], slots: 1, lastSeenAt: `2026-11-27T10:29:55.000Z` },
    { id: `dev-old`, label: `build-box`, platform: null, labels: [], slots: 4, lastSeenAt: `2026-11-26T22:00:00.000Z` },
  ],
}

const filters = [
  { name: `all`, filter: { status: `all`, repositoryId: null, source: null } },
  { name: `running = running and queued`, filter: { status: `running`, repositoryId: null, source: null } },
  { name: `failed`, filter: { status: `failed`, repositoryId: null, source: null } },
  { name: `mine: started by me or my PR's chat`, filter: { status: `mine`, repositoryId: null, source: null } },
  { name: `one repository`, filter: { status: `all`, repositoryId: `repo-mobile`, source: null } },
  { name: `GitHub Actions only`, filter: { status: `all`, repositoryId: null, source: `github` } },
  { name: `ours, failed`, filter: { status: `failed`, repositoryId: null, source: `exponential` } },
] as const

fixture.scenario = SCENARIO
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
fixture.trainCases = [
  { name: `the Runs tab: testing batches only`, queue: false, expected: ciTrainGroups(SCENARIO, false) },
  { name: `the Merge train tab: batches, the queue, the waiting`, queue: true, expected: ciTrainGroups(SCENARIO, true) },
]
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
