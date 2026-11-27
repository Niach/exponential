// @vitest-environment node
import { afterAll, beforeEach, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { boards, devices, mergeTrains, pullRequestChecks, pullRequests, repositories, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb, retryingConflict } from "@/test/real-db"

// #EXP-2170 (Phase 0 dogfood): the merge train over a HOSTED repository
// (provider `ecode`) with the REAL eCode forge and host against the migrated
// Postgres; only the device job layer is faked. On prod the first hosted PR
// (feedback/dogfood #1) sat at "Queued · 1st" with no merge_trains row ever.

const jobs = vi.hoisted(() => ({ calls: [] as Array<{ op: string }>, ciJson: `` }))
vi.mock(`@/lib/ecode/jobs`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/ecode/jobs")>()),
  runJob: async (_repo: unknown, job: { op: string }) => {
    jobs.calls.push(job)
    if (job.op === `read_file`) return { ok: true, result: { text: jobs.ciJson } }
    if (job.op === `is_ancestor`) return { ok: true, result: { yes: false } }
    // compose: the device never answers in this test.
    return { ok: false, reason: `runner`, message: `no device in the test` }
  },
  queueMaintenance: async () => null,
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
}))
vi.mock(`@/lib/pull-requests`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/pull-requests")>()),
  syncPullRequestBodies: async () => {},
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: false }),
}))
vi.mock(`@/lib/integrations/github-client`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/integrations/github-client")>()),
  githubFetch: async () => new Response(`{}`, { status: 404 }),
}))

const ready = await probeRealDb(`
  SELECT (EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
    AND to_regclass('public.repo_refs') IS NOT NULL AND to_regclass('public.merge_trains') IS NOT NULL) AS ok
`)
if (!ready.ok) console.info(`[merge-train worker-ecode.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { recordPullRequestOpened } = await import(`@/lib/integrations/pr-sync`)
const { compareAndSwapRefs } = await import(`@/lib/ecode/git-store`)
const worker = await import(`@/lib/merge-train/worker`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }
afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await retryingConflict(() => db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`))
  if (made.users.length) await retryingConflict(() => db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`))
})
beforeEach(() => {
  jobs.calls.length = 0
  jobs.ciJson = JSON.stringify({ version: 1, on: { pull_request: { branches: [`main`] }, merge_train: {} }, jobs: { test: { name: `test`, runsOn: [`linux`], steps: [{ name: `Unit`, run: `true` }] } }, required: [`test`] })
  worker.resetMergeTrainForTests()
})

const MAIN = `38a7a3cc6f355b432c849d83a60cde1554aef526`
const HEAD = `f1df2ddfe3955ebcfd2cbdc49ed3a6591c48fa42`

async function hostedWorld() {
  const owner = `mte-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${owner}, 'Tester', ${`${owner}@mt.test`}, true, false, now(), now())
  `)
  made.users.push(owner)
  const slug = `mte-${id8()}`
  const [team] = await db.insert(teams).values({ name: `MTE`, slug }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const fullName = `${slug}/dogfood`
  const [repo] = await db
    .insert(repositories)
    .values({ teamId, provider: `ecode`, fullName, remoteUrl: `exp://${fullName}`, defaultBranch: `main`, private: true, providerConfig: {}, ciConfig: { enabled: true } })
    .returning({ id: repositories.id })
  await db.insert(boards).values({ teamId, name: `Dog`, slug: `dog-${id8()}`, prefix: `DOG`, repositoryId: repo!.id })
  expect(
    await compareAndSwapRefs(repo!.id, [
      { ref: `refs/heads/main`, old: null, new: MAIN },
      { ref: `refs/heads/exp/DOG-1`, old: null, new: HEAD },
    ])
  ).toBe(true)
  const chat = (await sendSession({ userId: owner, input: { teamId, deviceId, text: `work` } })).sessionId
  const { pullRequest } = await recordPullRequestOpened({
    teamId,
    rootSessionId: chat,
    sessionId: chat,
    repositoryId: repo!.id,
    repoFullName: fullName,
    number: 1,
    url: `https://exp.example/t/${slug}/repos/dogfood/pulls/1`,
    title: `DOG-1: sum.sh rejects non-integer arguments`,
    head: `exp/DOG-1`,
    base: `main`,
    issueIds: [],
    actorUserId: owner,
  })
  await db.update(pullRequests).set({ headSha: HEAD }).where(eq(pullRequests.id, pullRequest.id))
  // Our CI's green `test` at the head (what the run writes for a hosted PR).
  await db.insert(pullRequestChecks).values({ teamId, pullRequestId: pullRequest.id, headSha: HEAD, name: `test`, status: `completed`, conclusion: `success`, required: false })
  await db.update(pullRequests).set({ checksState: `success` }).where(eq(pullRequests.id, pullRequest.id))
  await db.update(sessions).set({ status: `idle`, busy: false }).where(eq(sessions.rootId, chat))
  return { repoId: repo!.id, prId: pullRequest.id }
}

describe.skipIf(!ready.ok)(`#EXP-2170: the merge train on a hosted repository`, () => {
  it(`a green, queued PR on an ecode repo starts a train run on eCode's forge`, async () => {
    const w = await hostedWorld()
    await worker.runRepository(w.repoId)
    await worker.drainMergeTrainEdges()
    const runs = await db.select().from(mergeTrains).where(eq(mergeTrains.repositoryId, w.repoId))
    // Before the fix the worker handed GitHub's forge a hosted repo: every
    // PR read 404'd, startRun re-queued silently and no run was ever made.
    expect(runs).toHaveLength(1)
    expect(runs[0]).toMatchObject({ base: `main`, baseSha: MAIN, heads: { [w.prId]: HEAD } })
    const pr = (await db.select().from(pullRequests).where(eq(pullRequests.id, w.prId)))[0]!
    expect(pr).toMatchObject({ mergeState: `testing`, mergeTrainId: runs[0]!.id, mergeHeadSha: HEAD })
    // The dispatch read the pipeline file on a device (eCode's own train).
    expect(jobs.calls.map((c) => c.op)).toContain(`read_file`)
  })
})
