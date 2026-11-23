// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, asc, eq, isNull, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { boards, devices, issues, messages, pullRequests, repositories, sessions, sessionWatches, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// #NEW-49 (packages/eagent fixtures/wakeups.json WATCH EVENTS + watchdog.json
// TIME WATCHES): a session waits on EVENTS on the server, never in a shell:
// a PR's CI, its merge or close, an issue's move or deletion, a time. Each
// fires ONCE as pending input into the watcher; `sessions.watching` (synced)
// says what it waits on. Against the REAL migrated Postgres.

process.env.BETTER_AUTH_SECRET ||= `watches-test-secret-0123456789abcdef`

vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  fireAndForgetIssueNotify: () => {},
  notifySessionBlocked: async () => {},
  notifySessionAsk: async () => {},
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: false }),
}))

const ready = await probeRealDb(
  `SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'session_watches') AS ok`
)
if (!ready.ok) console.info(`[watches.real-db] skipped: ${ready.reason}`)

const { sendSession, endSession } = await import(`@/lib/sessions/send`).then(async (m) => ({
  sendSession: m.sendSession,
  endSession: (await import(`@/lib/sessions/lifecycle`)).endSession,
}))
const { postChecksWake } = await import(`@/lib/integrations/pr-checks`)
const { applyPullRequestClosed, applyPullRequestMerged } = await import(`@/lib/integrations/pr-sync`)
const { emitIssueEventInTx } = await import(`@/lib/events`)
const { trashIssuesInTx } = await import(`@/lib/issue-trash`)
const { fireDueTimeWatches, SESSION_WATCH_MAX, PRIVATE_WATCH_LABEL } = await import(`@/lib/sessions/watches`)
const { inSession } = await import(`@/lib/sessions/messages`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

async function world() {
  const owner = `watches-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${owner}, 'Tester', ${`${owner}@watches.test`}, true, false, now(), now())
  `)
  made.users.push(owner)
  const [team] = await db.insert(teams).values({ name: `Watches`, slug: `watches-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const [repo] = await db
    .insert(repositories)
    .values({ teamId, fullName: `org/watches-${id8()}`, defaultBranch: `main` })
    .returning({ id: repositories.id })
  const [board] = await db
    .insert(boards)
    .values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `WA` })
    .returning({ id: boards.id })
  const coordinator = (await sendSession({ userId: owner, input: { teamId, deviceId, text: `Coordinate.` } })).sessionId
  const pr = async (number: number) =>
    (
      await db
        .insert(pullRequests)
        .values({ teamId, repositoryId: repo!.id, number, url: `https://github.com/org/x/pull/${number}`, title: `t`, head: `feat-${number}`, base: `main` })
        .returning({ id: pullRequests.id })
    )[0]!.id
  const issue = async () =>
    (
      await db
        .insert(issues)
        .values({ boardId: board!.id, teamId, title: `An issue`, creatorId: owner })
        .returning({ id: issues.id, identifier: issues.identifier })
    )[0]!
  const watchFrom = (caller: string, wakeOn: string[]) =>
    sendSession({ userId: owner, callerSessionId: caller, viaMcp: true, input: { wakeOn } })
  const fired = async (sessionId: string) =>
    db
      .select({ body: messages.body, card: messages.card, authorKind: messages.authorKind, consumedAt: messages.consumedAt })
      .from(messages)
      .where(and(inSession(sessionId), sql`${messages.card}->>'event' = 'watch_fired'`))
      .orderBy(asc(messages.createdAt))
  const watching = async (sessionId: string) =>
    (await db.select({ watching: sessions.watching }).from(sessions).where(eq(sessions.id, sessionId)))[0]!.watching
  return { owner, teamId, deviceId, coordinator, pr, issue, watchFrom, fired, watching }
}

describe.skipIf(!ready.ok)(`#NEW-49 event watches`, () => {
  it(`a PR's CI settling wakes its watcher ONCE with the CI line; the watch and its watching entry go`, async () => {
    const w = await world()
    const prId = await w.pr(1130)
    await w.watchFrom(w.coordinator, [`pr:1130`])
    expect(await w.watching(w.coordinator)).toEqual([
      expect.objectContaining({ kind: `pr_checks`, subjectId: prId, label: `CI #1130` }),
    ])
    expect(await postChecksWake({ pullRequestId: prId, headSha: `abc`, state: `success` })).toBe(true)
    const lines = await w.fired(w.coordinator)
    expect(lines).toHaveLength(1)
    expect(lines[0]).toMatchObject({ body: `CI on #1130: success.`, authorKind: `user`, consumedAt: null })
    expect(lines[0]!.card).toMatchObject({ kind: `event`, event: `watch_fired`, sessionId: w.coordinator })
    expect(await w.watching(w.coordinator)).toBeNull()
    // Fired once: the next settle of the same PR wakes nobody here.
    await postChecksWake({ pullRequestId: prId, headSha: `def`, state: `failure` })
    expect(await w.fired(w.coordinator)).toHaveLength(1)
  })

  it(`a merge fires the CI and close watches on it as ONE line; a PR already closed fires at the watch`, async () => {
    const w = await world()
    const prId = await w.pr(1131)
    await w.watchFrom(w.coordinator, [`pr:1131`, `pr:#1131:closed`])
    expect((await w.watching(w.coordinator))?.map((entry) => entry.label)).toEqual([`CI #1131`, `#1131`])
    await applyPullRequestMerged({ pullRequestId: prId })
    expect((await w.fired(w.coordinator)).map((line) => line.body)).toEqual([`#1131 was merged.`])
    expect(await w.watching(w.coordinator)).toBeNull()

    const closed = await w.pr(1132)
    await applyPullRequestClosed({ pullRequestId: closed })
    await w.watchFrom(w.coordinator, [`pr:1132:closed`])
    expect((await w.fired(w.coordinator)).map((line) => line.body)).toEqual([
      `#1131 was merged.`,
      `#1132 was closed without merging.`,
    ])
    expect(await w.watching(w.coordinator)).toBeNull()
  })

  it(`an issue's move fires a status-less watch; a target status waits for it; a trashed issue fires the rest`, async () => {
    const w = await world()
    const issue = await w.issue()
    const other = (await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `Other.` } })).sessionId
    await w.watchFrom(w.coordinator, [`issue:${issue.identifier}:done`])
    await w.watchFrom(other, [`issue:${issue.identifier}`])
    const move = (status: `in_review` | `done`, toName: string) =>
      db.transaction(async (tx) => {
        await tx.update(issues).set({ status }).where(eq(issues.id, issue.id))
        await emitIssueEventInTx(tx, {
          issueId: issue.id,
          teamId: w.teamId,
          name: `issue.status_changed`,
          payload: { toName },
          emittedBy: { userId: w.owner },
        })
      })
    await move(`in_review`, `In Review`)
    expect((await w.fired(other)).map((line) => line.body)).toEqual([`${issue.identifier} moved to In Review.`])
    expect(await w.fired(w.coordinator)).toEqual([])
    expect(await w.watching(w.coordinator)).toEqual([expect.objectContaining({ kind: `issue_status`, label: issue.identifier })])
    // A watch made by status NAME matches too.
    await w.watchFrom(other, [`issue:${issue.identifier}:Done`])
    await move(`done`, `Done`)
    expect((await w.fired(w.coordinator)).map((line) => line.body)).toEqual([`${issue.identifier} moved to Done.`])
    expect((await w.fired(other)).map((line) => line.body)).toEqual([
      `${issue.identifier} moved to In Review.`,
      `${issue.identifier} moved to Done.`,
    ])

    const gone = await w.issue()
    await w.watchFrom(w.coordinator, [`issue:${gone.identifier}:done`])
    await db.transaction((tx) => trashIssuesInTx(tx, [gone.id], { userId: w.owner, origin: { viaMcp: false, sessionId: null } }))
    expect((await w.fired(w.coordinator)).at(-1)?.body).toBe(`${gone.identifier} was deleted; its watches are gone.`)
    expect(await w.watching(w.coordinator)).toBeNull()
  })

  it(`a time watch fires on the first tick at or after its time, once; a later one keeps waiting`, async () => {
    const w = await world()
    const now = Date.now()
    await w.watchFrom(w.coordinator, [`at:${new Date(now + 60_000).toISOString()}`, `at:${new Date(now + 3_600_000).toISOString()}`])
    expect(await w.watching(w.coordinator)).toHaveLength(2)
    expect(await fireDueTimeWatches(new Date(now))).toBe(0)
    expect(await fireDueTimeWatches(new Date(now + 61_000))).toBeGreaterThanOrEqual(1)
    const lines = await w.fired(w.coordinator)
    expect(lines).toHaveLength(1)
    expect(lines[0]!.body).toMatch(/^It is .+ UTC, the time you asked to be woken at\.$/)
    expect(await w.watching(w.coordinator)).toEqual([expect.objectContaining({ kind: `at` })])
    await fireDueTimeWatches(new Date(now + 62_000))
    expect(await w.fired(w.coordinator)).toHaveLength(1)
  })

  it(`#FEED-270: a future time with an offset posts nothing pending at the watch; it fires at its instant only`, async () => {
    const w = await world()
    const at = new Date(Math.ceil(Date.now() / 60_000) * 60_000 + 30 * 60_000)
    const local = new Date(at.getTime() + 3_600_000).toISOString().replace(`Z`, `+01:00`)
    const pending = async () =>
      (await db.select({ id: messages.id }).from(messages).where(and(inSession(w.coordinator), isNull(messages.consumedAt)))).length
    const before = await pending()
    await w.watchFrom(w.coordinator, [`at:${local}`])
    expect(await w.watching(w.coordinator)).toEqual([expect.objectContaining({ kind: `at`, at: at.toISOString() })])
    expect(await pending()).toBe(before)
    expect(await fireDueTimeWatches(new Date(at.getTime() - 1))).toBe(0)
    expect(await w.fired(w.coordinator)).toEqual([])
    expect(await fireDueTimeWatches(at)).toBeGreaterThanOrEqual(1)
    expect(await w.fired(w.coordinator)).toHaveLength(1)
  })

  it(`sessions and events share the cap; -id cancels; an End drops the event watches, never another's`, async () => {
    const w = await world()
    const others = await Promise.all(
      Array.from({ length: SESSION_WATCH_MAX - 1 }, (_, n) =>
        sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `Other ${n}.` } }).then((r) => r.sessionId)
      )
    )
    const prId = await w.pr(1140)
    await w.watchFrom(w.coordinator, [...others, `pr:1140`])
    expect(await w.watching(w.coordinator)).toHaveLength(SESSION_WATCH_MAX)
    await expect(w.watchFrom(w.coordinator, [`pr:1140:closed`])).rejects.toMatchObject({
      message: expect.stringContaining(`at most ${SESSION_WATCH_MAX} things`),
    })
    expect(await w.watching(w.coordinator)).toHaveLength(SESSION_WATCH_MAX)
    // A cancel (a watch id or a watched session's id) frees a slot in the same call.
    const ciWatch = (await w.watching(w.coordinator))!.find((entry) => entry.kind === `pr_checks`)!
    await w.watchFrom(w.coordinator, [`-${ciWatch.id}`, `-${others[0]}`, `pr:1140:closed`])
    const after = (await w.watching(w.coordinator))!
    expect(after).toHaveLength(SESSION_WATCH_MAX - 1)
    expect(after.some((entry) => entry.id === ciWatch.id || entry.subjectId === others[0])).toBe(false)
    expect(after.at(-1)).toMatchObject({ kind: `pr_closed`, subjectId: prId })

    // The watched session ends: every watch on it goes; the watcher's End drops its event watches.
    const watcher = others[1]!
    await w.watchFrom(watcher, [`pr:1140`])
    await endSession({ userId: w.owner, sessionId: others[2]! })
    expect((await w.watching(w.coordinator))!.some((entry) => entry.subjectId === others[2])).toBe(false)
    await endSession({ userId: w.owner, sessionId: watcher })
    const rows = await db.select().from(sessionWatches).where(eq(sessionWatches.sessionId, watcher))
    expect(rows.filter((row) => row.kind !== `session`)).toEqual([])
    expect((await w.watching(w.coordinator))!.some((entry) => entry.kind === `pr_closed`)).toBe(true)
  })

  it(`a watched private session reads as a private chat from a shared watcher`, async () => {
    const w = await world()
    const privateId = crypto.randomUUID()
    const [mine] = await db
      .insert(sessions)
      .values({
        id: privateId,
        rootId: privateId,
        teamId: w.teamId,
        userId: w.owner,
        deviceId: w.deviceId,
        depth: 0,
        status: `idle`,
        shared: false,
        title: `My secret plan`,
      } as never)
      .returning({ id: sessions.id })
    await db.update(sessions).set({ shared: true }).where(eq(sessions.id, w.coordinator))
    await w.watchFrom(w.coordinator, [mine!.id])
    expect(await w.watching(w.coordinator)).toEqual([expect.objectContaining({ kind: `session`, label: PRIVATE_WATCH_LABEL })])
  })
})
