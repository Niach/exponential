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
  teamMembers,
  teams,
} from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// EXP-1334: the status automation against the REAL migrated Postgres — a
// thread link starts its issues, a PR closes its session's share of the
// thread, a stacked PR's issues wait for the stack's bottom to land, and a
// parent issue waits for its linked sub-issues. Each case builds a fresh
// team; the suite deletes every team (and user) it made (cascade).

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
      WHERE table_name = 'sessions' AND column_name = 'dispatch')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
  ) AS ok
`)
if (!ready.ok) console.info(`[issue-automation.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { linkIssues, manualLinkText } = await import(`@/lib/sessions/issues`)
const { prDefaultIssueIds } = await import(`@/lib/sessions/pr-issues`)
const { endSession, settleFollowUpIssuesInTx } = await import(`@/lib/sessions/lifecycle`)
const {
  applyPullRequestBase,
  applyPullRequestClosed,
  applyPullRequestMerged,
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
  const [board] = await db
    .insert(boards)
    .values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `AU` })
    .returning({ id: boards.id })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const fullName = `o/r-${id8()}`
  const [repo] = await db
    .insert(repositories)
    .values({ teamId, fullName, defaultBranch: `main` })
    .returning({ id: repositories.id })
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
  const openPr = async (opts: { rootId: string; head: string; base?: string; issueIds: string[] }) => {
    prNumber += 1
    return (
      await recordPullRequestOpened({
        teamId,
        rootSessionId: opts.rootId,
        sessionId: opts.rootId,
        repositoryId: repo!.id,
        repoFullName: fullName,
        number: prNumber,
        url: `https://github.com/${fullName}/pull/${prNumber}`,
        title: `PR ${prNumber}`,
        head: opts.head,
        base: opts.base ?? `main`,
        issueIds: opts.issueIds,
        actorUserId: owner,
      })
    ).pullRequest
  }
  return { owner, teamId, deviceId, issue, parentOf, status, thread, openPr }
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
  it(`a thread link starts a not-yet-started issue and leaves a started one alone`, async () => {
    const w = await world()
    const fresh = await w.issue(`Fresh`)
    const reviewing = await w.issue(`Reviewing`)
    await db.update(issues).set({ status: `in_review` }).where(eq(issues.id, reviewing.id))

    // The quiet start link starts it too.
    const rootId = await w.thread([fresh.id])
    expect(await w.status(fresh.id)).toEqual({ status: `in_progress`, builtinKey: `in_progress` })
    const [event] = await db
      .select({ payload: issueEvents.payload, actor: issueEvents.actorUserId })
      .from(issueEvents)
      .where(and(eq(issueEvents.issueId, fresh.id), eq(issueEvents.type, `status_changed`)))
    expect(event).toMatchObject({ actor: w.owner, payload: { automation: true, toName: `In Progress` } })

    await db.transaction((tx) =>
      linkIssues(tx, { rootId, issueIds: [reviewing.id], source: `agent`, addedBy: w.owner, addedBySessionId: rootId })
    )
    expect(await w.status(reviewing.id)).toMatchObject({ status: `in_review` })
    expect(manualLinkText([`AU-1`, `AU-2`])).toBe(`Link #AU-1 #AU-2`)
    expect(manualLinkText([])).toBeNull()
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
    await db.transaction((tx) =>
      linkIssues(tx, { rootId, issueIds: [followUp.id], source: `agent`, addedBy: w.owner, addedBySessionId: a1 })
    )

    expect(await prDefaultIssueIds(db, { rootId, sessionId: a })).toEqual([named.id, followUp.id])
    expect(await prDefaultIssueIds(db, { rootId, sessionId: rootId })).toEqual([named.id, other.id, followUp.id])
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

describe.skipIf(!ready.ok)(`EXP-1350 follow-up work`, () => {
  /** A person sets the status by hand (issues.update writes no flag). */
  const setByHand = async (issueId: string, status: `in_review` | `in_progress` | `backlog`) => {
    await db.update(issues).set({ status, statusId: null }).where(eq(issues.id, issueId))
    await db.execute(sql`
      INSERT INTO issue_events (issue_id, team_id, board_id, type, payload, created_at, updated_at)
      SELECT id, team_id, board_id, 'status_changed', '{"fromName": null, "toName": null}'::jsonb,
        clock_timestamp(), clock_timestamp()
      FROM issues WHERE id = ${issueId}
    `)
  }
  const child = async (w: Awaited<ReturnType<typeof world>>, parentId: string, text = `follow up`) =>
    (await sendSession({ userId: w.owner, input: { parentId, text } })).sessionId
  const end = (w: Awaited<ReturnType<typeof world>>, sessionId: string) =>
    endSession({ userId: w.owner, sessionId, summary: `done` })
  const reviewing = async (w: Awaited<ReturnType<typeof world>>) => {
    const issue = await w.issue(`Follow-up target`)
    const rootId = await w.thread([issue.id])
    const pr = await w.openPr({ rootId, head: `exp/${issue.identifier}`, issueIds: [issue.id] })
    expect(await w.status(issue.id)).toMatchObject({ status: `in_review` })
    return { issue, rootId, pr }
  }

  it(`a child moves In Review back to In Progress and its end with the PR open returns it`, async () => {
    const w = await world()
    const { issue, rootId } = await reviewing(w)
    const a = await child(w, rootId)
    expect(await w.status(issue.id)).toEqual({ status: `in_progress`, builtinKey: `in_progress` })
    const events = await db
      .select({ payload: issueEvents.payload })
      .from(issueEvents)
      .where(and(eq(issueEvents.issueId, issue.id), eq(issueEvents.type, `status_changed`)))
    expect(events.map((row) => row.payload)).toContainEqual(
      expect.objectContaining({ automation: true, event: `resumed`, rootSessionId: rootId, toName: `In Progress` })
    )
    await end(w, a)
    expect(await w.status(issue.id)).toEqual({ status: `in_review`, builtinKey: `in_review` })
  })

  it(`a hand-set In Review stays when follow-up work starts`, async () => {
    const w = await world()
    const { issue, rootId } = await reviewing(w)
    await setByHand(issue.id, `in_review`)
    await child(w, rootId)
    expect(await w.status(issue.id)).toMatchObject({ status: `in_review` })
  })

  it(`a person's move while the child works wins over its end`, async () => {
    const w = await world()
    const { issue, rootId } = await reviewing(w)
    const a = await child(w, rootId)
    expect(await w.status(issue.id)).toMatchObject({ status: `in_progress` })
    await setByHand(issue.id, `in_progress`)
    await end(w, a)
    expect(await w.status(issue.id)).toMatchObject({ status: `in_progress` })
  })

  it(`only the last working child's end returns it to In Review`, async () => {
    const w = await world()
    const { issue, rootId } = await reviewing(w)
    const a = await child(w, rootId, `one`)
    const b = await child(w, rootId, `two`)
    await end(w, a)
    expect(await w.status(issue.id)).toMatchObject({ status: `in_progress` })
    await end(w, b)
    expect(await w.status(issue.id)).toMatchObject({ status: `in_review` })
  })

  it(`no PR left live: the end leaves it In Progress`, async () => {
    const w = await world()
    const { issue, rootId, pr } = await reviewing(w)
    const a = await child(w, rootId)
    await applyPullRequestClosed({ pullRequestId: pr.id })
    await end(w, a)
    expect(await w.status(issue.id)).toMatchObject({ status: `in_progress` })
  })

  it(`a dispatcher message never moves it; a message into an idle work child does, its turn end returns it`, async () => {
    const w = await world()
    const { issue, rootId } = await reviewing(w)
    await sendSession({ userId: w.owner, input: { sessionId: rootId, text: `status?` } })
    expect(await w.status(issue.id)).toMatchObject({ status: `in_review` })

    const a = await child(w, rootId)
    // The device ran the child: its turn is over (idle, message read).
    const settle = async () => {
      await db.execute(sql`UPDATE messages SET consumed_at = now() WHERE session_id = ${a}`)
      await db.execute(sql`UPDATE sessions SET status = 'idle', busy = false WHERE id = ${a}`)
      const [row] = await db.execute(sql`SELECT root_id FROM sessions WHERE id = ${a}`).then((r) => r.rows)
      await db.transaction((tx) =>
        settleFollowUpIssuesInTx(tx, { rootId: (row as { root_id: string }).root_id }, [a], w.owner)
      )
    }
    await settle()
    expect(await w.status(issue.id)).toMatchObject({ status: `in_review` })

    await sendSession({ userId: w.owner, input: { sessionId: a, text: `one more thing` } })
    expect(await w.status(issue.id)).toMatchObject({ status: `in_progress` })
    await settle()
    expect(await w.status(issue.id)).toMatchObject({ status: `in_review` })
  })

  it(`a team without PR-open automation never moves it back`, async () => {
    const w = await world()
    const { issue, rootId } = await reviewing(w)
    await db.update(teams).set({ prOpenedAutomation: false }).where(eq(teams.id, w.teamId))
    await child(w, rootId)
    expect(await w.status(issue.id)).toMatchObject({ status: `in_review` })
  })
})
