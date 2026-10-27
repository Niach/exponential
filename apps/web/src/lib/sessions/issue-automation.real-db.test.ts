// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  boards,
  devices,
  issueEvents,
  issueRelations,
  issues,
  issueStatuses,
  repositories,
  sessionIssues,
  sessions,
  teamMembers,
  teams,
} from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// EXP-1334/NEW-1: the status automation against the REAL migrated Postgres
// — a WORKER (a session with a repository) taking an issue starts it, a PR
// closes its session's share of the thread, a stacked PR's issues wait for
// the stack's bottom to land, a parent issue waits for its linked
// sub-issues, and the session side of issue-lifecycle.json (taken /
// unlinked, no settle-back). The world's board carries the repository, so
// its threads are workers. Each case builds a fresh team; the suite deletes
// every team (and user) it made (cascade).

vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
}))
vi.mock(`@/lib/integrations/github-app`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/integrations/github-app")>()),
  githubAppConfigured: () => false,
}))

const ready = await probeRealDb(`
  SELECT (
    EXISTS (SELECT 1 FROM information_schema.columns
      WHERE table_name = 'sessions' AND column_name = 'wake_on')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
  ) AS ok
`)
if (!ready.ok) console.info(`[issue-automation.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { forwardIssues, linkIssues, manualLinkText, unlinkIssue } = await import(`@/lib/sessions/issues`)
const { prDefaultIssueIds } = await import(`@/lib/sessions/pr-issues`)
const { endSession } = await import(`@/lib/sessions/lifecycle`)
const {
  applyPullRequestBase,
  applyPullRequestMerged,
  livePullRequestsForIssue,
  loadPullRequest,
  recordPullRequestOpened,
} = await import(
  `@/lib/integrations/pr-sync`
)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function world() {
  const owner = `auto-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${owner}, 'Tester', ${`${owner}@automation.test`}, true, false, now(), now())
  `)
  made.users.push(owner)
  const [team] = await db
    .insert(teams)
    .values({ name: `Automation`, slug: `automation-${id8()}` })
    .returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const fullName = `o/r-${id8()}`
  const [repo] = await db
    .insert(repositories)
    .values({ teamId, fullName, defaultBranch: `main` })
    .returning({ id: repositories.id })
  // The board's repository makes a thread on its issues a WORKER.
  const [board] = await db
    .insert(boards)
    .values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `AU`, repositoryId: repo!.id })
    .returning({ id: boards.id })
  const issue = async (title = `An issue`) =>
    (
      await db
        .insert(issues)
        .values({ boardId: board!.id, teamId, title, creatorId: owner })
        .returning({ id: issues.id, identifier: issues.identifier })
    )[0]!
  const parentOf = async (parent: string, child: string) =>
    db.insert(issueRelations).values({
      issueId: parent,
      relatedIssueId: child,
      type: `parent`,
      teamId,
      boardId: board!.id,
    })
  const status = async (issueId: string) =>
    (
      await db
        .select({ status: issues.status, builtinKey: issueStatuses.builtinKey })
        .from(issues)
        .leftJoin(issueStatuses, eq(issueStatuses.id, issues.statusId))
        .where(eq(issues.id, issueId))
    )[0]!
  const thread = async (issueIds: string[], text = `go`) =>
    (await sendSession({ userId: owner, input: { teamId, deviceId, issueIds, text } })).sessionId
  let prNumber = 0
  const openPr = async (opts: {
    rootId: string
    sessionId?: string
    head: string
    base?: string
    issueIds: string[]
    title?: string
  }) => {
    prNumber += 1
    return (
      await recordPullRequestOpened({
        teamId,
        rootSessionId: opts.rootId,
        sessionId: opts.sessionId ?? opts.rootId,
        repositoryId: repo!.id,
        repoFullName: fullName,
        number: prNumber,
        url: `https://github.com/${fullName}/pull/${prNumber}`,
        title: opts.title ?? `PR ${prNumber}`,
        head: opts.head,
        base: opts.base ?? `main`,
        issueIds: opts.issueIds,
        actorUserId: owner,
      })
    ).pullRequest
  }
  /** A root with no repository (no issues named at the start): no worker. */
  const chat = async (text = `chat`) =>
    (await sendSession({ userId: owner, input: { teamId, deviceId, text } })).sessionId
  return { owner, teamId, deviceId, repoId: repo!.id, issue, parentOf, status, thread, chat, openPr }
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

describe.skipIf(!ready.ok)(`EXP-1334 status automation`, () => {
  it(`a worker's link starts a backlog issue and resumes one in review; a repo-less session moves nothing`, async () => {
    const w = await world()
    const fresh = await w.issue(`Fresh`)
    const reviewing = await w.issue(`Reviewing`)
    const idle = await w.issue(`Chatted about`)
    await db.update(issues).set({ status: `in_review` }).where(eq(issues.id, reviewing.id))

    // The quiet start link takes it: the thread is a worker (the board's repo).
    const rootId = await w.thread([fresh.id])
    expect((await db.select().from(sessions).where(eq(sessions.id, rootId)))[0]!.repositoryId).toBe(w.repoId)
    expect(await w.status(fresh.id)).toEqual({ status: `in_progress`, builtinKey: `in_progress` })
    const [event] = await db
      .select({ payload: issueEvents.payload, actor: issueEvents.actorUserId })
      .from(issueEvents)
      .where(and(eq(issueEvents.issueId, fresh.id), eq(issueEvents.type, `status_changed`)))
    expect(event).toMatchObject({
      actor: w.owner,
      payload: { automation: true, toName: `In Progress`, event: `taken`, sessionId: rootId },
    })

    // In review → In progress (`resumed`): the worker took it again.
    await db.transaction((tx) =>
      linkIssues(tx, { sessionId: rootId, issueIds: [reviewing.id], source: `agent`, addedBy: w.owner, addedBySessionId: rootId })
    )
    expect(await w.status(reviewing.id)).toEqual({ status: `in_progress`, builtinKey: `in_progress` })

    // A session without a repository takes nothing.
    const chatId = await w.chat()
    await db.transaction((tx) =>
      linkIssues(tx, { sessionId: chatId, issueIds: [idle.id], source: `user`, addedBy: w.owner, addedBySessionId: null })
    )
    expect(await w.status(idle.id)).toMatchObject({ status: `backlog` })

    expect(manualLinkText([`AU-1`, `AU-2`])).toBe(`Link #AU-1 #AU-2`)
    expect(manualLinkText([])).toBeNull()
  })

  it(`a worker filing an issue holds it without taking it; a later link or a person's message takes it`, async () => {
    const w = await world()
    const named = await w.issue(`Named in the brief`)
    const rootId = await w.thread([named.id])
    expect(await w.status(named.id)).toMatchObject({ status: `in_progress` })
    // The worker FILES an issue (issues create with its session): the row
    // lands on it, nothing moves (issue-lifecycle.json `filed`: the N-10
    // rule, In progress only when a session really takes it up).
    const filed = await w.issue(`Filed while working`)
    await db.transaction((tx) =>
      linkIssues(tx, { sessionId: rootId, issueIds: [filed.id], source: `agent`, addedBy: w.owner, addedBySessionId: rootId, filed: true })
    )
    expect(
      await db
        .select({ issueId: sessionIssues.issueId })
        .from(sessionIssues)
        .where(and(eq(sessionIssues.sessionId, rootId), eq(sessionIssues.issueId, filed.id)))
    ).toHaveLength(1)
    expect(await w.status(filed.id)).toMatchObject({ status: `backlog` })
    expect(
      await db
        .select({ id: issueEvents.id })
        .from(issueEvents)
        .where(and(eq(issueEvents.issueId, filed.id), eq(issueEvents.type, `status_changed`)))
    ).toHaveLength(0)
    // A filed row is still the worker's: a person's message into it takes
    // the whole set it holds, the filed issue included.
    await sendSession({ userId: w.owner, input: { sessionId: rootId, text: `now do it` } })
    expect(await w.status(filed.id)).toEqual({ status: `in_progress`, builtinKey: `in_progress` })
    // Filing on an issue already in review moves nothing either.
    const reviewed = await w.issue(`Reviewed`)
    await db.update(issues).set({ status: `in_review` }).where(eq(issues.id, reviewed.id))
    await db.transaction((tx) =>
      linkIssues(tx, { sessionId: rootId, issueIds: [reviewed.id], source: `agent`, addedBy: w.owner, addedBySessionId: rootId, filed: true })
    )
    expect(await w.status(reviewed.id)).toMatchObject({ status: `in_review` })
  })

  it(`NEW-48: the PR the issue's holder opened ranks as assigned, the thread-level PR does not`, async () => {
    const w = await world()
    const named = await w.issue(`Held by the child`)
    const other = await w.issue(`The root's`)
    const rootId = await w.thread([named.id, other.id])
    const child = (await sendSession({ userId: w.owner, input: { parentId: rootId, text: `take it` } })).sessionId
    await db.transaction((tx) =>
      forwardIssues(tx, { sessionId: child, issueIds: [named.id], source: `user`, addedBy: w.owner, addedBySessionId: null })
    )
    const own = await w.openPr({ rootId, sessionId: child, head: `exp/child`, issueIds: [named.id] })
    const thread = await w.openPr({ rootId, head: `exp/root`, issueIds: [named.id, other.id] })
    const ranked = await livePullRequestsForIssue(named.id)
    expect(ranked.find((c) => c.pr.id === own.id)).toMatchObject({ assigned: true, linkedIssues: 1 })
    expect(ranked.find((c) => c.pr.id === thread.id)).toMatchObject({ assigned: false, linkedIssues: 2 })
  })

  it(`a PR closes its session's share of the thread`, async () => {
    const w = await world()
    const named = await w.issue(`Named in the brief`)
    const other = await w.issue(`Another child's`)
    const followUp = await w.issue(`Filed by the grandchild`)
    const rootId = await w.thread([named.id, other.id])
    const a = (await sendSession({ userId: w.owner, input: { parentId: rootId, text: `Fix #${named.identifier}` } }))
      .sessionId
    const a1 = (await sendSession({ userId: w.owner, input: { parentId: a, text: `follow up` } })).sessionId
    await sendSession({ userId: w.owner, input: { parentId: rootId, text: `Do #${other.identifier}` } })
    // NEW-1: the grandchild files it: the row sits on the grandchild.
    await db.transaction((tx) =>
      linkIssues(tx, { sessionId: a1, issueIds: [followUp.id], source: `agent`, addedBy: w.owner, addedBySessionId: a1, filed: true })
    )

    // a's share = its subtree's rows + what its start names.
    expect((await prDefaultIssueIds(db, { rootId, sessionId: a })).sort()).toEqual([named.id, followUp.id].sort())
    // The root's own PR takes the whole thread.
    expect((await prDefaultIssueIds(db, { rootId, sessionId: rootId })).sort()).toEqual(
      [named.id, other.id, followUp.id].sort()
    )
  })

  it(`a stacked PR's issues wait for the stack's bottom to land in the default branch`, async () => {
    const w = await world()
    const bottomIssue = await w.issue(`Bottom`)
    const topIssue = await w.issue(`Top`)
    const rootId = await w.thread([bottomIssue.id, topIssue.id])
    const bottom = await w.openPr({ rootId, head: `exp/bottom`, issueIds: [bottomIssue.id] })
    const top = await w.openPr({ rootId, head: `exp/top`, base: `exp/bottom`, issueIds: [topIssue.id] })
    expect(top.basePullRequestId).toBe(bottom.id)
    expect(await w.status(topIssue.id)).toMatchObject({ status: `in_review` })

    await applyPullRequestMerged({ pullRequestId: top.id, actorUserId: w.owner })
    expect(await w.status(topIssue.id)).toMatchObject({ status: `in_review` })
    const merged = await db
      .select({ id: issueEvents.id })
      .from(issueEvents)
      .where(and(eq(issueEvents.issueId, topIssue.id), eq(issueEvents.type, `pr_merged`)))
    expect(merged).toHaveLength(1)

    await applyPullRequestMerged({ pullRequestId: bottom.id, actorUserId: w.owner })
    expect(await w.status(bottomIssue.id)).toMatchObject({ status: `done`, builtinKey: `done` })
    expect(await w.status(topIssue.id)).toMatchObject({ status: `done`, builtinKey: `done` })
  })

  it(`EXP-1378: part PRs land their issue only once every part merged, in any order`, async () => {
    const w = await world()
    const split = await w.issue(`Split`)
    const rootId = await w.thread([split.id])
    const one = await w.openPr({ rootId, head: `exp/one`, issueIds: [split.id], title: `Split it (1/3)` })
    const two = await w.openPr({ rootId, head: `exp/two`, issueIds: [split.id], title: `Split it (2/3)` })
    const three = await w.openPr({ rootId, head: `exp/three`, issueIds: [split.id], title: `Split it (3/3)` })
    // The last part first: nothing lands.
    await applyPullRequestMerged({ pullRequestId: three.id, actorUserId: w.owner })
    expect(await w.status(split.id)).toMatchObject({ status: `in_review` })
    await applyPullRequestMerged({ pullRequestId: one.id, actorUserId: w.owner })
    expect(await w.status(split.id)).toMatchObject({ status: `in_review` })
    const merged = await db
      .select({ id: issueEvents.id })
      .from(issueEvents)
      .where(and(eq(issueEvents.issueId, split.id), eq(issueEvents.type, `pr_merged`)))
    expect(merged).toHaveLength(2)
    // The missing part lands it.
    await applyPullRequestMerged({ pullRequestId: two.id, actorUserId: w.owner })
    expect(await w.status(split.id)).toMatchObject({ status: `done`, builtinKey: `done` })
  })

  it(`EXP-1378: a part suffix mid-title is a whole PR; a whole PR beside the parts lands its issue`, async () => {
    const w = await world()
    const issue = await w.issue(`Fraction`)
    const rootId = await w.thread([issue.id])
    const pr = await w.openPr({ rootId, head: `exp/fraction`, issueIds: [issue.id], title: `Use (1/2) of the width` })
    await applyPullRequestMerged({ pullRequestId: pr.id, actorUserId: w.owner })
    expect(await w.status(issue.id)).toMatchObject({ status: `done` })
  })

  it(`a merge delivered with its base lands a PR retargeted onto the default branch`, async () => {
    // The `edited` (base change) delivery never arrived: the stored base
    // still names the live bottom PR, but the merge carries `main`.
    const w = await world()
    const bottomIssue = await w.issue(`Bottom`)
    const topIssue = await w.issue(`Top`)
    const rootId = await w.thread([bottomIssue.id, topIssue.id])
    const bottom = await w.openPr({ rootId, head: `exp/bottom`, issueIds: [bottomIssue.id] })
    const top = await w.openPr({ rootId, head: `exp/top`, base: `exp/bottom`, issueIds: [topIssue.id] })
    expect(top.basePullRequestId).toBe(bottom.id)

    await applyPullRequestMerged({ pullRequestId: top.id, base: `main`, actorUserId: w.owner })
    expect(await loadPullRequest(top.id)).toMatchObject({ state: `merged`, base: `main`, basePullRequestId: null })
    expect(await w.status(topIssue.id)).toMatchObject({ status: `done`, builtinKey: `done` })
    // The bottom PR is still open: its issue stays in review.
    expect(await w.status(bottomIssue.id)).toMatchObject({ status: `in_review` })
  })

  it(`a base corrected after the merge releases the held issues, its own and those above`, async () => {
    // `edited` after `closed`: bottom → mid → top, top merged (held by mid),
    // mid merged off its stale base (held by bottom), then mid's base turns
    // out to be `main`: mid and top land, bottom is still open.
    const w = await world()
    const bottomIssue = await w.issue(`Bottom`)
    const midIssue = await w.issue(`Mid`)
    const topIssue = await w.issue(`Top`)
    const rootId = await w.thread([bottomIssue.id, midIssue.id, topIssue.id])
    const bottom = await w.openPr({ rootId, head: `exp/bottom`, issueIds: [bottomIssue.id] })
    const mid = await w.openPr({ rootId, head: `exp/mid`, base: `exp/bottom`, issueIds: [midIssue.id] })
    const top = await w.openPr({ rootId, head: `exp/top`, base: `exp/mid`, issueIds: [topIssue.id] })
    expect(mid.basePullRequestId).toBe(bottom.id)
    expect(top.basePullRequestId).toBe(mid.id)

    await applyPullRequestMerged({ pullRequestId: top.id, actorUserId: w.owner })
    await applyPullRequestMerged({ pullRequestId: mid.id, actorUserId: w.owner })
    expect(await w.status(midIssue.id)).toMatchObject({ status: `in_review` })
    expect(await w.status(topIssue.id)).toMatchObject({ status: `in_review` })

    // A base the row already stores is a no-op; the corrected one lands it.
    await applyPullRequestBase({ pullRequestId: mid.id, base: `exp/bottom` })
    expect(await w.status(midIssue.id)).toMatchObject({ status: `in_review` })
    await applyPullRequestBase({ pullRequestId: mid.id, base: `main` })
    expect(await loadPullRequest(mid.id)).toMatchObject({ base: `main`, basePullRequestId: null })
    expect(await w.status(midIssue.id)).toMatchObject({ status: `done`, builtinKey: `done` })
    expect(await w.status(topIssue.id)).toMatchObject({ status: `done`, builtinKey: `done` })
    expect(await w.status(bottomIssue.id)).toMatchObject({ status: `in_review` })
    // Idempotent: a repeat of the correction re-fires nothing.
    await applyPullRequestBase({ pullRequestId: mid.id, base: `main` })
    const moves = await db
      .select({ id: issueEvents.id })
      .from(issueEvents)
      .where(and(eq(issueEvents.issueId, midIssue.id), eq(issueEvents.type, `status_changed`)))
    // started (the thread link), opened (its PR), merged (the correction).
    expect(moves).toHaveLength(3)
  })

  it(`a parent waits for its linked sub-issues and follows the last one`, async () => {
    const w = await world()
    const parent = await w.issue(`Parent`)
    const child = await w.issue(`Linked child`)
    const stray = await w.issue(`Unlinked child`)
    await w.parentOf(parent.id, child.id)
    await w.parentOf(parent.id, stray.id)
    const rootId = await w.thread([parent.id, child.id])
    expect(await w.status(child.id)).toMatchObject({ status: `in_progress` })

    // The parent's PR opens while its linked sub-issue is still in progress.
    const parentPr = await w.openPr({ rootId, head: `exp/parent`, issueIds: [parent.id] })
    expect(await w.status(parent.id)).toMatchObject({ status: `in_progress` })
    // The sub-issue's PR opens: the parent follows it into review; the
    // unlinked sub-issue never moves and never holds the parent back.
    const childPr = await w.openPr({ rootId, head: `exp/child`, issueIds: [child.id] })
    expect(await w.status(child.id)).toMatchObject({ status: `in_review` })
    expect(await w.status(parent.id)).toMatchObject({ status: `in_review` })
    expect(await w.status(stray.id)).toMatchObject({ status: `backlog` })

    await applyPullRequestMerged({ pullRequestId: parentPr.id, actorUserId: w.owner })
    expect(await w.status(parent.id)).toMatchObject({ status: `in_review` })
    await applyPullRequestMerged({ pullRequestId: childPr.id, actorUserId: w.owner })
    expect(await w.status(child.id)).toMatchObject({ status: `done` })
    expect(await w.status(parent.id)).toMatchObject({ status: `done` })
    expect(await w.status(stray.id)).toMatchObject({ status: `backlog` })
  })

  it(`one PR closing a parent and its sub-issue moves both together`, async () => {
    const w = await world()
    const parent = await w.issue(`Parent`)
    const child = await w.issue(`Child`)
    await w.parentOf(parent.id, child.id)
    const rootId = await w.thread([parent.id, child.id])
    const pr = await w.openPr({ rootId, head: `exp/both`, issueIds: [parent.id, child.id] })
    expect(await w.status(parent.id)).toMatchObject({ status: `in_review` })
    expect(await w.status(child.id)).toMatchObject({ status: `in_review` })
    await applyPullRequestMerged({ pullRequestId: pr.id, actorUserId: w.owner })
    expect(await w.status(parent.id)).toMatchObject({ status: `done` })
    expect(await w.status(child.id)).toMatchObject({ status: `done` })
  })
})

// NEW-1 (issue-lifecycle.json): the session side of the lifecycle. A row
// landing on a worker TAKES the issue (backlog / in review → In progress),
// so does a PERSON's message into a worker holding it; the last live worker
// dropping it with no live PR sends it back to backlog. Nothing settles back
// when work stops (EXP-1350's follow-up machinery is gone).
describe.skipIf(!ready.ok)(`NEW-1 issue lifecycle`, () => {
  const child = async (
    w: Awaited<ReturnType<typeof world>>,
    parentId: string,
    opts: { text?: string; issueIds?: string[] } = {}
  ) =>
    (await sendSession({ userId: w.owner, input: { parentId, text: opts.text ?? `follow up`, issueIds: opts.issueIds } }))
      .sessionId
  const end = (w: Awaited<ReturnType<typeof world>>, sessionId: string) =>
    endSession({ userId: w.owner, sessionId, summary: `done` })
  const reviewing = async (w: Awaited<ReturnType<typeof world>>) => {
    const issue = await w.issue(`Follow-up target`)
    const rootId = await w.thread([issue.id])
    const pr = await w.openPr({ rootId, head: `exp/${issue.identifier}`, issueIds: [issue.id] })
    expect(await w.status(issue.id)).toMatchObject({ status: `in_review` })
    return { issue, rootId, pr }
  }
  const statusEvents = async (issueId: string) =>
    (
      await db
        .select({ payload: issueEvents.payload })
        .from(issueEvents)
        .where(and(eq(issueEvents.issueId, issueId), eq(issueEvents.type, `status_changed`)))
    ).map((row) => row.payload)

  it(`a child started on an issue in review takes it back to In progress; its end moves nothing`, async () => {
    const w = await world()
    const { issue, rootId } = await reviewing(w)
    // A child that names no issue takes nothing.
    const bare = await child(w, rootId)
    expect(await w.status(issue.id)).toMatchObject({ status: `in_review` })
    const a = await child(w, rootId, { issueIds: [issue.id] })
    expect(await w.status(issue.id)).toEqual({ status: `in_progress`, builtinKey: `in_progress` })
    expect(await statusEvents(issue.id)).toContainEqual(
      expect.objectContaining({ automation: true, event: `taken`, sessionId: a, toName: `In Progress` })
    )
    // No settle-back: the work stopping leaves it where it is.
    await end(w, a)
    await end(w, bare)
    expect(await w.status(issue.id)).toMatchObject({ status: `in_progress` })
  })

  it(`a person's message into the worker holding it retakes it; a session's message does not`, async () => {
    const w = await world()
    const { issue, rootId } = await reviewing(w)
    const other = await w.chat(`a sibling thread`)
    // Another session's message (its MCP key) is bus input: nothing moves.
    await sendSession({
      userId: w.owner,
      input: { sessionId: rootId, text: `status?` },
      callerSessionId: other,
      viaMcp: true,
    })
    expect(await w.status(issue.id)).toMatchObject({ status: `in_review` })
    // A message into a session that does NOT hold it takes nothing.
    const bare = await child(w, rootId)
    await sendSession({ userId: w.owner, input: { sessionId: bare, text: `hi` } })
    expect(await w.status(issue.id)).toMatchObject({ status: `in_review` })
    // A person's message into the holder: the worker takes it again.
    await sendSession({ userId: w.owner, input: { sessionId: rootId, text: `one more thing` } })
    expect(await w.status(issue.id)).toEqual({ status: `in_progress`, builtinKey: `in_progress` })
  })

  it(`a forward and a continuation take the issue onto the new holder`, async () => {
    const w = await world()
    const { issue, rootId } = await reviewing(w)
    const a = await child(w, rootId)
    await sendSession({ userId: w.owner, input: { sessionId: a, issueIds: [issue.id] } })
    expect(await db.select({ sessionId: sessionIssues.sessionId }).from(sessionIssues).where(eq(sessionIssues.issueId, issue.id))).toEqual([
      { sessionId: a },
    ])
    expect(await w.status(issue.id)).toMatchObject({ status: `in_progress` })

    // Back in review (a second PR's open), then a's continuation takes it over.
    await db.update(issues).set({ status: `in_review`, statusId: null }).where(eq(issues.id, issue.id))
    await end(w, a)
    const cont = (await sendSession({ userId: w.owner, input: { sessionId: a, text: `again` } })).sessionId
    expect(await db.select({ sessionId: sessionIssues.sessionId }).from(sessionIssues).where(eq(sessionIssues.issueId, issue.id))).toEqual([
      { sessionId: cont },
    ])
    expect(await w.status(issue.id)).toEqual({ status: `in_progress`, builtinKey: `in_progress` })

    // forwardIssues of an issue the target already holds retakes it too.
    await db.update(issues).set({ status: `in_review`, statusId: null }).where(eq(issues.id, issue.id))
    await db.transaction((tx) =>
      forwardIssues(tx, { sessionId: cont, issueIds: [issue.id], source: `user`, addedBy: w.owner, addedBySessionId: null })
    )
    expect(await w.status(issue.id)).toMatchObject({ status: `in_progress` })
  })

  it(`unlinking from the last live worker with no live PR sends it back to backlog`, async () => {
    const w = await world()
    const issue = await w.issue(`Dropped`)
    const rootId = await w.thread([issue.id])
    const a = await child(w, rootId, { issueIds: [issue.id] })
    expect(await w.status(issue.id)).toMatchObject({ status: `in_progress` })
    const unlink = (sessionId: string) =>
      db.transaction((tx) => unlinkIssue(tx, { sessionId, issueId: issue.id, removedBy: w.owner }))
    // Another worker still holds it: nothing moves.
    await unlink(a)
    expect(await w.status(issue.id)).toMatchObject({ status: `in_progress` })
    // The last one: backlog.
    await unlink(rootId)
    expect(await w.status(issue.id)).toEqual({ status: `backlog`, builtinKey: `backlog` })
    expect(await statusEvents(issue.id)).toHaveLength(2)

    // With a live PR it stays: in progress (retaken after the PR opened).
    const kept = await w.issue(`Kept`)
    const keptRoot = await w.thread([kept.id])
    await w.openPr({ rootId: keptRoot, head: `exp/${kept.identifier}`, issueIds: [kept.id] })
    await sendSession({ userId: w.owner, input: { sessionId: keptRoot, text: `fix the review` } })
    expect(await w.status(kept.id)).toMatchObject({ status: `in_progress` })
    await db.transaction((tx) => unlinkIssue(tx, { sessionId: keptRoot, issueId: kept.id, removedBy: w.owner }))
    expect(await w.status(kept.id)).toMatchObject({ status: `in_progress` })

    // An ended worker no longer counts as holding it.
    const last = await w.issue(`Last`)
    const lastRoot = await w.thread([last.id])
    const lastChild = await child(w, lastRoot, { issueIds: [last.id] })
    await end(w, lastChild)
    await db.transaction((tx) => unlinkIssue(tx, { sessionId: lastRoot, issueId: last.id, removedBy: w.owner }))
    // lastRoot's own row went; the ended child's row remains but is no worker.
    expect(await w.status(last.id)).toMatchObject({ status: `backlog` })
  })

  it(`taken targets the builtins: a team without PR-open automation still starts its work`, async () => {
    const w = await world()
    const { issue, rootId } = await reviewing(w)
    await db.update(teams).set({ prOpenedAutomation: false }).where(eq(teams.id, w.teamId))
    await child(w, rootId, { issueIds: [issue.id] })
    expect(await w.status(issue.id)).toEqual({ status: `in_progress`, builtinKey: `in_progress` })
  })
})
