// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { boards, devices, messages, repositories, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// #EXP-1391 + #EXP-1408 against the REAL migrated Postgres (harness from
// bus-private.real-db.test.ts): a refused merge posts ONE waking
// `pr_merge_refused` line into the PR's root; a PR opened by a PRIVATE
// session the root's readers may not see gets it in the opener instead.

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

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors') AS ok
`)
if (!ready.ok) console.info(`[merge-refusal.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { recordPullRequestOpened } = await import(`@/lib/integrations/pr-sync`)
const { postMergeRefusal, mergeRefusalText } = await import(`@/lib/merge-refusal`)
const { MergeRefusal } = await import(`@/lib/trpc/pr-merge-error`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `refusal-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@refusal.test`}, true, false, now(), now())
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
    .values({ name: `Refusal`, slug: `refusal-${id8()}` })
    .returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `RF` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const thread = (await sendSession({ userId: owner, input: { teamId, deviceId, text: `work` } })).sessionId
  const child = (await sendSession({ userId: owner, input: { parentId: thread, text: `part` } })).sessionId
  const fullName = `o/refusal-${id8()}`
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

const refusals = (sessionId: string) =>
  db
    .select()
    .from(messages)
    .where(and(eq(messages.sessionId, sessionId), sql`${messages.card}->>'event' = 'pr_merge_refused'`))

const refusal = () => new MergeRefusal({ kind: `checks`, message: `Required status check "gate" is failing.` })

describe.skipIf(!ready.ok)(`#EXP-1391: a refused merge goes into the thread`, () => {
  it(`lands once in the root as waking input; a repeat while unread adds nothing`, async () => {
    const w = await threadWithPr()
    expect(await postMergeRefusal({ pullRequestId: w.prId, actorUserId: w.owner, error: refusal() })).toBe(true)
    expect(await postMergeRefusal({ pullRequestId: w.prId, actorUserId: w.owner, error: refusal() })).toBe(true)
    expect(await refusals(w.thread)).toEqual([
      expect.objectContaining({
        authorKind: `user`,
        authorId: w.owner,
        body: mergeRefusalText(7, `Required status check "gate" is failing.`, `checks`),
        consumedAt: null,
        card: { kind: `event`, event: `pr_merge_refused`, pullRequestId: w.prId },
      }),
    ])
    expect(await refusals(w.child)).toEqual([])
  })

  it(`#EXP-1408: a private opener hidden from the root's readers gets it instead`, async () => {
    const w = await threadWithPr()
    // The root's conversation becomes team-readable; the opener stays private.
    await db.update(sessions).set({ shared: true }).where(eq(sessions.id, w.thread))
    expect(await postMergeRefusal({ pullRequestId: w.prId, actorUserId: w.owner, error: refusal() })).toBe(true)
    expect(await refusals(w.thread)).toEqual([])
    expect(await refusals(w.child)).toEqual([
      expect.objectContaining({
        body: mergeRefusalText(7, `Required status check "gate" is failing.`, `checks`),
        consumedAt: null,
      }),
    ])
  })

  it(`automerge: the same refusal posts once, read or not; a new one is news`, async () => {
    const w = await threadWithPr()
    const auto = (error: InstanceType<typeof MergeRefusal>) =>
      postMergeRefusal({ pullRequestId: w.prId, actorUserId: w.owner, error, automerge: true })
    expect(await auto(refusal())).toBe(true)
    // The root read it and ended its turn: the next pass stays quiet.
    await db.update(messages).set({ consumedAt: new Date() }).where(eq(messages.sessionId, w.thread))
    expect(await auto(refusal())).toBe(false)
    expect(await refusals(w.thread)).toHaveLength(1)
    // A different refusal is news.
    expect(await auto(new MergeRefusal({ kind: `conflict`, message: `Conflicts in a.ts` }))).toBe(true)
    expect(await refusals(w.thread)).toHaveLength(2)
  })

  it(`an ended root keeps one display line per refusal, never a repeat`, async () => {
    const w = await threadWithPr()
    await db.update(sessions).set({ status: `ended` }).where(eq(sessions.id, w.thread))
    expect(await postMergeRefusal({ pullRequestId: w.prId, actorUserId: w.owner, error: refusal() })).toBe(false)
    expect(await postMergeRefusal({ pullRequestId: w.prId, actorUserId: w.owner, error: refusal() })).toBe(false)
    const lines = await refusals(w.thread)
    expect(lines).toHaveLength(1)
    expect(lines[0]!.consumedAt).not.toBeNull()
  })
})
