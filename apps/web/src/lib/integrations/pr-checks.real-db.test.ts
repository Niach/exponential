// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { boards, devices, messages, repositories, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { inSession } from "@/lib/sessions/messages"

// EXP-1610 against the REAL migrated Postgres: the check writer, the aggregate, the ONE
// waking `pr_checks` line when CI leaves pending, and the queued flag.

vi.mock(`@/lib/pull-requests`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/pull-requests")>()),
  syncPullRequestBodies: async () => {},
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: false }),
}))

// Batch 3: the watchdog sweep reads GitHub through these two seams only.
const githubRuns = vi.hoisted(() => ({ runs: [] as Array<Record<string, unknown>>, reads: [] as string[] }))
vi.mock(`@/lib/integrations/github-app`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/integrations/github-app")>()),
  resolveRepoInstallationToken: async () => `tok`,
}))
vi.mock(`@/lib/integrations/github-client`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/integrations/github-client")>()),
  githubFetch: async (path: string) => {
    githubRuns.reads.push(path)
    if (path.includes(`/check-runs`)) {
      return new Response(JSON.stringify({ total_count: githubRuns.runs.length, check_runs: githubRuns.runs }), { status: 200 })
    }
    return new Response(`{}`, { status: 404 })
  },
}))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors') AS ok
`)
if (!ready.ok) console.info(`[pr-checks.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { recordPullRequestOpened } = await import(`@/lib/integrations/pr-sync`)
const { writePullRequestChecks, flagQueuedChecks, sweepUnsettledChecks, resetPrChecksForTests } = await import(`@/lib/integrations/pr-checks`)
const { pullRequestChecks, pullRequests } = await import(`@/db/schema`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `checks-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@checks.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) {
    await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  }
  if (made.users.length) {
    await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
  }
})

/** A private thread with a child; the child opens PR #7. */
async function threadWithPr() {
  const owner = await user()
  const [team] = await db
    .insert(teams)
    .values({ name: `Checks`, slug: `checks-${id8()}` })
    .returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `RF` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const thread = (await sendSession({ userId: owner, input: { teamId, deviceId, text: `work` } })).sessionId
  const child = (await sendSession({ userId: owner, input: { parentId: thread, text: `part` } })).sessionId
  const fullName = `o/checks-${id8()}`
  const [repo] = await db
    .insert(repositories)
    .values({ teamId, fullName, defaultBranch: `main` })
    .returning({ id: repositories.id })
  const { pullRequest } = await recordPullRequestOpened({
    teamId,
    rootSessionId: thread,
    sessionId: child,
    repositoryId: repo!.id,
    repoFullName: fullName,
    number: 7,
    url: `https://github.com/${fullName}/pull/7`,
    title: `Part`,
    head: `exp/part-${id8()}`,
    base: `main`,
    issueIds: [],
    actorUserId: owner,
  })
  return { owner, thread, child, prId: pullRequest.id }
}

const ciLines = (sessionId: string) =>
  db
    .select()
    .from(messages)
    .where(and(inSession(sessionId), sql`${messages.card}->>'event' = 'pr_checks'`))

const at = (minute: number) => new Date(Date.UTC(2026, 10, 1, 10, minute))
const run = (name: string, status: `queued` | `in_progress` | `completed`, conclusion: string | null, minute = 0) => ({
  name,
  status,
  conclusion,
  startedAt: at(minute),
  completedAt: status === `completed` ? at(minute + 3) : null,
  detailsUrl: `https://github.com/o/r/actions/runs/${name}`,
})

const stateOf = async (prId: string) =>
  (await db.select({ s: pullRequests.checksState }).from(pullRequests).where(eq(pullRequests.id, prId)))[0]!.s

describe.skipIf(!ready.ok)(`EXP-1610: CI checks`, () => {
  it(`pending → failure wakes the opener ONCE, naming the red check and its logs`, async () => {
    const w = await threadWithPr()
    await writePullRequestChecks({ pullRequestId: w.prId, headSha: `aaa`, checks: [run(`gate`, `in_progress`, null), run(`lint`, `queued`, null)] })
    expect(await stateOf(w.prId)).toBe(`pending`)
    expect(await ciLines(w.child)).toEqual([])
    await writePullRequestChecks({ pullRequestId: w.prId, headSha: `aaa`, checks: [run(`gate`, `completed`, `failure`)] })
    expect(await stateOf(w.prId)).toBe(`failure`)
    // a redelivery changes nothing and posts nothing
    await writePullRequestChecks({ pullRequestId: w.prId, headSha: `aaa`, checks: [run(`gate`, `completed`, `failure`)] })
    expect(await ciLines(w.child)).toEqual([
      expect.objectContaining({
        authorKind: `user`,
        consumedAt: null,
        body: `CI on #7: failure (gate (https://github.com/o/r/actions/runs/gate)).`,
        card: { kind: `event`, event: `pr_checks`, pullRequestId: w.prId, checksState: `failure`, headSha: `aaa` },
      }),
    ])
  })

  it(`a newer head replaces the older head's rows; a late older-head delivery is dropped`, async () => {
    const w = await threadWithPr()
    await writePullRequestChecks({ pullRequestId: w.prId, headSha: `old`, checks: [run(`gate`, `completed`, `failure`, 0)] })
    await writePullRequestChecks({ pullRequestId: w.prId, headSha: `new`, checks: [run(`gate`, `in_progress`, null, 10)] })
    const late = await writePullRequestChecks({ pullRequestId: w.prId, headSha: `old`, checks: [run(`e2e`, `completed`, `success`, 1)] })
    expect(late?.applied).toBe(false)
    const rows = await db.select().from(pullRequestChecks).where(eq(pullRequestChecks.pullRequestId, w.prId))
    expect(rows.map((row) => [row.headSha, row.name, row.status])).toEqual([[`new`, `gate`, `in_progress`]])
    expect(await stateOf(w.prId)).toBe(`pending`)
  })

  it(`a snapshot drops the names it lacks; pending → success wakes with success`, async () => {
    const w = await threadWithPr()
    await writePullRequestChecks({ pullRequestId: w.prId, headSha: `s1`, checks: [run(`gate`, `in_progress`, null), run(`gone`, `queued`, null)] })
    await writePullRequestChecks({ pullRequestId: w.prId, headSha: `s1`, checks: [run(`gate`, `completed`, `success`)], snapshot: true, required: new Set([`gate`]) })
    const rows = await db.select().from(pullRequestChecks).where(eq(pullRequestChecks.pullRequestId, w.prId))
    expect(rows.map((row) => [row.name, row.required])).toEqual([[`gate`, true]])
    expect((await ciLines(w.child)).map((line) => line.body)).toEqual([`CI on #7: success.`])
  })

  // #FEED-199: a wake means the REQUIRED check reported a real outcome at
  // the PR's current head.
  it(`an early workflow never wakes; the required check's success does, once`, async () => {
    const w = await threadWithPr()
    const required = new Set([`gate`])
    await writePullRequestChecks({ pullRequestId: w.prId, headSha: `r1`, checks: [run(`pick`, `completed`, `success`)], required })
    expect(await stateOf(w.prId)).toBe(`success`)
    expect(await ciLines(w.child)).toEqual([])
    await writePullRequestChecks({ pullRequestId: w.prId, headSha: `r1`, checks: [run(`gate`, `in_progress`, null)], required })
    expect(await ciLines(w.child)).toEqual([])
    await writePullRequestChecks({ pullRequestId: w.prId, headSha: `r1`, checks: [run(`gate`, `completed`, `success`)], required })
    await writePullRequestChecks({ pullRequestId: w.prId, headSha: `r1`, checks: [run(`ios-core`, `queued`, null)], required })
    expect((await ciLines(w.child)).map((line) => line.body)).toEqual([`CI on #7: success.`])
  })

  it(`a cancelled run wakes nobody; a delivery for a superseded head is dropped`, async () => {
    const w = await threadWithPr()
    const required = new Set([`gate`])
    await writePullRequestChecks({ pullRequestId: w.prId, headSha: `c1`, checks: [run(`gate`, `completed`, `cancelled`)], required })
    expect(await ciLines(w.child)).toEqual([])
    const stale = await writePullRequestChecks({
      pullRequestId: w.prId,
      headSha: `c1`,
      prHeadSha: `c2`,
      checks: [run(`gate`, `completed`, `failure`, 5)],
      required,
    })
    expect(stale).toBeNull()
    expect(await ciLines(w.child)).toEqual([])
    await writePullRequestChecks({ pullRequestId: w.prId, headSha: `c2`, prHeadSha: `c2`, checks: [run(`gate`, `completed`, `failure`, 10)], required })
    expect((await ciLines(w.child)).map((line) => line.body)).toEqual([
      `CI on #7: failure (gate (https://github.com/o/r/actions/runs/gate)).`,
    ])
  })

  // 0f6fb188: #1714 went green at 02:34 with no check webhook subscribed;
  // its line came at 03:00, off the next merge call. The watchdog's sweep
  // reads the pending PR itself.
  it(`the watchdog sweep wakes a pending PR's session without any webhook`, async () => {
    resetPrChecksForTests()
    const w = await threadWithPr()
    await writePullRequestChecks({ pullRequestId: w.prId, headSha: `w1`, checks: [run(`gate`, `in_progress`, null)] })
    githubRuns.runs = [
      { name: `gate`, status: `completed`, conclusion: `success`, head_sha: `w1`, started_at: at(0).toISOString(), completed_at: at(3).toISOString() },
    ]
    const now = new Date()
    expect(await sweepUnsettledChecks(now)).toBeGreaterThanOrEqual(1)
    expect(await stateOf(w.prId)).toBe(`success`)
    expect((await ciLines(w.child)).map((line) => line.body)).toEqual([`CI on #7: success.`])
    // settled: a later tick posts nothing more
    await sweepUnsettledChecks(new Date(now.getTime() + 5 * 60_000))
    expect(await ciLines(w.child)).toHaveLength(1)
  })

  it(`the sweep skips a pending PR that has not moved in hours`, async () => {
    resetPrChecksForTests()
    const w = await threadWithPr()
    await writePullRequestChecks({ pullRequestId: w.prId, headSha: `z1`, checks: [run(`gate`, `in_progress`, null)] })
    githubRuns.runs = [
      { name: `gate`, status: `completed`, conclusion: `success`, head_sha: `z1`, started_at: at(0).toISOString(), completed_at: at(3).toISOString() },
    ]
    await sweepUnsettledChecks(new Date(Date.now() + 7 * 60 * 60_000))
    expect(await stateOf(w.prId)).toBe(`pending`)
  })

  it(`a check queued past the limit is flagged once`, async () => {
    const w = await threadWithPr()
    await writePullRequestChecks({ pullRequestId: w.prId, headSha: `q1`, checks: [run(`android`, `queued`, null, 0)] })
    const later = new Date(at(0).getTime() + 31 * 60_000)
    expect(await flagQueuedChecks(later)).toBeGreaterThanOrEqual(1)
    await flagQueuedChecks(later)
    expect((await ciLines(w.child)).map((line) => line.body)).toEqual([
      `CI on #7: android queued for 30+ minutes; no runner picked it up. Tell a person if it stays queued.`,
    ])
  })
})
