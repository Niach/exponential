// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  actions,
  attachments,
  boards,
  devices,
  issues,
  messages,
  sessionIssues,
  sessions,
  teamMembers,
  teams,
} from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// EXP-1291: the session tree's core against the REAL migrated Postgres —
// sessions.send's three forms, visibility, the tree caps, lifecycle, the
// Guide, the brief and the custom-trigger mirrors. These helpers open their
// own transactions, so each case builds a fresh team and the suite deletes
// every team (and user) it made (cascade). Skips without a migrated DB that
// has the EXP-1291 triggers (see src/test/real-db.ts).

vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  notifySessionBlocked: async () => {},
  // EXP-1328: File as issue runs issues.create.
  fireAndForgetAssignmentNotify: () => {},
  fireAndForgetIssueMentionNotify: () => {},
}))

const ready = await probeRealDb(`
  SELECT (
    EXISTS (SELECT 1 FROM information_schema.columns
      WHERE table_name = 'sessions' AND column_name = 'root_id')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'propagate_issue_board_to_session_children')
  ) AS ok
`)
if (!ready.ok) console.info(`[sessions.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { loadSessionForUser } = await import(`@/lib/sessions/access`)
const { endSession } = await import(`@/lib/sessions/lifecycle`)
const { linkIssues, unlinkIssue } = await import(`@/lib/sessions/issues`)
const { writeGuide } = await import(`@/lib/sessions/guide`)
const { composeSessionBrief } = await import(`@/lib/sessions/brief`)
const { getSessionTree, listSessions } = await import(`@/lib/sessions/tree`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `sess-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@sessions.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function world(opts: { shared?: boolean } = {}) {
  const owner = await user()
  const mate = await user()
  const [team] = await db
    .insert(teams)
    .values({ name: `Sessions`, slug: `sessions-${id8()}` })
    .returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner, role: `owner` },
    { teamId, userId: mate, role: `member` },
  ])
  const [board] = await db
    .insert(boards)
    .values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `CR` })
    .returning({ id: boards.id })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({
    userId: owner,
    deviceId,
    label: `box`,
    kind: `server`,
    sharedTeamIds: opts.shared ? [teamId] : [],
  })
  const issue = async (title = `An issue`, boardId = board!.id) =>
    (
      await db
        .insert(issues)
        .values({ boardId, teamId, title, creatorId: owner })
        .returning({ id: issues.id })
    )[0]!.id
  return { owner, mate, teamId, boardId: board!.id, deviceId, issue }
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

const row = async (id: string) =>
  (await db.select().from(sessions).where(eq(sessions.id, id)).limit(1))[0]!

describe.skipIf(!ready.ok)(`sessions.send — a new root`, () => {
  it(`queues a private thread on the caller's own device with its issues and first message`, async () => {
    const w = await world()
    const issueId = await w.issue(`Fix login`)
    const sent = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, issueIds: [issueId], text: `go @${w.mate}@sessions.test` },
    })
    expect(sent.txid).toBeTypeOf(`number`)
    // FEED-101: the start returns at once as PENDING, never waits on the device.
    expect(sent.status).toBe(`queued`)
    const root = await row(sent.sessionId)
    expect(root).toMatchObject({
      kind: `thread`,
      status: `queued`,
      rootId: root.id,
      depth: 0,
      shared: false,
      hostUserId: null,
      cli: null,
    })
    const links = await db.select().from(sessionIssues).where(eq(sessionIssues.sessionId, root.id))
    expect(links).toMatchObject([{ issueId, ownerId: w.owner, shared: false, boardId: w.boardId }])
    const [message] = await db.select().from(messages).where(eq(messages.id, sent.messageId!))
    expect(message).toMatchObject({ rootId: root.id, teamId: w.teamId, ownerId: w.owner, authorKind: `user` })
    // Start facets post no event cards (the brief names them).
    const cards = await db.select().from(messages).where(eq(messages.rootId, root.id))
    expect(cards).toHaveLength(1)

    // Private: the teammate cannot see it.
    await expect(loadSessionForUser(w.mate, root.id)).rejects.toMatchObject({ code: `NOT_FOUND` })
    expect(await listSessions(w.mate, { kind: `thread`, limit: 10, offset: 0 })).toEqual([])
    const [listed] = await listSessions(w.owner, { teamId: w.teamId, kind: `thread`, limit: 10, offset: 0 })
    expect(listed).toMatchObject({ id: root.id, title: `Fix login` })
  })

  it(`stamps shared from the device and lets a teammate start on the shared box`, async () => {
    const w = await world({ shared: true })
    const sent = await sendSession({
      userId: w.mate,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `hello` },
    })
    const root = await row(sent.sessionId)
    expect(root).toMatchObject({ shared: true, userId: w.mate, hostUserId: w.owner })
    // Visible to the owner (member + shared).
    expect((await loadSessionForUser(w.owner, root.id)).id).toBe(root.id)
  })

  it(`refuses a device that is neither yours nor shared, and an unknown model`, async () => {
    const w = await world()
    await expect(
      sendSession({ userId: w.mate, input: { teamId: w.teamId, deviceId: w.deviceId, text: `x` } })
    ).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
    await expect(
      sendSession({
        userId: w.owner,
        input: { teamId: w.teamId, deviceId: w.deviceId, text: `x`, model: `gpt-5.6-sol` },
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`claims uploads onto the message and embeds them`, async () => {
    const w = await world()
    const [upload] = await db
      .insert(attachments)
      .values({
        teamId: w.teamId,
        uploaderId: w.owner,
        filename: `shot.png`,
        contentType: `image/png`,
        sizeBytes: 3,
        storageKey: `k-${id8()}`,
        url: `/api/attachments/x`,
      })
      .returning({ id: attachments.id })
    const sent = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `look`, attachmentIds: [upload!.id] },
    })
    const [att] = await db.select().from(attachments).where(eq(attachments.id, upload!.id))
    expect(att!.messageId).toBe(sent.messageId)
    const [message] = await db.select().from(messages).where(eq(messages.id, sent.messageId!))
    expect(message!.body).toBe(`look\n\n![shot.png](/api/attachments/${upload!.id})`)
    // A second claim of the same upload is refused.
    await expect(
      sendSession({
        userId: w.owner,
        input: { sessionId: sent.sessionId, attachmentIds: [upload!.id] },
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`resolves an action's inputs and snapshots its name`, async () => {
    const w = await world()
    const issueId = await w.issue(`Target`)
    const [action] = await db
      .insert(actions)
      .values({
        teamId: w.teamId,
        name: `Triage`,
        body: `Triage the issue.`,
        inputs: [{ key: `issue`, label: `Issue`, type: `issue`, required: true }],
      })
      .returning({ id: actions.id })
    await expect(
      sendSession({
        userId: w.owner,
        input: { teamId: w.teamId, deviceId: w.deviceId, actionId: action!.id, inputs: {} },
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    const sent = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, actionId: action!.id, inputs: { issue: issueId } },
    })
    expect(await row(sent.sessionId)).toMatchObject({
      actionName: `Triage`,
      actionInputs: { issue: issueId },
    })
    const brief = await composeSessionBrief(sent.sessionId)
    expect(brief.brief).toContain(`Triage the issue.`)
    expect(brief.brief).toContain(`Target`)
  })
})

// Dispatch (owner 2026-10-15): every new root is a DISPATCHER on the device's
// thread model; the composer's picks ride `work_launch` to its children.
describe.skipIf(!ready.ok)(`sessions.send — dispatcher roots`, () => {
  it(`a new root dispatches on no model of its own and hands its picks to children`, async () => {
    const w = await world()
    const root = await sendSession({
      userId: w.owner,
      input: {
        teamId: w.teamId,
        deviceId: w.deviceId,
        cli: `claude`,
        model: `opus`,
        effort: `high`,
        planMode: true,
        ultracode: true,
        computerUse: true,
        text: `build it`,
      },
    })
    expect(await row(root.sessionId)).toMatchObject({
      dispatch: true,
      cli: `claude`,
      model: null,
      effort: null,
      planMode: false,
      ultracode: false,
      computerUse: false,
      workLaunch: {
        model: `opus`,
        effort: `high`,
        planMode: true,
        ultracode: true,
        computerUse: true,
      },
    })

    // A child inherits the WORK picks, never the dispatcher's own run, and
    // is no dispatcher itself.
    const child = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `part 1` } })
    expect(await row(child.sessionId)).toMatchObject({
      dispatch: false,
      cli: `claude`,
      model: `opus`,
      effort: `high`,
      planMode: true,
      ultracode: true,
      computerUse: true,
    })
    // The child's own picks win over the inherited ones.
    const picked = await sendSession({
      userId: w.owner,
      input: { parentId: root.sessionId, text: `part 2`, model: `haiku`, planMode: false },
    })
    expect(await row(picked.sessionId)).toMatchObject({ model: `haiku`, planMode: false, effort: `high` })

    // An ended dispatcher's continuation dispatches on.
    await endSession({ userId: w.owner, sessionId: root.sessionId, summary: `Done.` })
    const cont = await sendSession({ userId: w.owner, input: { sessionId: root.sessionId, text: `again` } })
    expect(await row(cont.sessionId)).toMatchObject({
      parentId: root.sessionId,
      dispatch: true,
      model: null,
      workLaunch: { model: `opus`, effort: `high`, planMode: true },
    })
  })
})

describe.skipIf(!ready.ok)(`sessions.send — children, messages, continuation`, () => {
  it(`starts a child under the parent with a session card, inheriting visibility`, async () => {
    const w = await world({ shared: true })
    const root = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `root` },
    })
    const child = await sendSession({ userId: w.mate, input: { parentId: root.sessionId, text: `sub` } })
    expect(await row(child.sessionId)).toMatchObject({
      parentId: root.sessionId,
      rootId: root.sessionId,
      depth: 1,
      shared: true,
      deviceId: w.deviceId,
    })
    const cards = await db
      .select({ card: messages.card })
      .from(messages)
      .where(and(eq(messages.sessionId, root.sessionId), eq(messages.authorKind, `system`)))
    expect(cards).toEqual([{ card: { kind: `session`, sessionId: child.sessionId } }])

    const brief = await composeSessionBrief(child.sessionId)
    expect(brief.brief).toContain(`root`)
    expect(brief.parentBranch).toBeNull()

    // Depth cap: 3 below the root.
    const d2 = await sendSession({ userId: w.owner, input: { parentId: child.sessionId, text: `d2` } })
    const d3 = await sendSession({ userId: w.owner, input: { parentId: d2.sessionId, text: `d3` } })
    await expect(
      sendSession({ userId: w.owner, input: { parentId: d3.sessionId, text: `d4` } })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })

    const tree = await getSessionTree(w.mate, d3.sessionId, { messages: 50 })
    expect(tree.root.id).toBe(root.sessionId)
    expect(tree.children.map((s) => s.id)).toEqual([child.sessionId, d2.sessionId, d3.sessionId])
    expect(`instructions` in tree.root).toBe(false)
  })

  it(`a message to a live session lands on it; to an ended thread it continues in a new child`, async () => {
    const w = await world()
    const root = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `first` },
    })
    const again = await sendSession({ userId: w.owner, input: { sessionId: root.sessionId, text: `more` } })
    expect(again.sessionId).toBe(root.sessionId)

    const ended = await endSession({ userId: w.owner, sessionId: root.sessionId, summary: `Done.` })
    expect(ended.endedIds).toEqual([root.sessionId])
    expect(await row(root.sessionId)).toMatchObject({ status: `ended`, busy: false })

    const cont = await sendSession({ userId: w.owner, input: { sessionId: root.sessionId, text: `one more` } })
    expect(cont.sessionId).not.toBe(root.sessionId)
    expect(await row(cont.sessionId)).toMatchObject({ parentId: root.sessionId, depth: 1, status: `queued` })
    const [onChild] = await db.select().from(messages).where(eq(messages.id, cont.messageId!))
    expect(onChild).toMatchObject({ sessionId: cont.sessionId, rootId: root.sessionId, body: `one more` })
  })

  it(`an account on a message switches the live session's login; alone it posts no message`, async () => {
    const w = await world()
    const root = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, cli: `claude`, account: `aaaa1111`, text: `first` },
    })
    const before = await db.select().from(messages).where(eq(messages.sessionId, root.sessionId))

    // Account-only: the row's login moves (the device acts on it), no bubble.
    const alone = await sendSession({ userId: w.owner, input: { sessionId: root.sessionId, account: `bbbb2222` } })
    expect(alone).toMatchObject({ sessionId: root.sessionId, messageId: null })
    expect(alone.txid).toBeTypeOf(`number`)
    expect(await row(root.sessionId)).toMatchObject({ account: `bbbb2222`, status: `queued` })
    const after = await db.select().from(messages).where(eq(messages.sessionId, root.sessionId))
    expect(after).toHaveLength(before.length)

    // With text: the switch and the message land together.
    const both = await sendSession({
      userId: w.owner,
      input: { sessionId: root.sessionId, account: `cccc3333`, text: `carry on` },
    })
    expect(both.messageId).not.toBeNull()
    expect(await row(root.sessionId)).toMatchObject({ account: `cccc3333` })

    // Codex keeps one login per session.
    const codex = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, cli: `codex`, text: `codex` },
    })
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: codex.sessionId, account: `dddd4444` } })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })

    // A session with NO cli runs the device's default agent: a codex
    // default refuses the switch too; an unknowable one is left to the device.
    const defaulted = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `default agent` },
    })
    expect((await row(defaulted.sessionId)).cli).toBeNull()
    await sendSession({ userId: w.owner, input: { sessionId: defaulted.sessionId, account: `ffff6666` } })
    expect((await row(defaulted.sessionId)).account).toBe(`ffff6666`)
    await db
      .update(devices)
      .set({ launchDefaults: { defaultAgent: `codex` } })
      .where(eq(devices.deviceId, w.deviceId))
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: defaulted.sessionId, account: `gggg7777` } })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })

    // An ended thread: a switch alone is refused; with a message the
    // continuation child runs on the new login.
    await endSession({ userId: w.owner, sessionId: root.sessionId, summary: `Done.` })
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: root.sessionId, account: `eeee5555` } })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    const cont = await sendSession({
      userId: w.owner,
      input: { sessionId: root.sessionId, account: `eeee5555`, text: `again` },
    })
    expect(cont.sessionId).not.toBe(root.sessionId)
    expect(await row(cont.sessionId)).toMatchObject({ account: `eeee5555`, parentId: root.sessionId })
    expect(await row(root.sessionId)).toMatchObject({ account: `cccc3333`, status: `ended` })
  })

  it(`ending a root ends its whole tree; an agent refuses End`, async () => {
    const w = await world()
    const root = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `r` } })
    const child = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `c` } })
    const ended = await endSession({ userId: w.owner, sessionId: root.sessionId })
    expect(new Set(ended.endedIds)).toEqual(new Set([root.sessionId, child.sessionId]))

    const agent = await sendSession({
      userId: w.owner,
      input: {
        teamId: w.teamId,
        deviceId: w.deviceId,
        agent: { name: `Ada`, icon: `rocket`, color: `#ff0000`, instructions: `Be brief.`, triggers: [] },
      },
    })
    expect(await row(agent.sessionId)).toMatchObject({ kind: `agent`, name: `Ada`, instructions: `Be brief.` })
    await expect(endSession({ userId: w.owner, sessionId: agent.sessionId })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
    })
    // A member may not create an agent.
    await expect(
      sendSession({
        userId: w.mate,
        input: {
          teamId: w.teamId,
          deviceId: w.deviceId,
          agent: { name: `Bo`, icon: `rocket`, color: `#00ff00`, instructions: ``, triggers: [] },
        },
      })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })

    // The agent spawns a thread: it lands in the team, on the agent's device,
    // with a thread_started card on the agent.
    const spawned = await sendSession({
      userId: w.owner,
      callerSessionId: agent.sessionId,
      input: { text: `look into it` },
    })
    expect(await row(spawned.sessionId)).toMatchObject({
      teamId: w.teamId,
      deviceId: w.deviceId,
      spawnedBySessionId: agent.sessionId,
    })
    const [card] = await db
      .select({ card: messages.card })
      .from(messages)
      .where(eq(messages.sessionId, agent.sessionId))
    expect(card!.card).toEqual({ kind: `thread_started`, sessionId: spawned.sessionId })
    const brief = await composeSessionBrief(agent.sessionId)
    expect(brief.agentInstructions).toBe(`Be brief.`)
  })
})

describe.skipIf(!ready.ok)(`thread issues, the Guide and the trigger mirrors`, () => {
  it(`links idempotently on the root with event cards, and unlinks`, async () => {
    const w = await world()
    const a = await w.issue(`A`)
    const root = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `r` } })
    const child = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `c` } })
    await db.transaction(async (tx) => {
      const first = await linkIssues(tx, {
        rootId: child.sessionId,
        issueIds: [a],
        source: `agent`,
        addedBy: w.owner,
        addedBySessionId: child.sessionId,
        filed: true,
      })
      const second = await linkIssues(tx, {
        rootId: root.sessionId,
        issueIds: [a],
        source: `user`,
        addedBy: w.owner,
        addedBySessionId: null,
      })
      expect(first.linked).toEqual([a])
      expect(second.linked).toEqual([])
    })
    const links = await db.select().from(sessionIssues).where(eq(sessionIssues.issueId, a))
    expect(links).toMatchObject([{ sessionId: root.sessionId, source: `agent` }])
    await db.transaction((tx) => unlinkIssue(tx, { rootId: root.sessionId, issueId: a }))
    const events = await db
      .select({ card: messages.card })
      .from(messages)
      .where(and(eq(messages.sessionId, root.sessionId), eq(messages.authorKind, `system`)))
    expect(events.map((e) => (e.card as { event?: string }).event).filter(Boolean)).toEqual([
      `issue_filed`,
      `issue_unlinked`,
    ])
  })

  it(`board trash/archive and an issue's move flow onto session_issues`, async () => {
    const w = await world()
    const a = await w.issue(`Moves`)
    const root = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, issueIds: [a] },
    })
    const link = async () =>
      (await db.select().from(sessionIssues).where(eq(sessionIssues.sessionId, root.sessionId)))[0]!
    await db.update(boards).set({ archivedAt: new Date() }).where(eq(boards.id, w.boardId))
    expect((await link()).boardArchivedAt).not.toBeNull()
    await db.update(boards).set({ archivedAt: null }).where(eq(boards.id, w.boardId))
    expect((await link()).boardArchivedAt).toBeNull()

    const [other] = await db
      .insert(boards)
      .values({ teamId: w.teamId, name: `Other`, slug: `other-${id8()}`, prefix: `OT`, deletedAt: new Date() })
      .returning({ id: boards.id })
    await db.update(issues).set({ boardId: other!.id }).where(eq(issues.id, a))
    expect(await link()).toMatchObject({ boardId: other!.id })
    expect((await link()).boardDeletedAt).not.toBeNull()
  })

  it(`writes the ONE Guide on the root, per writer, with caps and removal`, async () => {
    const w = await world()
    const root = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `r` } })
    const child = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `c` } })
    await writeGuide({ userId: w.owner, sessionId: root.sessionId, topic: `Summary`, text: `Root did X.` })
    const out = await writeGuide({
      userId: w.owner,
      sessionId: child.sessionId,
      topic: `Summary`,
      text: `Child did Y.`,
      files: [`a.ts`, `a.ts`, ` `],
      prUrl: `https://www.github.com/o/r/pull/7/files`,
    })
    expect(out.results).toEqual([{ topic: `Summary`, text: 12 }])
    const guide = (await row(root.sessionId)).guide!
    expect(guide).toHaveLength(2)
    expect(guide[1]).toMatchObject({
      sessionId: child.sessionId,
      files: [`a.ts`],
      prUrl: `https://github.com/o/r/pull/7`,
    })
    expect((await row(child.sessionId)).guide).toBeNull()

    await writeGuide({ userId: w.owner, sessionId: child.sessionId, topic: `Summary`, remove: true })
    expect((await row(root.sessionId)).guide).toHaveLength(1)
    await expect(
      writeGuide({ userId: w.owner, sessionId: root.sessionId, topic: `Huge`, text: `x`.repeat(5000) })
    ).rejects.toBeTruthy()
    await expect(
      writeGuide({ userId: w.mate, sessionId: root.sessionId, topic: `T`, text: `nope` })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
  })
})

describe.skipIf(!ready.ok)(`the sessions router: device procedures and answers`, async () => {
  const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
  const caller = (userId: string) =>
    sessionsRouter.createCaller({
      session: { user: { id: userId } },
      db,
      request: new Request(`http://x.test`),
    } as never)

  it(`claims, reports, asks, is answered and consumes — only from the device's account`, async () => {
    const w = await world({ shared: true })
    const sent = await sendSession({
      userId: w.mate,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `start` },
    })
    // The starter is not the device: only the host runs the device procedures.
    await expect(caller(w.mate).claim({ sessionId: sent.sessionId })).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
    const host = caller(w.owner)
    await host.claim({ sessionId: sent.sessionId, branch: `exp/x` })
    expect(await row(sent.sessionId)).toMatchObject({ status: `running`, branch: `exp/x` })
    await host.consume({ sessionId: sent.sessionId, messageIds: [sent.messageId!] })
    const [first] = await db.select().from(messages).where(eq(messages.id, sent.messageId!))
    expect(first!.consumedAt).not.toBeNull()

    const asked = await host.reply({
      sessionId: sent.sessionId,
      body: ``,
      card: { kind: `question`, question: `Which one?`, options: [`a`, `b`] },
    })
    expect((await row(sent.sessionId)).needsInput).toBe(true)
    await expect(
      host.reply({ sessionId: sent.sessionId, body: ``, card: { kind: `session`, sessionId: sent.sessionId } })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })

    // Any member answers a shared session's question IN the card.
    await caller(w.mate).answer({ messageId: asked.messageId, answer: `b` })
    const [card] = await db.select().from(messages).where(eq(messages.id, asked.messageId))
    expect(card!.card).toMatchObject({ kind: `question`, answer: `b` })
    expect(card!.answeredAt).not.toBeNull()
    expect((await row(sent.sessionId)).needsInput).toBe(false)
    await expect(caller(w.owner).answer({ messageId: asked.messageId, answer: `a` })).rejects.toMatchObject({
      code: `CONFLICT`,
    })

    // The answer lives IN the card: no person message is posted for it.
    const posted = await db
      .select()
      .from(messages)
      .where(and(eq(messages.sessionId, sent.sessionId), eq(messages.body, `b`)))
    expect(posted).toEqual([])

    // A queued person message; only its author may revoke it.
    const queued = await sendSession({
      userId: w.mate,
      input: { sessionId: sent.sessionId, text: `one more thing` },
    })
    await expect(caller(w.owner).unqueue({ messageId: queued.messageId! })).rejects.toMatchObject({
      code: `CONFLICT`,
    })
    await caller(w.mate).unqueue({ messageId: queued.messageId! })

    await host.report({ sessionId: sent.sessionId, status: `idle`, busy: false, title: `Auto` })
    expect(await row(sent.sessionId)).toMatchObject({ status: `idle`, title: `Auto` })
    expect((await row(sent.sessionId)).lastTurnAt).not.toBeNull()
    await host.report({ sessionId: sent.sessionId, title: `Other` })
    expect((await row(sent.sessionId)).title).toBe(`Auto`)

    // EXP-849: the device reports the login the run actually runs on (a
    // start pick or a rotation moved it); a report without one leaves it.
    await host.report({ sessionId: sent.sessionId, account: `bbbb2222` })
    expect((await row(sent.sessionId)).account).toBe(`bbbb2222`)
    await host.report({ sessionId: sent.sessionId, busy: true })
    expect((await row(sent.sessionId)).account).toBe(`bbbb2222`)

    const { brief } = await host.brief({ sessionId: sent.sessionId })
    expect(brief).toContain(`start`)
  })

  it(`manages agents: owner writes, members read, a device move re-stamps visibility`, async () => {
    const w = await world()
    const agent = await sendSession({
      userId: w.owner,
      input: {
        teamId: w.teamId,
        deviceId: w.deviceId,
        agent: {
          name: `Ada`,
          icon: `rocket`,
          color: `#123456`,
          instructions: `Hi`,
          triggers: [{ kind: `schedule`, enabled: true, interval: `daily`, minuteOfDay: 540 } as never],
        },
      },
    })
    const stored = (await row(agent.sessionId)).triggers
    expect(stored[0]!.id).toMatch(/^[0-9a-f-]{36}$/)
    expect(await caller(w.owner).getAgent({ sessionId: agent.sessionId })).toEqual({
      sessionId: agent.sessionId,
      instructions: `Hi`,
    })
    // Private (unshared device): the teammate cannot see it.
    await expect(caller(w.mate).getAgent({ sessionId: agent.sessionId })).rejects.toMatchObject({
      code: `NOT_FOUND`,
    })
    const sharedDevice = `dev-${id8()}`
    await db.insert(devices).values({
      userId: w.owner,
      deviceId: sharedDevice,
      label: `srv`,
      kind: `server`,
      sharedTeamIds: [w.teamId],
    })
    await caller(w.owner).updateAgent({ sessionId: agent.sessionId, deviceId: sharedDevice, name: `Ada 2` })
    expect(await row(agent.sessionId)).toMatchObject({
      deviceId: sharedDevice,
      shared: true,
      status: `queued`,
      name: `Ada 2`,
    })
    expect((await caller(w.mate).getAgent({ sessionId: agent.sessionId })).instructions).toBe(`Hi`)
    await expect(caller(w.mate).updateAgent({ sessionId: agent.sessionId, name: `x` })).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
    await caller(w.owner).deleteAgent({ sessionId: agent.sessionId })
    expect(await db.select().from(sessions).where(eq(sessions.id, agent.sessionId))).toEqual([])
  })
})

describe.skipIf(!ready.ok)(`EXP-1323: reports, asides, reads and suggestions`, async () => {
  const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
  const { postSuggestions } = await import(`@/lib/sessions/suggestions`)
  const { sessionReads } = await import(`@/db/schema`)
  const caller = (userId: string) =>
    sessionsRouter.createCaller({
      session: { user: { id: userId } },
      db,
      request: new Request(`http://x.test`),
    } as never)
  const sessionMessages = (sessionId: string) =>
    db.select().from(messages).where(eq(messages.sessionId, sessionId))

  it(`a child's question and end land on its parent as pending agent input`, async () => {
    const w = await world()
    const root = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `r` } })
    const child = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `c` } })
    // The child's MCP key messages its parent: ITS input, not the person's.
    const asked = await sendSession({
      userId: w.owner,
      input: { sessionId: root.sessionId, text: `1:1 ratio?` },
      callerSessionId: child.sessionId,
    })
    const [question] = await db.select().from(messages).where(eq(messages.id, asked.messageId!))
    expect(question).toMatchObject({ authorKind: `agent`, authorSessionId: child.sessionId, consumedAt: null })

    await endSession({ userId: w.owner, sessionId: child.sessionId, summary: `PR #12 is open.` })
    const report = (await sessionMessages(root.sessionId)).find(
      (m) => (m.card as { kind?: string } | null)?.kind === `report`
    )!
    expect(report).toMatchObject({
      authorKind: `agent`,
      authorSessionId: child.sessionId,
      body: `PR #12 is open.`,
      consumedAt: null,
      card: { kind: `report`, sessionId: child.sessionId, summary: `PR #12 is open.` },
    })
    // The device consumes it like a person's message.
    await caller(w.owner).consume({ sessionId: root.sessionId, messageIds: [report.id, question!.id] })
    const after = await db.select().from(messages).where(eq(messages.id, report.id))
    expect(after[0]!.consumedAt).not.toBeNull()
    // Ending the root reports nowhere (no parent).
    const before = (await sessionMessages(root.sessionId)).length
    await endSession({ userId: w.owner, sessionId: root.sessionId })
    expect(
      (await sessionMessages(root.sessionId)).filter((m) => (m.card as { kind?: string } | null)?.kind === `report`)
    ).toHaveLength(1)
    expect((await sessionMessages(root.sessionId)).length).toBe(before + 1)
  })

  it(`the device's ended report falls back to the child's last reply; an aside never reports`, async () => {
    const w = await world()
    const root = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `r` } })
    const child = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `c` } })
    const host = caller(w.owner)
    await host.claim({ sessionId: child.sessionId })
    await host.reply({ sessionId: child.sessionId, body: `All done.` })
    await host.report({ sessionId: child.sessionId, status: `ended` })
    const reports = (await sessionMessages(root.sessionId)).filter(
      (m) => (m.card as { kind?: string } | null)?.kind === `report`
    )
    expect(reports.map((m) => m.body)).toEqual([`All done.`])

    const aside = await sendSession({
      userId: w.owner,
      input: { parentId: root.sessionId, text: `why is CI red?\nctx`, aside: true },
    })
    expect(await row(aside.sessionId)).toMatchObject({
      aside: true,
      cli: `claude`,
      title: `why is CI red?`,
      depth: 1,
      deviceId: w.deviceId,
    })
    await endSession({ userId: w.owner, sessionId: aside.sessionId, summary: `flaky test` })
    expect(
      (await sessionMessages(root.sessionId)).filter((m) => (m.card as { kind?: string } | null)?.kind === `report`)
    ).toHaveLength(1)
    // Never on codex.
    await db.update(sessions).set({ cli: `codex` }).where(eq(sessions.id, root.sessionId))
    await expect(
      sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `q`, aside: true } })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`reports a caption (trimmed, capped, cleared at turn end)`, async () => {
    const w = await world()
    const root = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `r` } })
    const host = caller(w.owner)
    await host.claim({ sessionId: root.sessionId })
    await host.report({ sessionId: root.sessionId, busy: true, caption: `  Running ${`x`.repeat(200)}` })
    expect((await row(root.sessionId)).caption).toHaveLength(80)
    await host.report({ sessionId: root.sessionId, busy: false })
    expect((await row(root.sessionId)).caption).toBeNull()
  })

  it(`markRead upserts the caller's read mark of a visible session only`, async () => {
    const w = await world()
    const root = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `r` } })
    const first = await caller(w.owner).markRead({ sessionId: root.sessionId })
    expect(first.txid).toBeTypeOf(`number`)
    await caller(w.owner).markRead({ sessionId: root.sessionId })
    const reads = await db
      .select()
      .from(sessionReads)
      .where(and(eq(sessionReads.sessionId, root.sessionId), eq(sessionReads.userId, w.owner)))
    expect(reads).toHaveLength(1)
    // Private session: the teammate cannot mark it.
    await expect(caller(w.mate).markRead({ sessionId: root.sessionId })).rejects.toMatchObject({
      code: `NOT_FOUND`,
    })
  })

  it(`suggestions: start as a child, do it here, dismiss, file as an issue`, async () => {
    const w = await world()
    const issueId = await w.issue(`Gallery`)
    const root = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, issueIds: [issueId], text: `r` },
    })
    const posted = await postSuggestions({
      userId: w.owner,
      callerSessionId: root.sessionId,
      tasks: [
        { title: `Thumbnails`, text: `Add lazy thumbnails`, issueIds: [issueId] },
        { title: `Docs`, text: `Document it` },
        { title: `Cleanup`, text: `Drop the old grid` },
        { title: `Followup`, text: `File me` },
      ],
    })
    const [card] = await db.select().from(messages).where(eq(messages.id, posted.messageId))
    expect(card).toMatchObject({ authorKind: `agent`, authorSessionId: root.sessionId })
    expect(card!.consumedAt).not.toBeNull()
    const [a, b, c, d] = posted.itemIds as [string, string, string, string]
    const host = caller(w.owner)

    const started = await host.startSuggestion({ messageId: posted.messageId, itemId: a, where: `child` })
    expect(await row(started.sessionId)).toMatchObject({ parentId: root.sessionId, title: `Thumbnails` })
    const [firstMessage] = await sessionMessages(started.sessionId)
    expect(firstMessage!.body).toBe(`Thumbnails\n\nAdd lazy thumbnails`)
    await expect(
      host.startSuggestion({ messageId: posted.messageId, itemId: a, where: `child` })
    ).rejects.toMatchObject({ code: `CONFLICT` })

    await host.startSuggestion({ messageId: posted.messageId, itemId: b, where: `here` })
    expect((await sessionMessages(root.sessionId)).some((m) => m.body === `Docs\n\nDocument it`)).toBe(true)
    await host.dismissSuggestion({ messageId: posted.messageId, itemId: c })
    const filed = await host.fileSuggestion({ messageId: posted.messageId, itemId: d })
    const [issue] = await db.select().from(issues).where(eq(issues.id, filed.issueId))
    expect(issue).toMatchObject({ boardId: w.boardId, title: `Followup` })
    const linked = await db
      .select()
      .from(sessionIssues)
      .where(and(eq(sessionIssues.sessionId, root.sessionId), eq(sessionIssues.issueId, filed.issueId)))
    expect(linked).toHaveLength(1)

    const [stamped] = await db.select().from(messages).where(eq(messages.id, posted.messageId))
    const items = (stamped!.card as unknown as { items: Array<Record<string, unknown>> }).items
    expect(items[0]!.sessionId).toBe(started.sessionId)
    expect(items[1]!.here).toBe(true)
    expect(items[2]!.dismissed).toBe(true)
    expect(items[3]!.issueId).toBe(filed.issueId)
    // Nothing open is left to start.
    expect((await host.startAllSuggestions({ messageId: posted.messageId })).sessionIds).toEqual([])
  })
})
