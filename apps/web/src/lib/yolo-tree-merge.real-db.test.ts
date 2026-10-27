// @vitest-environment node
import { afterAll, beforeEach, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { boards, devices, messages, repositories, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// #EXP-1413 against the REAL migrated Postgres (harness from
// merge-refusal.real-db.test.ts): the lifecycle edges (a turn ending in
// `sessions.report`, `endSession`) run the yolo automerge once the tree is
// idle; a refusal lands in the thread once (session-thread.json EXP-1413).

const merge = vi.fn()
vi.mock(`@/lib/pull-requests`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/pull-requests")>()),
  syncPullRequestBodies: async () => {},
  mergePullRequestOrThrow: (args: unknown) => merge(args),
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
if (!ready.ok) console.info(`[yolo-tree-merge.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { endSession } = await import(`@/lib/sessions/lifecycle`)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
const { recordPullRequestOpened } = await import(`@/lib/integrations/pr-sync`)
const { mergeRefusalText } = await import(`@/lib/merge-refusal`)
const { MergeRefusal } = await import(`@/lib/trpc/pr-merge-error`)
const { maybeMergeYoloThread } = await import(`@/lib/yolo-tree-merge`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `yolo-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@yolo.test`}, true, false, now(), now())
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

beforeEach(() => {
  merge.mockReset()
  merge.mockResolvedValue({ queued: false })
})

/** A thread (running, busy) with a child (running, busy) that opened PR #7. */
async function thread(yoloMode = true) {
  const owner = await user()
  const [team] = await db
    .insert(teams)
    .values({ name: `Yolo`, slug: `yolo-${id8()}`, yoloMode })
    .returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `YO` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const root = (await sendSession({ userId: owner, input: { teamId, deviceId, text: `work` } })).sessionId
  const child = (await sendSession({ userId: owner, input: { parentId: root, text: `part` } })).sessionId
  const now = new Date()
  await db
    .update(sessions)
    .set({ status: `running`, busy: true, startedAt: now, lastTurnAt: now })
    .where(eq(sessions.rootId, root))
  const fullName = `o/yolo-${id8()}`
  const [repo] = await db
    .insert(repositories)
    .values({ teamId, fullName, defaultBranch: `main` })
    .returning({ id: repositories.id })
  const { pullRequest } = await recordPullRequestOpened({
    teamId,
    rootSessionId: root,
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
  const report = (sessionId: string, input: { status?: `idle` | `ended`; busy?: boolean }) =>
    sessionsRouter
      .createCaller({ session: { user: { id: owner } }, db } as never)
      .report({ sessionId, deviceId, ...input })
  return { owner, root, child, prId: pullRequest.id, report }
}

const refusals = (sessionId: string) =>
  db
    .select()
    .from(messages)
    .where(and(eq(messages.sessionId, sessionId), sql`${messages.card}->>'event' = 'pr_merge_refused'`))

describe.skipIf(!ready.ok)(`#EXP-1413: the yolo automerge runs on the lifecycle edges`, () => {
  it(`a turn ending while another session of the tree works merges nothing`, async () => {
    const t = await thread()
    await t.report(t.root, { status: `idle`, busy: false })
    expect(await maybeMergeYoloThread(t.root)).toEqual({ status: `waiting` })
    expect(merge).not.toHaveBeenCalled()
  })

  it(`the tree's last turn ending merges its PRs`, async () => {
    const t = await thread()
    await t.report(t.child, { status: `idle`, busy: false })
    await t.report(t.root, { status: `idle`, busy: false })
    await vi.waitFor(() => expect(merge).toHaveBeenCalledWith({ userId: t.owner, pullRequestId: t.prId }))
  })

  it(`a session ending (the child's process gone, then End) is an edge too`, async () => {
    const t = await thread()
    await t.report(t.root, { status: `idle`, busy: false })
    await t.report(t.child, { status: `ended` })
    await vi.waitFor(() => expect(merge).toHaveBeenCalledTimes(1))

    const u = await thread()
    await db.update(sessions).set({ status: `idle`, busy: false }).where(eq(sessions.id, u.root))
    await endSession({ userId: u.owner, sessionId: u.child })
    await vi.waitFor(() => expect(merge).toHaveBeenCalledWith({ userId: u.owner, pullRequestId: u.prId }))
  })

  it(`a team without yolo mode never merges`, async () => {
    const t = await thread(false)
    await t.report(t.child, { status: `idle`, busy: false })
    await t.report(t.root, { status: `idle`, busy: false })
    expect(await maybeMergeYoloThread(t.root)).toEqual({ status: `not_yolo` })
    expect(merge).not.toHaveBeenCalled()
  })

  it(`a refusal goes into the thread once, however many turns end on it`, async () => {
    const t = await thread()
    const reason = `Required status check "gate" is failing.`
    merge.mockRejectedValue(new MergeRefusal({ kind: `checks`, message: reason }))
    await t.report(t.child, { status: `idle`, busy: false })
    await t.report(t.root, { status: `idle`, busy: false })
    await vi.waitFor(async () => expect(await refusals(t.root)).toHaveLength(1))
    expect((await refusals(t.root))[0]).toEqual(
      expect.objectContaining({ body: mergeRefusalText(7, reason, `checks`), consumedAt: null })
    )
    // The root reads it and its turn ends on it: the next pass stays quiet.
    await db.update(messages).set({ consumedAt: new Date() }).where(eq(messages.sessionId, t.root))
    await db.update(sessions).set({ status: `running`, busy: true }).where(eq(sessions.id, t.root))
    await t.report(t.root, { status: `idle`, busy: false })
    await vi.waitFor(() => expect(merge).toHaveBeenCalledTimes(2))
    expect(await maybeMergeYoloThread(t.root)).toMatchObject({ status: `merged` })
    expect(await refusals(t.root)).toHaveLength(1)
  })
})
