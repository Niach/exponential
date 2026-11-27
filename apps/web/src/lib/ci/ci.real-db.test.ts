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
  events,
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
      listFiles: async () => null,
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
    .values({ teamId: team!.id, fullName: `org/${slug}`, ciConfig: { enabled: true } })
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
  it(`plans nothing on a GitHub repo that did not opt in, nor without a runner device`, async () => {
    const f = await fixture()
    const plan = (sha: string) => runs.startRun({ repositoryId: f.repositoryId, sha, trigger: `push`, ref: `main`, file: PIPELINE })
    const runsOf = () => db.select({ id: ciRuns.id }).from(ciRuns).where(eq(ciRuns.repositoryId, f.repositoryId))
    // A GitHub repo is opt-in: unset = off.
    await db.update(repositories).set({ ciConfig: {} }).where(eq(repositories.id, f.repositoryId))
    expect(await plan(`b`.repeat(40))).toEqual({ started: false, reason: `disabled` })
    // An ecode repo is on unless switched off.
    await db.update(repositories).set({ provider: `ecode` }).where(eq(repositories.id, f.repositoryId))
    expect(runs.ciEnabled({ provider: `ecode`, ciConfig: {} })).toBe(true)
    expect(runs.ciEnabled({ provider: `ecode`, ciConfig: { enabled: false } })).toBe(false)
    // On, but no member's device runs CI: no queued rows nobody can take.
    await db.update(devices).set({ runner: { enabled: false, labels: [], slots: 1, timeoutMin: 60 } }).where(eq(devices.id, f.deviceRowId))
    expect(await plan(`c`.repeat(40))).toEqual({ started: false, reason: `no_runner` })
    expect(await runsOf()).toEqual([])
    await db.update(devices).set({ runner: { enabled: true, labels: [], slots: 1, timeoutMin: 60 } }).where(eq(devices.id, f.deviceRowId))
    expect((await plan(`c`.repeat(40))).started).toBe(true)
  })

  // #EXP-2170: every daemon advertises `ecode-jobs`; only a device whose
  // owner turned its runner on gets a repository's jobs (code + a job token).
  it(`repository jobs go only to a runner its owner turned on`, async () => {
    const { pickJobDevice } = await import(`@/lib/ecode/jobs`)
    const f = await fixture()
    await db.update(devices).set({ caps: [runs.CI_RUNNER_CAP, `ecode-jobs`], lastSeenAt: new Date(Date.now() - 30_000) }).where(eq(devices.id, f.deviceRowId))
    // Another member's daemon, seen just now, set up for something else.
    const otherId = `u-${crypto.randomUUID()}`
    await db.insert(users).values({ id: otherId, name: `other`, email: `other-${f.slug}@example.com` })
    madeUsers.push(otherId)
    await db.insert(teamMembers).values({ teamId: f.teamId, userId: otherId, role: `member` })
    const [other] = await db
      .insert(devices)
      .values({ userId: otherId, deviceId: `dev-other-${f.slug}`, label: `lab`, kind: `server`, platform: `linux`, caps: [runs.CI_RUNNER_CAP, `ecode-jobs`], lastSeenAt: new Date() })
      .returning({ id: devices.id })
    expect((await pickJobDevice(f.teamId))?.rowId).toBe(f.deviceRowId)
    // Its runner off too: no device at all, never the lab box.
    await db.update(devices).set({ runner: { enabled: false, labels: [], slots: 1, timeoutMin: 60 } }).where(eq(devices.id, f.deviceRowId))
    expect(await pickJobDevice(f.teamId)).toBeNull()
    // The lab box's owner turns its runner on: now it may run them.
    await db.update(devices).set({ runner: { enabled: true, labels: [], slots: 1, timeoutMin: 60 } }).where(eq(devices.id, other!.id))
    expect((await pickJobDevice(f.teamId))?.rowId).toBe(other!.id)
  })

  it(`a push to a PR cancels the older head's unfinished run`, async () => {
    const f = await fixture()
    const [pr] = await db
      .insert(pullRequests)
      .values({ teamId: f.teamId, repositoryId: f.repositoryId, number: 7, url: `https://github.com/org/${f.slug}/pull/7`, title: `t`, head: `feat`, base: `main` })
      .returning({ id: pullRequests.id })
    const plan = (sha: string) =>
      runs.startRun({ repositoryId: f.repositoryId, sha, trigger: `pull_request`, ref: `feat`, base: `main`, pullRequestId: pr!.id, fork: false, file: PIPELINE })
    const first = (await plan(`d`.repeat(40))) as { runId: string }
    const second = (await plan(`e`.repeat(40))) as { runId: string }
    const state = async (id: string) => (await db.select({ state: ciRuns.state }).from(ciRuns).where(eq(ciRuns.id, id)))[0]!.state
    // The old run is over (its jobs cancelled); the new one waits for a runner.
    expect(await state(first.runId)).not.toBe(`pending`)
    expect(new Set((await jobsOf(first.runId)).map((job) => job.conclusion))).toEqual(new Set([`cancelled`]))
    expect(await state(second.runId)).toBe(`pending`)
    // The old head's rows are gone, never left red; the new head's are the PR's.
    const prRows = await db.select({ sha: pullRequestChecks.headSha }).from(pullRequestChecks).where(eq(pullRequestChecks.pullRequestId, pr!.id))
    expect(prRows.length).toBeGreaterThan(0)
    expect(new Set(prRows.map((row) => row.sha))).toEqual(new Set([`e`.repeat(40)]))
    const oldCommitRows = await db
      .select({ id: commitChecks.id })
      .from(commitChecks)
      .where(and(eq(commitChecks.repositoryId, f.repositoryId), eq(commitChecks.sha, `d`.repeat(40))))
    expect(oldCommitRows).toEqual([])
  })

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
    // #EXP-2193: the runner's limits ride the payload (defaults for an older row).
    expect(commands[0].payload).toEqual({ jobId: build.id, memoryMaxMb: 8192, pauseWhenBusy: true })
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
    expect(await takeJobSecrets({ jobId: build.id, token: claimed!.token, repositoryId: f.repositoryId })).toEqual({
      values: { TOKEN: `s3cret` },
      withheld: [],
      notice: null,
    })
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

  it(`a killed runner's job is re-queued once, then fails (runner_lost); a hung one times out`, async () => {
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
    // #EXP-2193: the first loss hands it back: queued, no runner, a dead token.
    const [requeued] = await db.select().from(ciJobs).where(eq(ciJobs.id, build.id))
    expect([requeued.status, requeued.runnerDeviceId, requeued.jobTokenHash, requeued.logKey]).toEqual([`queued`, null, null, null])
    expect(requeued.requeuedAt).not.toBeNull()
    expect(await runs.beatJob({ jobId: build.id, token: claimed!.token })).toEqual({ cancel: true })
    // The runner comes back and claims it afresh (a new token); then it is lost again.
    await db.update(devices).set({ lastSeenAt: new Date() }).where(eq(devices.id, f.deviceRowId))
    const again = await runs.claimJob({ userId: f.userId, deviceId: f.deviceId, jobId: build.id })
    expect(again).not.toBeNull()
    expect(again!.token).not.toBe(claimed!.token)
    await db.update(devices).set({ lastSeenAt: past }).where(eq(devices.id, f.deviceRowId))
    await db.update(ciJobs).set({ lastBeatAt: past }).where(eq(ciJobs.id, build.id))
    await runs.ciTick(new Date())
    const [lost] = await db.select().from(ciJobs).where(eq(ciJobs.id, build.id))
    expect([lost.status, lost.conclusion, lost.reason]).toEqual([`completed`, `failure`, `runner_lost`])
    // Its dependent is skipped by the chain; the run is red.
    const [, test] = await jobsOf(runId)
    expect([test.conclusion, test.reason]).toEqual([`skipped`, `needs`])
    expect(await runs.beatJob({ jobId: build.id, token: again!.token })).toEqual({ cancel: true })

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

  it(`ci.release: a quitting runner hands its job back at once, the second time it fails (#EXP-2193)`, async () => {
    const f = await fixture()
    const started = await runs.startRun({ repositoryId: f.repositoryId, sha: `9`.repeat(40), trigger: `push`, ref: `main`, file: PIPELINE })
    const [build] = await jobsOf((started as { runId: string }).runId)
    const first = await runs.claimJob({ userId: f.userId, deviceId: f.deviceId, jobId: build.id })
    expect(await runs.releaseJob({ jobId: build.id, token: `expci_wrong-token-value` })).toEqual({ requeued: false })
    expect(await runs.releaseJob({ jobId: build.id, token: first!.token })).toEqual({ requeued: true })
    const [back] = await db.select().from(ciJobs).where(eq(ciJobs.id, build.id))
    expect([back.status, back.runnerDeviceId, back.conclusion]).toEqual([`queued`, null, null])
    // The old token is dead; the job is dispatched again right away.
    expect(await runs.releaseJob({ jobId: build.id, token: first!.token })).toEqual({ requeued: false })
    const dispatched = await db.select().from(deviceCommands).where(eq(deviceCommands.deviceRowId, f.deviceRowId))
    expect(dispatched.filter((c) => c.payload.jobId === build.id).length).toBeGreaterThan(0)
    const second = await runs.claimJob({ userId: f.userId, deviceId: f.deviceId, jobId: build.id })
    expect(second).not.toBeNull()
    expect(await runs.releaseJob({ jobId: build.id, token: second!.token })).toEqual({ requeued: false })
    const [failed] = await db.select().from(ciJobs).where(eq(ciJobs.id, build.id))
    expect([failed.status, failed.conclusion, failed.reason]).toEqual([`completed`, `failure`, `runner_lost`])
  })

  it(`secrets reach only default-branch, tag and merge-train runs unless the repo allows pull requests (#EXP-2180)`, async () => {
    const { ciRouter } = await import(`@/lib/trpc/ci`)
    const f = await fixture()
    await db.update(repositories).set({ defaultBranch: `main` }).where(eq(repositories.id, f.repositoryId))
    await setSecret({ teamId: f.teamId, repositoryId: f.repositoryId, name: `TOKEN`, value: `s3cret`, userId: f.userId })
    let n = 0
    const take = async (trigger: `push` | `pull_request` | `tag` | `merge_train` | `manual`, ref: string) => {
      const sha = `${++n}`.padStart(40, `f`)
      const file = { ...PIPELINE, on: { push: {}, pull_request: {}, tag: {}, merge_train: {}, manual: {} } }
      const started = await runs.startRun({ repositoryId: f.repositoryId, sha, trigger, ref, file })
      expect(started.started).toBe(true)
      const [build] = await jobsOf((started as { runId: string }).runId)
      const claimed = await runs.claimJob({ userId: f.userId, deviceId: f.deviceId, jobId: build.id })
      expect(claimed).not.toBeNull()
      const secrets = await takeJobSecrets({ jobId: build.id, token: claimed!.token, repositoryId: f.repositoryId })
      await runs.finishJob({ jobId: build.id, token: claimed!.token, conclusion: `success` })
      return secrets
    }
    const pr = await take(`pull_request`, `feat/x`)
    expect(pr?.values).toEqual({})
    expect(pr?.withheld).toEqual([`TOKEN`])
    expect(pr?.notice).toContain(`TOKEN`)
    expect(pr?.notice).not.toContain(`s3cret`)
    expect((await take(`push`, `feat/x`))?.values).toEqual({})
    expect((await take(`manual`, `feat/x`))?.values).toEqual({})
    expect((await take(`push`, `main`))?.values).toEqual({ TOKEN: `s3cret` })
    expect((await take(`manual`, `main`))?.values).toEqual({ TOKEN: `s3cret` })
    expect((await take(`tag`, `v1.0`))?.values).toEqual({ TOKEN: `s3cret` })
    expect((await take(`merge_train`, `exp-train/1`))?.values).toEqual({ TOKEN: `s3cret` })

    // A plain member cannot allow pull requests; the owner can (an audit event).
    const mateId = `u-${crypto.randomUUID()}`
    await db.insert(users).values({ id: mateId, name: `mate`, email: `mate-${f.slug}@example.com` })
    madeUsers.push(mateId)
    await db.insert(teamMembers).values({ teamId: f.teamId, userId: mateId, role: `member` })
    const caller = (userId: string) => ciRouter.createCaller({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) } as never)
    await expect(caller(mateId).setRepoConfig({ repositoryId: f.repositoryId, pullRequestSecrets: true })).rejects.toThrow(/owner/)
    expect((await take(`pull_request`, `feat/y`))?.values).toEqual({})
    await caller(f.userId).setRepoConfig({ repositoryId: f.repositoryId, pullRequestSecrets: true })
    expect((await take(`pull_request`, `feat/y`))?.values).toEqual({ TOKEN: `s3cret` })

    // Every secret change and the opt-in are in the team's stream, names only.
    await caller(f.userId).deleteSecret({ repositoryId: f.repositoryId, name: `TOKEN` })
    const audit = await db.select().from(events).where(eq(events.teamId, f.teamId)).orderBy(asc(events.createdAt))
    expect(audit.map((e) => [e.name, e.userId])).toEqual([
      [`ci.secret_set`, f.userId],
      [`ci.pull_request_secrets`, f.userId],
      [`ci.secret_deleted`, f.userId],
    ])
    expect(JSON.stringify(audit.map((e) => e.payload))).not.toContain(`s3cret`)
  })

  it(`runner settings: memory and pause are stored, a Mac desktop app cannot turn its runner on (#EXP-2193)`, async () => {
    const { ciRouter } = await import(`@/lib/trpc/ci`)
    const f = await fixture()
    const caller = ciRouter.createCaller({ session: { user: { id: f.userId } }, db, request: new Request(`http://x.test`) } as never)
    const base = { deviceId: f.deviceId, enabled: true, labels: [], slots: 1, timeoutMin: 60 }
    expect((await caller.setRunner({ ...base, memoryMaxMb: 16384, pauseWhenBusy: false })).runner).toMatchObject({ memoryMaxMb: 16384, pauseWhenBusy: false })
    // An older client omits them: kept.
    expect((await caller.setRunner(base)).runner).toMatchObject({ memoryMaxMb: 16384, pauseWhenBusy: false })
    await expect(caller.setRunner({ ...base, memoryMaxMb: 1500 })).rejects.toThrow()
    // A server daemon on a Mac keeps its runner; the desktop app there does not.
    await db.update(devices).set({ platform: `macos` }).where(eq(devices.id, f.deviceRowId))
    expect((await caller.setRunner(base)).runner.enabled).toBe(true)
    await db.update(devices).set({ kind: `desktop` }).where(eq(devices.id, f.deviceRowId))
    await expect(caller.setRunner(base)).rejects.toThrow()
    expect((await caller.setRunner({ ...base, enabled: false })).runner.enabled).toBe(false)
  })

  it(`a one-slot runner takes the job expected to finish first, across runs (#EXP-1862)`, async () => {
    const f = await fixture()
    const SLOW_FIRST = {
      version: 1,
      name: `ci`,
      on: { push: { branches: [`main`] } },
      jobs: {
        typecheck: { runsOn: [`linux`], steps: [{ name: `tsc`, run: `tsc` }] },
        contract: { runsOn: [`linux`], steps: [{ name: `replay`, run: `true` }] },
      },
    }
    const plan = async (sha: string) =>
      ((await runs.startRun({ repositoryId: f.repositoryId, sha, trigger: `push`, ref: `main`, file: SLOW_FIRST })) as { runId: string }).runId
    const dispatchedTo = async () =>
      (await db.select().from(deviceCommands).where(eq(deviceCommands.deviceRowId, f.deviceRowId))).map((c) => c.payload.jobId as string)
    const run = async (jobId: string, at: Date, minutes: number) => {
      const claimed = await runs.claimJob({ userId: f.userId, deviceId: f.deviceId, jobId, now: at })
      expect(claimed).not.toBeNull()
      await runs.finishJob({ jobId, token: claimed!.token, conclusion: `success`, now: new Date(at.getTime() + minutes * 60_000) })
    }

    // No history: equal timeouts, queue order.
    const [typecheck1, contract1] = await jobsOf(await plan(`1`.repeat(40)))
    expect(await dispatchedTo()).toEqual([typecheck1.id])
    const t0 = new Date()
    await run(typecheck1.id, t0, 30)
    expect(await dispatchedTo()).toContain(contract1.id)
    await run(contract1.id, t0, 0.1)

    // With history: the 6-second check goes before the 30-minute one.
    const [typecheck2, contract2] = await jobsOf(await plan(`2`.repeat(40)))
    expect(await dispatchedTo()).toContain(contract2.id)
    expect(await dispatchedTo()).not.toContain(typecheck2.id)
    // A newer run's cheap job beats the older run's typecheck.
    const [typecheck3, contract3] = await jobsOf(await plan(`3`.repeat(40)))
    await run(contract2.id, new Date(), 0.1)
    const after = await dispatchedTo()
    expect(after).toContain(contract3.id)
    expect(after).not.toContain(typecheck2.id)
    expect(after).not.toContain(typecheck3.id)
  })

  it(`a job no runner claims within queuedLimitMinutes ends timed_out (unclaimed) on its check row (#EXP-1862)`, async () => {
    const f = await fixture()
    const runId = ((await runs.startRun({ repositoryId: f.repositoryId, sha: `f`.repeat(40), trigger: `push`, ref: `main`, file: PIPELINE })) as { runId: string }).runId
    const mac = (await jobsOf(runId)).find((job) => job.check === `mac`)!
    const macCheck = async () =>
      (await db.select().from(commitChecks).where(and(eq(commitChecks.repositoryId, f.repositoryId), eq(commitChecks.name, `mac`))))[0]!
    expect((await macCheck()).status).toBe(`queued`)
    await runs.ciTick(new Date(Date.now() + 59 * 60_000))
    expect((await db.select().from(ciJobs).where(eq(ciJobs.id, mac.id)))[0]!.status).toBe(`queued`)
    await runs.ciTick(new Date(Date.now() + 61 * 60_000))
    const [done] = await db.select().from(ciJobs).where(eq(ciJobs.id, mac.id))
    expect([done.status, done.conclusion, done.reason]).toEqual([`completed`, `timed_out`, `unclaimed`])
    const check = await macCheck()
    expect([check.status, check.conclusion]).toEqual([`completed`, `timed_out`])
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
            asked = JSON.parse(String(job.payload.args)) as Record<string, unknown>
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
        partialForgeForTests({ kind: `github`, requiredChecks: async () => [], reportStatus: async () => {}, readFile: async () => null, listFiles: async () => null })
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
        partialForgeForTests({ kind: `github`, requiredChecks: async () => [], reportStatus: async () => {}, readFile: async () => null, listFiles: async () => null })
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
        partialForgeForTests({ kind: `github`, requiredChecks: async () => [], reportStatus: async () => {}, readFile: async () => null, listFiles: async () => null })
      )
    }
  })

  // Q6: runner devices also serve GitHub Actions for opted-in repos.
  it(`a GitHub repo with our CI on plans its workflows too, as "ecode <job>" checks (#EXP-2078)`, async () => {
    const f = await fixture()
    const files: Record<string, string> = {
      ".github/workflows/ci.yml": `name: ci\non: [push, pull_request]\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - uses: actions/setup-node@v4\n      - run: npm test\n  lint:\n    runs-on: ubuntu-latest\n    needs: build\n    steps:\n      - run: npm run lint\n`,
    }
    setForgeForTests(() =>
      partialForgeForTests({
        kind: `github`,
        requiredChecks: async () => [],
        reportStatus: async () => {},
        readFile: async (_repo, path) => files[path] ?? null,
        listFiles: async (_repo, dir) => (dir === `.github/workflows` ? [`ci.yml`] : null),
      })
    )
    try {
      const sha = `7`.repeat(40)
      const started = await runs.startRun({ repositoryId: f.repositoryId, sha, trigger: `push`, ref: `main` })
      expect(started.started).toBe(true)
      const [run] = await db.select().from(ciRuns).where(and(eq(ciRuns.repositoryId, f.repositoryId), eq(ciRuns.sha, sha)))
      expect([run.name, run.state]).toEqual([`ci`, `pending`])
      const jobs = await jobsOf(run.id)
      expect(jobs.map((j) => j.check)).toEqual([`ecode build`, `ecode lint`])
      expect(jobs[1].needs).toEqual([jobs[0].id])
      expect(jobs[0].spec?.steps.map((step) => step.name)).toEqual([`actions/setup-node`, `npm test`])
      const checks = await db
        .select({ name: commitChecks.name })
        .from(commitChecks)
        .where(and(eq(commitChecks.repositoryId, f.repositoryId), eq(commitChecks.sha, sha)))
      expect(checks.map((c) => c.name).sort()).toEqual([`ecode build`, `ecode lint`])
      // `.exponential/ci.json` still wins: the workflows are not read then.
      files[`.exponential/ci.json`] = JSON.stringify(PIPELINE)
      const other = `6`.repeat(40)
      expect((await runs.startRun({ repositoryId: f.repositoryId, sha: other, trigger: `push`, ref: `main` })).started).toBe(true)
      const [own] = await db.select().from(ciRuns).where(and(eq(ciRuns.repositoryId, f.repositoryId), eq(ciRuns.sha, other)))
      expect((await jobsOf(own.id)).map((j) => j.check)).not.toContain(`ecode build`)
    } finally {
      setForgeForTests(() =>
        partialForgeForTests({ kind: `github`, requiredChecks: async () => [], reportStatus: async () => {}, readFile: async () => null, listFiles: async () => null })
      )
    }
  })

  it(`an opted-in repo gets an Actions runner on every enabled runner device`, async () => {
    const f = await fixture()
    const ar = await import(`@/lib/ci/actions-runners`)
    await db
      .update(devices)
      .set({ caps: [runs.CI_RUNNER_CAP, ar.ACTIONS_RUNNER_CAP], label: `Danny's Mint`, runner: { enabled: true, labels: [`exp-linux`], slots: 2, timeoutMin: 60 } })
      .where(eq(devices.id, f.deviceRowId))
    // Not opted in yet: nothing to serve, and no registration token.
    expect(await ar.actionsRunnerTargets(f.userId, f.deviceId)).toEqual([])
    await db.update(repositories).set({ ciConfig: { enabled: true, actionsRunners: true } }).where(eq(repositories.id, f.repositoryId))
    const targets = await ar.actionsRunnerTargets(f.userId, f.deviceId)
    expect(targets).toEqual([
      {
        repositoryId: f.repositoryId,
        fullName: `org/${f.slug}`,
        url: `https://github.com/org/${f.slug}`,
        labels: [`self-hosted`, `linux`, `exp-linux`],
        name: `danny-s-mint-${f.slug}`,
      },
    ])
    const minted: string[] = []
    ar.setRunnerTokenSourceForTests({
      mint: async (repo, kind) => {
        minted.push(`${kind}:${repo.fullName}`)
        return { token: `T-${kind}`, expiresAt: null }
      },
    })
    try {
      expect(await ar.actionsRunnerToken({ userId: f.userId, deviceId: f.deviceId, repositoryId: f.repositoryId, kind: `registration` })).toEqual({
        token: `T-registration`,
        expiresAt: null,
      })
      // Switched off: no new registration, but the device may still deregister.
      await db.update(repositories).set({ ciConfig: { enabled: true } }).where(eq(repositories.id, f.repositoryId))
      await expect(
        ar.actionsRunnerToken({ userId: f.userId, deviceId: f.deviceId, repositoryId: f.repositoryId, kind: `registration` })
      ).rejects.toThrow(/does not serve GitHub Actions/)
      expect((await ar.actionsRunnerToken({ userId: f.userId, deviceId: f.deviceId, repositoryId: f.repositoryId, kind: `remove` })).token).toBe(`T-remove`)
      expect(minted).toEqual([`registration:org/${f.slug}`, `remove:org/${f.slug}`])
    } finally {
      ar.setRunnerTokenSourceForTests(null)
    }
  })
})

