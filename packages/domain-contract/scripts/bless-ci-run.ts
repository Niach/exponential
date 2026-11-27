// #EXP-2172: rewrite fixtures/ci-run.json's expected values from the TS
// reference (src/ci-run.ts), inputs kept; new case lists are built here.
// Review the diff: it is the spec change.
//   bun packages/domain-contract/scripts/bless-ci-run.ts
import { readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import { ciFailureSummary, ciJobRows, ciJobsCaption, ciJobSummary, ciRunActions, ciRunChips, ciRunStatus } from "../src/ci-run"

const path = join(import.meta.dir, `../fixtures/ci-run.json`)
const f = JSON.parse(readFileSync(path, `utf8`))
for (const c of f.runStatus) c.expected = ciRunStatus(c.state, c.jobs)
for (const c of f.jobRows) c.expected = ciJobRows(c.jobs, c.now)
for (const c of f.actions) c.expected = ciRunActions(c)

const s = f.specimen
const jobs = s.jobs as Array<Record<string, unknown> & { runnerLabel: string | null }>
f.jobsCaption = [
  { name: `the specimen`, jobs: jobs.map((j) => ({ status: j.status, conclusion: j.conclusion })) },
  { name: `running and queued`, jobs: [{ status: `in_progress`, conclusion: null }, { status: `queued`, conclusion: null }, { status: `completed`, conclusion: `success` }] },
  { name: `cancelled and skipped count as skipped`, jobs: [{ status: `completed`, conclusion: `cancelled` }, { status: `completed`, conclusion: `skipped` }] },
].map((c) => ({ ...c, expected: ciJobsCaption(c.jobs) }))
const job = (over: Record<string, unknown>) => ({ id: `j`, check: `test (linux)`, position: 0, reason: null, runnerLabel: `mint`, startedAt: `2026-11-13T10:00:00.000Z`, completedAt: `2026-11-13T10:08:25.000Z`, status: `completed`, conclusion: `failure`, ...over })
f.jobSummary = [
  { name: `failed on a runner`, job: job({}) },
  { name: `passed, no runner (GitHub)`, job: job({ conclusion: `success`, runnerLabel: null }) },
  { name: `running`, job: job({ status: `in_progress`, conclusion: null, completedAt: null }) },
  { name: `queued`, job: job({ status: `queued`, conclusion: null, startedAt: null, completedAt: null }) },
  { name: `skipped with a reason`, job: job({ conclusion: `skipped`, reason: `needs`, startedAt: null, completedAt: null }) },
].map((c) => ({ ...c, now: `2026-11-13T10:10:00.000Z`, expected: ciJobSummary(c.job as never, `2026-11-13T10:10:00.000Z`) }))
f.chips = [
  { name: `the specimen with its PR and author`, run: s.run, jobs, pullRequest: { number: s.pullRequestNumber, base: `main` }, author: `Mara Lind`, now: s.now },
  { name: `a GitHub run: no PR, no author, no runner`, run: { ...s.run, ref: `main` }, jobs: jobs.map((j) => ({ ...j, runnerLabel: null })), pullRequest: null, author: null, now: s.now },
  { name: `queued: no duration yet`, run: { ...s.run, state: `pending`, startedAt: null, completedAt: null }, jobs: jobs.map((j) => ({ ...j, status: `queued`, conclusion: null, runnerLabel: null })), pullRequest: null, author: null, now: s.now },
].map((c) => ({ ...c, expected: ciRunChips(c as never) }))
f.failureSummary = [
  { name: `the specimen's failed job and its log`, job: { check: `test (linux)` }, log: s.log },
  { name: `no log read yet`, job: { check: `test (linux)` }, log: null },
  { name: `a log without an error line`, job: { check: `build` }, log: `##[group]Build\nok\n##[endgroup]\n` },
].map((c) => ({ ...c, expected: ciFailureSummary(c.job, c.log) }))
writeFileSync(path, `${JSON.stringify(f, null, 2)}\n`)
console.log(`blessed ci-run.json`)
