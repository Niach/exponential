// @vitest-environment node
import { afterAll, describe, expect, it } from "vitest"
import { asc, eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { ciJobs, ciRuns, devices, repositories, teamMembers, teams, users } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// #EXP-2172: GitHub Actions runs land in ci_runs / ci_jobs (provider
// github) from the workflow_run / workflow_job webhooks, and our scheduler
// never touches them.

const ready = await probeRealDb(`SELECT to_regclass('public.ci_jobs') IS NOT NULL AS ok`)
if (!ready.ok) console.info(`[github-runs.real-db] skipped: ${ready.reason}`)

const gh = await import(`@/lib/ci/github-runs`)
const runs = await import(`@/lib/ci/runs`)

const madeTeams: string[] = []
const madeUsers: string[] = []
afterAll(async () => {
  if (!ready.ok) return
  for (const id of madeTeams) await db.delete(teams).where(eq(teams.id, id))
  for (const id of madeUsers) await db.delete(users).where(eq(users.id, id))
})

async function fixture() {
  const slug = `gh-${crypto.randomUUID().slice(0, 8)}`
  const [team] = await db.insert(teams).values({ name: slug, slug }).returning({ id: teams.id })
  madeTeams.push(team!.id)
  const userId = `u-${crypto.randomUUID()}`
  await db.insert(users).values({ id: userId, name: slug, email: `${slug}@example.com` })
  madeUsers.push(userId)
  await db.insert(teamMembers).values({ teamId: team!.id, userId, role: `owner` })
  const fullName = `org/${slug}`
  const [repo] = await db.insert(repositories).values({ teamId: team!.id, fullName, ciConfig: { enabled: true } }).returning({ id: repositories.id })
  // A runner that would take any queued job of ours.
  await db.insert(devices).values({
    userId,
    deviceId: `dev-${slug}`,
    label: `mint`,
    kind: `server`,
    platform: `linux`,
    caps: [runs.CI_RUNNER_CAP],
    runner: { enabled: true, labels: [], slots: 4, timeoutMin: 60 },
    lastSeenAt: new Date(),
  })
  return { teamId: team!.id, repositoryId: repo!.id, fullName }
}

const RUN = {
  id: 9_000_001,
  name: `CI`,
  run_number: 1207,
  run_attempt: 1,
  event: `push`,
  status: `in_progress`,
  conclusion: null,
  head_branch: `main`,
  head_sha: `f`.repeat(40),
  html_url: `https://github.com/org/x/actions/runs/9000001`,
  display_title: `Bump the widget to 2.4.1\n\nlonger body`,
  run_started_at: `2026-11-27T10:00:00Z`,
  updated_at: `2026-11-27T10:00:00Z`,
}

describe.skipIf(!ready.ok)(`GitHub Actions ingest`, { timeout: 30_000 }, () => {
  it(`writes a run and its jobs, never moves back, and is never dispatched`, async () => {
    const f = await fixture()
    const [runId] = await gh.ingestWorkflowRun({ fullName: f.fullName, installationId: null, run: RUN })
    const [run] = await db.select().from(ciRuns).where(eq(ciRuns.id, runId!))
    expect(run).toMatchObject({ provider: `github`, externalId: `9000001`, number: 1207, name: `CI`, ref: `main`, trigger: `push`, state: `pending`, externalAttempt: 1, title: `Bump the widget to 2.4.1` })

    const job = {
      id: 77,
      run_id: RUN.id,
      run_attempt: 1,
      name: `test`,
      status: `queued`,
      html_url: `https://github.com/org/x/actions/runs/9000001/job/77`,
      created_at: `2026-11-27T10:00:01Z`,
      labels: [`ubuntu-latest`],
    }
    await gh.ingestWorkflowJob({ fullName: f.fullName, installationId: null, job })
    // A queued GitHub job is GitHub's: our dispatcher and clock leave it.
    expect(await runs.dispatchQueued({ teamId: f.teamId })).toBe(0)
    await runs.ciTick()
    await gh.ingestWorkflowJob({
      fullName: f.fullName,
      installationId: null,
      job: { ...job, status: `completed`, conclusion: `failure`, started_at: `2026-11-27T10:00:02Z`, completed_at: `2026-11-27T10:01:00Z`, steps: [{ name: `bun test`, status: `completed`, conclusion: `failure` }] },
    })
    // A late in_progress delivery never moves the job back.
    await gh.ingestWorkflowJob({ fullName: f.fullName, installationId: null, job: { ...job, status: `in_progress` } })
    let rows = await db.select().from(ciJobs).where(eq(ciJobs.runId, runId!)).orderBy(asc(ciJobs.position))
    expect(rows).toHaveLength(1)
    expect(rows[0]).toMatchObject({ provider: `github`, externalId: `77`, status: `completed`, conclusion: `failure`, runsOn: [`ubuntu-latest`], dispatchedAt: null })
    expect(rows[0]!.steps[0]).toMatchObject({ name: `bun test`, status: `completed`, conclusion: `failure` })

    await gh.ingestWorkflowRun({ fullName: f.fullName, installationId: null, run: { ...RUN, status: `completed`, conclusion: `failure`, updated_at: `2026-11-27T10:01:01Z` } })
    // A late in_progress of the same attempt never reopens it.
    await gh.ingestWorkflowRun({ fullName: f.fullName, installationId: null, run: RUN })
    expect((await db.select().from(ciRuns).where(eq(ciRuns.id, runId!)))[0]).toMatchObject({ state: `failure` })

    // A re-run attempt reopens the same row and replaces the job by name.
    await gh.ingestWorkflowRun({ fullName: f.fullName, installationId: null, run: { ...RUN, run_attempt: 2, status: `queued` } })
    await gh.ingestWorkflowJob({ fullName: f.fullName, installationId: null, job: { ...job, id: 78, run_attempt: 2, status: `in_progress` } })
    expect((await db.select().from(ciRuns).where(eq(ciRuns.id, runId!)))[0]).toMatchObject({ state: `pending`, externalAttempt: 2, completedAt: null })
    rows = await db.select().from(ciJobs).where(eq(ciJobs.runId, runId!))
    expect(rows.map((row) => [row.externalId, row.status])).toEqual([[`78`, `in_progress`]])
    // An attempt-1 job arriving late changes nothing.
    await gh.ingestWorkflowJob({ fullName: f.fullName, installationId: null, job: { ...job, id: 79, name: `lint`, run_attempt: 1, status: `completed`, conclusion: `success` } })
    expect(await db.select().from(ciJobs).where(eq(ciJobs.runId, runId!))).toHaveLength(1)
  })

  it(`keeps our run numbers our own beside GitHub's`, async () => {
    const f = await fixture()
    await gh.ingestWorkflowRun({ fullName: f.fullName, installationId: null, run: { ...RUN, id: 9_000_002, run_number: 500 } })
    // GitHub numbers per workflow: two workflows may share a number.
    await gh.ingestWorkflowRun({ fullName: f.fullName, installationId: null, run: { ...RUN, id: 9_000_003, name: `Deploy`, run_number: 500 } })
    const result = await runs.startRun({
      repositoryId: f.repositoryId,
      sha: `e`.repeat(40),
      trigger: `push`,
      ref: `main`,
      file: { version: 1, name: `ci`, on: { push: { branches: [`main`] } }, jobs: { build: { runsOn: [`linux`], steps: [{ name: `b`, run: `true` }] } } },
    })
    expect(result.started).toBe(true)
    const [own] = await db.select().from(ciRuns).where(eq(ciRuns.id, (result as { runId: string }).runId))
    expect(own).toMatchObject({ provider: `exponential`, number: 1 })
  })

  it(`a job delivered first creates its run`, async () => {
    const f = await fixture()
    await gh.ingestWorkflowJob({
      fullName: f.fullName,
      installationId: null,
      job: { id: 90, run_id: 9_000_010, run_attempt: 1, name: `build`, workflow_name: `CI`, head_branch: `main`, head_sha: `d`.repeat(40), status: `queued` },
    })
    const [run] = await db.select().from(ciRuns).where(eq(ciRuns.externalId, `9000010`))
    expect(run).toMatchObject({ provider: `github`, name: `CI`, repositoryId: f.repositoryId })
    expect(await db.select().from(ciJobs).where(eq(ciJobs.runId, run!.id))).toHaveLength(1)
  })
})
