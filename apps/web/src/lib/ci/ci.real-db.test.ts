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
  repoRefs,
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

  it(`an ecode repo push plans through a device read_file job and dispatches the run`, async () => {
    // The real ecode forge (no GitHub): the pipeline is read by a device job.
    setForgeForTests(null)
    try {
      const f = await fixture()
      await db.update(repositories).set({ provider: `ecode` }).where(eq(repositories.id, f.repositoryId))
      // The device runs eCode's git jobs too.
      await db
        .update(devices)
        .set({ caps: [runs.CI_RUNNER_CAP, `ecode-jobs`] })
        .where(eq(devices.id, f.deviceRowId))
      const sha = `f`.repeat(40)
      await db.insert(repoRefs).values({ repositoryId: f.repositoryId, ref: `refs/heads/main`, sha })

      // The daemon's side of the read_file job: answer it from "the repo".
      let asked: Record<string, unknown> | null = null
      const device = (async () => {
        for (let i = 0; i < 100; i++) {
          const [job] = await db
            .select()
            .from(deviceCommands)
            .where(and(eq(deviceCommands.deviceRowId, f.deviceRowId), eq(deviceCommands.kind, `ecode_job`), eq(deviceCommands.status, `pending`)))
            .limit(1)
          if (job) {
            asked = JSON.parse(job.payload.args!) as Record<string, unknown>
            await db
              .update(deviceCommands)
              .set({ status: `done`, completedAt: new Date(), result: JSON.stringify({ ok: true, result: { text: JSON.stringify(PIPELINE) } }) })
              .where(eq(deviceCommands.id, job.id))
            return
          }
          await new Promise((resolve) => setTimeout(resolve, 100))
        }
      })()

      const { pgEcodeHost } = await import(`@/lib/ecode/ecode-host`)
      const { drainCiTriggers } = await import(`@/lib/ci/triggers`)
      const repo = { id: f.repositoryId, fullName: `org/${f.slug}`, provider: `ecode` as const }
      await pgEcodeHost.publish(repo, [{ kind: `pushed`, branch: `main`, sha }])
      await device
      await drainCiTriggers()

      expect(asked).toEqual({ op: `read_file`, sha, path: `.exponential/ci.json` })
      const [run] = await db.select().from(ciRuns).where(eq(ciRuns.repositoryId, f.repositoryId))
      expect([run.trigger, run.sha, run.ref, run.state]).toEqual([`push`, sha, `main`, `pending`])
      const [build] = await jobsOf(run.id)
      const dispatched = await db
        .select()
        .from(deviceCommands)
        .where(and(eq(deviceCommands.deviceRowId, f.deviceRowId), eq(deviceCommands.kind, runs.CI_JOB_COMMAND)))
      expect(dispatched.map((c) => c.payload.jobId)).toEqual([build.id])
    } finally {
      setForgeForTests(() =>
        partialForgeForTests({ kind: `github`, requiredChecks: async () => [], reportStatus: async () => {}, readFile: async () => null })
      )
    }
  })

  // #EXP-1807: a push whose pipeline cannot be read is a FAILED run with a
  // `pipeline` check (the session hears it), never a silent no-op.
  it(`a pipeline read that fails is a failed run with a pipeline check`, async () => {
    const f = await fixture()
    setForgeForTests(() =>
      partialForgeForTests({
        kind: `github`,
        requiredChecks: async () => [],
        reportStatus: async () => {},
        readFile: async () => {
          throw new Error(`No team device with eCode jobs is online to run read_file`)
        },
      })
    )
    try {
      const sha = `9`.repeat(40)
      const started = await runs.startRun({ repositoryId: f.repositoryId, sha, trigger: `push`, ref: `main` })
      expect(started.started).toBe(true)
      const [run] = await db.select().from(ciRuns).where(eq(ciRuns.id, (started as { runId: string }).runId))
      expect(run.state).toBe(`failure`)
      expect(run.error).toContain(`Could not read the pipeline: No team device`)
      const [check] = await db
        .select()
        .from(commitChecks)
        .where(and(eq(commitChecks.repositoryId, f.repositoryId), eq(commitChecks.sha, sha)))
      expect([check.name, check.conclusion]).toEqual([`pipeline`, `failure`])
    } finally {
      setForgeForTests(() =>
        partialForgeForTests({ kind: `github`, requiredChecks: async () => [], reportStatus: async () => {}, readFile: async () => null })
      )
    }
  })

  it(`an ecode repo without ci.json plans one run per GitHub workflow`, async () => {
    const f = await fixture()
    await db.update(repositories).set({ provider: `ecode` }).where(eq(repositories.id, f.repositoryId))
    const files: Record<string, string> = {
      ".github/workflows/ci.yml": `name: CI\non:\n  push:\n    branches: [main]\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - run: make\n`,
      ".github/workflows/nightly.yaml": `on:\n  schedule:\n    - cron: "0 0 * * *"\njobs:\n  x:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo ok\n`,
    }
    setForgeForTests(() =>
      partialForgeForTests({
        kind: `ecode`,
        requiredChecks: async () => [],
        reportStatus: async () => {},
        readFile: async (_repo, path) => files[path] ?? null,
        listFiles: async (_repo, dir) => (dir === `.github/workflows` ? [`ci.yml`, `nightly.yaml`, `README.md`] : null),
      })
    )
    try {
      const sha = `8`.repeat(40)
      const started = await runs.startRun({ repositoryId: f.repositoryId, sha, trigger: `push`, ref: `main` })
      expect(started.started).toBe(true)
      const planned = await db.select().from(ciRuns).where(and(eq(ciRuns.repositoryId, f.repositoryId), eq(ciRuns.sha, sha)))
      expect(planned.map((r) => [r.name, r.state]).sort()).toEqual([
        [`CI`, `pending`],
        [`nightly`, `failure`],
      ])
      const nightly = planned.find((r) => r.name === `nightly`)!
      expect(nightly.error).toBe(`.github/workflows/nightly.yaml/on/schedule: unsupported_event`)
      const ci = planned.find((r) => r.name === `CI`)!
      expect((await jobsOf(ci.id)).map((j) => [j.check, j.runsOn])).toEqual([[`build`, [`linux`]]])
      const checks = await db
        .select({ name: commitChecks.name, conclusion: commitChecks.conclusion })
        .from(commitChecks)
        .where(and(eq(commitChecks.repositoryId, f.repositoryId), eq(commitChecks.sha, sha)))
      expect(checks.map((c) => c.name).sort()).toEqual([`build`, `pipeline (nightly)`])
      // A second delivery of the same push plans nothing new.
      expect((await runs.startRun({ repositoryId: f.repositoryId, sha, trigger: `push`, ref: `main` })).started).toBe(false)
    } finally {
      setForgeForTests(() =>
        partialForgeForTests({ kind: `github`, requiredChecks: async () => [], reportStatus: async () => {}, readFile: async () => null })
      )
    }
  })
})

