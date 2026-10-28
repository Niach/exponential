// @vitest-environment node
import { afterAll, afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db, pool } from "@/db/connection"
import { boards, devices, messages, pullRequests, repositories, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// #EXP-1413 against the REAL migrated Postgres (harness from
// merge-refusal.real-db.test.ts): the lifecycle edges (a turn ending in
// `sessions.report`, `endSession`, `endHostedSessions`) run the yolo
// automerge once the tree is complete (nothing queued, busy, waiting on a
// person, Stopped or holding unread input); a refusal lands in the thread
// once per PR head (session-thread.json EXP-1413).

const merge = vi.fn()
const headSha = vi.fn()
vi.mock(`@/lib/pull-requests`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/pull-requests")>()),
  syncPullRequestBodies: async () => {},
  mergePullRequestOrThrow: (args: unknown) => merge(args),
  pullRequestHeadSha: (id: string) => headSha(id),
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
const { endHostedSessions, endSession, stopSession } = await import(`@/lib/sessions/lifecycle`)
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
  headSha.mockReset()
  headSha.mockResolvedValue(`sha-1`)
})

/** The device delivered every message of the thread (nothing unread). */
const readAll = (rootId: string) =>
  db.update(messages).set({ consumedAt: new Date() }).where(eq(messages.rootId, rootId))

/** Fire-and-forget edges: give a skipped one the time a fired one takes. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 300))
// A case that ends on its first merge may still have a trigger in flight; it
// must land before the next case's mockReset, never as that case's call.
afterEach(settle)

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
  await readAll(root)
  const report = (
    sessionId: string,
    input: {
      status?: `idle` | `ended`
      busy?: boolean
      needsInput?: boolean
      blocked?: { kind: string } | null
      interrupted?: boolean
    }
  ) =>
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

  it(`a child ending leaves its report unread in the idle parent: nothing merges until the parent's turn on it ends`, async () => {
    const t = await thread()
    await t.report(t.root, { status: `idle`, busy: false })
    // Its turn is over (idle), then its process goes: it ends (NEW-17: a
    // process going MID-turn is an interruption, not an end).
    await settle()
    await db.update(sessions).set({ status: `idle`, busy: false }).where(eq(sessions.id, t.child))
    await t.report(t.child, { status: `ended` })
    await settle()
    expect(merge).not.toHaveBeenCalled()
    expect(await maybeMergeYoloThread(t.root)).toEqual({ status: `waiting` })
    // The parent reads the report and its turn on it ends.
    await readAll(t.root)
    await db.update(sessions).set({ status: `running`, busy: true }).where(eq(sessions.id, t.root))
    await t.report(t.root, { status: `idle`, busy: false })
    await vi.waitFor(() => expect(merge).toHaveBeenCalledWith({ userId: t.owner, pullRequestId: t.prId }))

    const u = await thread()
    await db.update(sessions).set({ status: `idle`, busy: false }).where(eq(sessions.id, u.root))
    await endSession({ userId: u.owner, sessionId: u.child })
    expect(await maybeMergeYoloThread(u.root)).toEqual({ status: `waiting` })
    expect(merge).toHaveBeenCalledTimes(1)
  })

  it(`endHostedSessions ending the whole tree is an edge`, async () => {
    const t = await thread()
    await endHostedSessions([t.root, t.child])
    await vi.waitFor(() => expect(merge).toHaveBeenCalledWith({ userId: t.owner, pullRequestId: t.prId }))
  })

  it(`a person's Stop mid-turn: the interrupted turn end merges nothing until the session works again`, async () => {
    const t = await thread()
    await t.report(t.child, { status: `idle`, busy: false })
    await stopSession({ userId: t.owner, sessionId: t.root })
    await t.report(t.root, { status: `idle`, busy: false })
    await settle()
    expect(merge).not.toHaveBeenCalled()
    expect(await maybeMergeYoloThread(t.root)).toEqual({ status: `waiting` })
    // A later message resumes it: busy again clears the Stop, its end merges.
    await t.report(t.root, { busy: true })
    const [row] = await db.select({ stoppedAt: sessions.stoppedAt }).from(sessions).where(eq(sessions.id, t.root))
    expect(row!.stoppedAt).toBeNull()
    await t.report(t.root, { status: `idle`, busy: false })
    await vi.waitFor(() => expect(merge).toHaveBeenCalledTimes(1))
  })

  it(`#EXP-1418: a Stop that raced a turn ending by itself holds nothing once the device says the turn was not interrupted`, async () => {
    const t = await thread()
    await t.report(t.child, { status: `idle`, busy: false })
    await stopSession({ userId: t.owner, sessionId: t.root })
    // An older device says nothing: the Stop holds the thread.
    const u = await thread()
    await u.report(u.child, { status: `idle`, busy: false })
    await stopSession({ userId: u.owner, sessionId: u.root })
    await u.report(u.root, { status: `idle`, busy: false })
    await settle()
    expect(merge).not.toHaveBeenCalled()
    // `interrupted: false` = the turn finished by itself: the Stop settles.
    await t.report(t.root, { status: `idle`, busy: false, interrupted: false })
    const [row] = await db.select({ stoppedAt: sessions.stoppedAt }).from(sessions).where(eq(sessions.id, t.root))
    expect(row!.stoppedAt).toBeNull()
    await vi.waitFor(() => expect(merge).toHaveBeenCalledWith({ userId: t.owner, pullRequestId: t.prId }))
    expect(merge).toHaveBeenCalledTimes(1)
  })

  it(`#EXP-1418: a turn the device never reported busy for (running → idle) is an edge`, async () => {
    const t = await thread()
    await t.report(t.child, { status: `idle`, busy: false })
    await db.update(sessions).set({ status: `running`, busy: false }).where(eq(sessions.id, t.root))
    await t.report(t.root, { status: `idle` })
    await vi.waitFor(() => expect(merge).toHaveBeenCalledTimes(1))
  })

  it(`Stopping a never-started session merges nothing`, async () => {
    const t = await thread()
    await t.report(t.root, { status: `idle`, busy: false })
    await db.update(sessions).set({ status: `queued`, busy: false, startedAt: null }).where(eq(sessions.id, t.child))
    await stopSession({ userId: t.owner, sessionId: t.child })
    await settle()
    expect(merge).not.toHaveBeenCalled()
  })

  it(`a turn ending on an open question/plan card or a wall waits for the person`, async () => {
    const t = await thread()
    await t.report(t.child, { status: `idle`, busy: false })
    await t.report(t.root, { status: `idle`, busy: false, needsInput: true })
    await settle()
    expect(await maybeMergeYoloThread(t.root)).toEqual({ status: `waiting` })

    const u = await thread()
    await u.report(u.child, { status: `idle`, busy: false, blocked: { kind: `usage_limit` } })
    await u.report(u.root, { status: `idle`, busy: false })
    await settle()
    expect(await maybeMergeYoloThread(u.root)).toEqual({ status: `waiting` })
    expect(merge).not.toHaveBeenCalled()
  })

  it(`only a real busy → idle edge fires: a repeated idle report does not`, async () => {
    const t = await thread()
    await t.report(t.child, { status: `idle`, busy: false })
    await t.report(t.root, { status: `idle`, busy: false })
    await vi.waitFor(() => expect(merge).toHaveBeenCalledTimes(1))
    await t.report(t.root, { status: `idle`, busy: false })
    await t.report(t.root, { status: `idle` })
    await settle()
    expect(merge).toHaveBeenCalledTimes(1)
  })

  it(`a draft is skipped silently: no merge, no refusal, its stack waits`, async () => {
    const t = await thread()
    await db.update(pullRequests).set({ state: `draft` }).where(eq(pullRequests.id, t.prId))
    await t.report(t.child, { status: `idle`, busy: false })
    await t.report(t.root, { status: `idle`, busy: false })
    await settle()
    expect(await maybeMergeYoloThread(t.root)).toEqual({ status: `merged`, outcomes: { [t.prId]: { kind: `draft` } } })
    expect(merge).not.toHaveBeenCalled()
    expect(await refusals(t.root)).toHaveLength(0)
  })

  it(`two concurrent triggers merge once (in process, and across instances by the advisory lock)`, async () => {
    const t = await thread()
    await db.update(sessions).set({ status: `idle`, busy: false }).where(eq(sessions.rootId, t.root))
    let release!: () => void
    // The merge lands in the DB (as the real one does): EXP-1418's re-run
    // of the coalesced second trigger finds nothing left to merge.
    merge.mockImplementation(
      () =>
        new Promise((resolve) => {
          release = () =>
            void db
              .update(pullRequests)
              .set({ state: `merged` })
              .where(eq(pullRequests.id, t.prId))
              .then(() => resolve({ queued: false }))
        })
    )
    const a = maybeMergeYoloThread(t.root)
    const b = maybeMergeYoloThread(t.root)
    await vi.waitFor(() => expect(merge).toHaveBeenCalledTimes(1))
    release()
    expect(await a).toEqual(await b)
    expect(merge).toHaveBeenCalledTimes(1)
    await db.update(pullRequests).set({ state: `open` }).where(eq(pullRequests.id, t.prId))

    // Another instance holds the thread's lock: this one stands down.
    const other = await pool.connect()
    try {
      await other.query(`SELECT pg_advisory_lock(hashtext($1))`, [`yolo_thread:${t.root}`])
      expect(await maybeMergeYoloThread(t.root)).toEqual({ status: `waiting` })
      await other.query(`SELECT pg_advisory_unlock(hashtext($1))`, [`yolo_thread:${t.root}`])
    } finally {
      other.release()
    }
    // (the waiting pass schedules one retry: the merge below lands in the
    // DB, so that retry finds nothing left.)
    merge.mockImplementation(async () => {
      await db.update(pullRequests).set({ state: `merged` }).where(eq(pullRequests.id, t.prId))
      return { queued: false }
    })
    expect(await maybeMergeYoloThread(t.root)).toMatchObject({ status: `merged` })
    expect(merge).toHaveBeenCalledTimes(2)
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
    // The root reads it and its turn ends on it: the next pass stays quiet,
    // even when GitHub words the same refusal differently.
    await readAll(t.root)
    await db.update(sessions).set({ status: `running`, busy: true }).where(eq(sessions.id, t.root))
    merge.mockRejectedValue(new MergeRefusal({ kind: `checks`, message: `Required status check "gate" is expected.` }))
    await t.report(t.root, { status: `idle`, busy: false })
    await vi.waitFor(() => expect(merge).toHaveBeenCalledTimes(2))
    expect(await maybeMergeYoloThread(t.root)).toMatchObject({ status: `merged` })
    expect(await refusals(t.root)).toHaveLength(1)
    // A push = a new head: its refusal wakes the root again.
    headSha.mockResolvedValue(`sha-2`)
    expect(await maybeMergeYoloThread(t.root)).toMatchObject({ status: `merged` })
    expect(await refusals(t.root)).toHaveLength(2)
  })
})
