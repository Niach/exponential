// @vitest-environment node
import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { and, asc, eq } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  ciJobs,
  ciRuns,
  commitChecks,
  deviceCommands,
  devices,
  pullRequestChecks,
  pullRequests,
  repositories,
  teamMembers,
  teams,
  users,
} from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { partialForgeForTests, setForgeForTests } from "@/lib/ecode/forge"

// #EXP-1617: eCode's own CI against the real migrated Postgres: planning a
// run from the pipeline file, dispatch as a `ci_job` command, the runner's
// claim/beat/finish with its one-time token, the spec's skip chain, the
// clock (timeouts, lost runners) and the check rows it all writes.

const ready = await probeRealDb(`SELECT to_regclass('public.ci_jobs') IS NOT NULL AS ok`)
if (!ready.ok) console.info(`[ci.real-db] skipped: ${ready.reason}`)

const runs = await import(`@/lib/ci/runs`)
const { takeJobSecrets, setSecret } = await import(`@/lib/ci/secrets`)

const madeTeams: string[] = []
const madeUsers: string[] = []
afterAll(async () => {
  setForgeForTests(null)
  if (!ready.ok) return
  for (const id of madeTeams) await db.delete(teams).where(eq(teams.id, id))
  for (const id of madeUsers) await db.delete(users).where(eq(users.id, id))
})

beforeAll(() => {
  process.env.BETTER_AUTH_SECRET ??= `ci-real-db-test-secret-0123456789abcdef`
  setForgeForTests(() =>
    partialForgeForTests({
      kind: `github`,
      requiredChecks: async () => [],
      reportStatus: async () => {},
      readFile: async () => null,
    })
  )
})

const PIPELINE = {
  version: 1,
  name: `ci`,
  on: { push: { branches: [`main`] }, pull_request: {} },
  secrets: [`TOKEN`],
  jobs: {
    build: { runsOn: [`linux`], steps: [{ name: `build`, run: `make` }] },
    test: { runsOn: [`linux`], needs: [`build`], timeoutMin: 5, steps: [{ name: `test`, run: `make test` }] },
    mac: { runsOn: [`mac`], steps: [{ name: `mac`, run: `true` }] },
  },
}

async function fixture() {
  const slug = `ci-${crypto.randomUUID().slice(0, 8)}`
  const [team] = await db.insert(teams).values({ name: slug, slug }).returning({ id: teams.id })
  madeTeams.push(team!.id)
  const userId = `u-${crypto.randomUUID()}`
  await db.insert(users).values({ id: userId, name: slug, email: `${slug}@example.com` })
  madeUsers.push(userId)
  await db.insert(teamMembers).values({ teamId: team!.id, userId, role: `owner` })
  const [repo] = await db
    .insert(repositories)
    .values({ teamId: team!.id, fullName: `org/${slug}` })
    .returning({ id: repositories.id })
  const deviceId = `dev-${slug}`
  const [device] = await db
    .insert(devices)
    .values({
      userId,
      deviceId,
      label: `mint`,
      kind: `server`,
      platform: `linux`,
      caps: [runs.CI_RUNNER_CAP],
      runner: { enabled: true, labels: [], slots: 1, timeoutMin: 60 },
    })
    .returning({ id: devices.id })
  return { teamId: team!.id, userId, repositoryId: repo!.id, deviceId, deviceRowId: device!.id, slug }
}

async function jobsOf(runId: string) {
  return db.select().from(ciJobs).where(eq(ciJobs.runId, runId)).orderBy(asc(ciJobs.position))
}

describe.skipIf(!ready.ok)(`eCode CI`, { timeout: 30_000 }, () => {
  it(`plans, dispatches, claims, runs the chain and writes check rows`, async () => {
    const f = await fixture()
    const started = await runs.startRun({ repositoryId: f.repositoryId, sha: `a`.repeat(40), trigger: `push`, ref: `main`, file: PIPELINE })
    expect(started.started).toBe(true)
    const runId = (started as { runId: string }).runId
    const [build, test, mac] = await jobsOf(runId)
    expect([build.check, test.check, mac.check]).toEqual([`build`, `test`, `mac`])
    expect(test.needs).toEqual([build.id])

    // Only `build` is ready and labelled for the linux runner (one slot).
    const commands = await db.select().from(deviceCommands).where(eq(deviceCommands.deviceRowId, f.deviceRowId))
    expect(commands.map((c) => [c.kind, c.payload.jobId])).toEqual([[runs.CI_JOB_COMMAND, build.id]])
    expect(JSON.stringify(commands[0].payload)).not.toContain(`TOKEN`)

    // A job not dispatched-and-ready cannot be claimed.
    expect(await runs.claimJob({ userId: f.userId, deviceId: f.deviceId, jobId: test.id })).toBeNull()
    const claimed = await runs.claimJob({ userId: f.userId, deviceId: f.deviceId, jobId: build.id })
    expect(claimed?.job.sha).toBe(`a`.repeat(40))
    expect(claimed?.job.secrets).toEqual([`TOKEN`])
    // CAS: a second claim loses.
    expect(await runs.claimJob({ userId: f.userId, deviceId: f.deviceId, jobId: build.id })).toBeNull()

    // Secrets: once, with the token, never again.
    await setSecret({ teamId: f.teamId, repositoryId: f.repositoryId, name: `TOKEN`, value: `s3cret`, userId: f.userId })
    expect(await takeJobSecrets({ jobId: build.id, token: `expci_wrong-token-value`, repositoryId: f.repositoryId })).toBeNull()
    expect(await takeJobSecrets({ jobId: build.id, token: claimed!.token, repositoryId: f.repositoryId })).toEqual({ TOKEN: `s3cret` })
    expect(await takeJobSecrets({ jobId: build.id, token: claimed!.token, repositoryId: f.repositoryId })).toBeNull()

    expect(await runs.beatJob({ jobId: build.id, token: claimed!.token })).toEqual({ cancel: false })
    let [check] = await db
      .select()
      .from(commitChecks)
      .where(and(eq(commitChecks.repositoryId, f.repositoryId), eq(commitChecks.name, `build`)))
    expect(check.status).toBe(`in_progress`)
    expect(check.detailsUrl).toContain(`/t/${f.slug}/ci/${runId}`)

    expect(await runs.finishJob({ jobId: build.id, token: claimed!.token, conclusion: `success` })).toEqual({ ok: true })
    ;[check] = await db
      .select()
      .from(commitChecks)
      .where(and(eq(commitChecks.repositoryId, f.repositoryId), eq(commitChecks.name, `build`)))
    expect([check.status, check.conclusion]).toEqual([`completed`, `success`])
    // The finish freed the slot: `test` is dispatched next.
    const next = await db.select().from(deviceCommands).where(eq(deviceCommands.deviceRowId, f.deviceRowId))
    expect(next.map((c) => c.payload.jobId)).toContain(test.id)

    // A failed `test` fails the run; `mac` (no runner) stays queued → pending.
    const claimedTest = await runs.claimJob({ userId: f.userId, deviceId: f.deviceId, jobId: test.id })
    await runs.finishJob({ jobId: test.id, token: claimedTest!.token, conclusion: `failure` })
    const [run] = await db.select().from(ciRuns).where(eq(ciRuns.id, runId))
    expect(run.state).toBe(`failure`)

    // Cancel completes the rest.
    await runs.cancelRun(runId)
    const after = await jobsOf(runId)
    expect(after.map((j) => [j.check, j.conclusion])).toEqual([
      [`build`, `success`],
      [`test`, `failure`],
      [`mac`, `cancelled`],
    ])
  })

  it(`a killed runner fails its job (runner_lost) and a hung one times out`, async () => {
    const f = await fixture()
    const started = await runs.startRun({ repositoryId: f.repositoryId, sha: `b`.repeat(40), trigger: `push`, ref: `main`, file: PIPELINE })
    const runId = (started as { runId: string }).runId
    const [build] = await jobsOf(runId)
    const claimed = await runs.claimJob({ userId: f.userId, deviceId: f.deviceId, jobId: build.id })
    expect(claimed).not.toBeNull()
    // The runner goes silent: no device heartbeat, no job beat.
    const past = new Date(Date.now() - 10 * 60_000)
    await db.update(devices).set({ lastSeenAt: past }).where(eq(devices.id, f.deviceRowId))
    await db.update(ciJobs).set({ lastBeatAt: past }).where(eq(ciJobs.id, build.id))
    await runs.ciTick(new Date())
    const [lost] = await db.select().from(ciJobs).where(eq(ciJobs.id, build.id))
    expect([lost.status, lost.conclusion, lost.reason]).toEqual([`completed`, `failure`, `runner_lost`])
    // Its dependent is skipped by the chain; the run is red.
    const [, test] = await jobsOf(runId)
    expect([test.conclusion, test.reason]).toEqual([`skipped`, `needs`])
    expect(await runs.beatJob({ jobId: build.id, token: claimed!.token })).toEqual({ cancel: true })

    // A second run: a job past its timeout is killed as timed_out.
    await db.update(devices).set({ lastSeenAt: new Date() }).where(eq(devices.id, f.deviceRowId))
    const second = await runs.startRun({ repositoryId: f.repositoryId, sha: `c`.repeat(40), trigger: `push`, ref: `main`, file: PIPELINE })
    const [build2] = await jobsOf((second as { runId: string }).runId)
    await runs.claimJob({ userId: f.userId, deviceId: f.deviceId, jobId: build2.id })
    // The clock checks timeouts before lost runners: timed_out wins.
    await runs.ciTick(new Date(Date.now() + 61 * 60_000))
    const [timedOut] = await db.select().from(ciJobs).where(eq(ciJobs.id, build2.id))
    expect([timedOut.conclusion, timedOut.reason]).toEqual([`timed_out`, null])
  })

  it(`a fork PR never runs, and its checks land on the PR`, async () => {
    const f = await fixture()
    const [pr] = await db
      .insert(pullRequests)
      .values({ teamId: f.teamId, repositoryId: f.repositoryId, number: 7, url: `https://x/7`, head: `fork:feat`, base: `main` })
      .returning({ id: pullRequests.id })
    const started = await runs.startRun({
      repositoryId: f.repositoryId,
      sha: `d`.repeat(40),
      trigger: `pull_request`,
      base: `main`,
      pullRequestId: pr!.id,
      fork: true,
      file: PIPELINE,
    })
    const runId = (started as { runId: string }).runId
    const jobs = await jobsOf(runId)
    expect(jobs.every((j) => j.status === `completed` && j.reason === `fork`)).toBe(true)
    expect(await db.select().from(deviceCommands).where(eq(deviceCommands.deviceRowId, f.deviceRowId))).toEqual([])
    const checks = await db.select().from(pullRequestChecks).where(eq(pullRequestChecks.pullRequestId, pr!.id))
    expect(checks.map((c) => c.name).sort()).toEqual([`build`, `mac`, `test`])
  })

  it(`an invalid pipeline file is a failed run with a pipeline check`, async () => {
    const f = await fixture()
    const started = await runs.startRun({
      repositoryId: f.repositoryId,
      sha: `e`.repeat(40),
      trigger: `push`,
      ref: `main`,
      file: { version: 1, on: { push: {} }, jobs: { Bad: { steps: [] } } },
    })
    const runId = (started as { runId: string }).runId
    const [run] = await db.select().from(ciRuns).where(eq(ciRuns.id, runId))
    expect(run.state).toBe(`failure`)
    expect(run.error).toContain(`/jobs/Bad`)
    const [check] = await db
      .select()
      .from(commitChecks)
      .where(and(eq(commitChecks.repositoryId, f.repositoryId), eq(commitChecks.sha, `e`.repeat(40))))
    expect([check.name, check.conclusion]).toEqual([`pipeline`, `failure`])
  })
})
