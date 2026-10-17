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
  mcpServers,
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

const notifyBlocked = vi.hoisted(() => vi.fn(async (..._args: unknown[]) => {}))
vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  notifySessionBlocked: notifyBlocked,
  // EXP-1328: File as issue runs issues.create.
  fireAndForgetAssignmentNotify: () => {},
  fireAndForgetIssueMentionNotify: () => {},
}))

// EXP-1252: the handoff package's object store (never reached here).
const storage = vi.hoisted(() => ({
  uploaded: [] as string[],
  deleted: [] as string[],
}))
vi.mock(`@/lib/storage`, () => ({
  uploadObject: async (options: { key: string }) => {
    storage.uploaded.push(options.key)
  },
  getObject: async () => ({ Body: {}, ContentLength: 3 }),
  toResponseBody: async () => new Uint8Array([1, 2, 3]).buffer,
}))
vi.mock(`@/lib/storage/issue-attachment-cleanup`, () => ({
  deleteStorageObjects: async (keys: string[]) => {
    storage.deleted.push(...keys)
  },
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
    // EXP-1341: the shared chain: no name yet, so its issue names it.
    expect(listed).toMatchObject({ id: root.id })
    expect(listed!.title).toMatch(/^#CR-\d+ Fix login$/)
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
    // A dispatcher started on an action hands it off; a work child sees the
    // action through its dispatcher parent.
    expect(brief.brief).toContain(`## Dispatch`)
    const child = await sendSession({ userId: w.owner, input: { parentId: sent.sessionId, text: `do it` } })
    const childBrief = await composeSessionBrief(child.sessionId)
    expect(childBrief.brief).toContain(`Triage the issue.`)
    expect(childBrief.brief).not.toContain(`## Dispatch`)

    // An ended dispatcher's continuation carries the Action on, so ITS
    // children (grandchildren of the root) still get it, and dispatches.
    await endSession({ userId: w.owner, sessionId: sent.sessionId, summary: `Done.` })
    const cont = await sendSession({ userId: w.owner, input: { sessionId: sent.sessionId, text: `again` } })
    expect(await row(cont.sessionId)).toMatchObject({
      dispatch: true,
      actionId: action!.id,
      actionName: `Triage`,
      actionInputs: { issue: issueId },
    })
    expect((await composeSessionBrief(cont.sessionId)).brief).toContain(`## Dispatch`)
    const grandchild = await sendSession({ userId: w.owner, input: { parentId: cont.sessionId, text: `go` } })
    expect((await composeSessionBrief(grandchild.sessionId)).brief).toContain(`Triage the issue.`)
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
    // An old client's blank model/effort ("CLI default") is tolerated but
    // never stored: the inherited pick stays.
    const blank = await sendSession({
      userId: w.owner,
      input: { parentId: root.sessionId, text: `part 3`, model: ``, effort: `` },
    })
    expect(await row(blank.sessionId)).toMatchObject({ model: `opus`, effort: `high` })
    const blankRoot = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, cli: `claude`, model: ``, text: `x` },
    })
    expect((await row(blankRoot.sessionId)).workLaunch).toMatchObject({ model: null })

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

  it(`EXP-1341: a child takes a name and its ASSIGNED issues; a message forwards one`, async () => {
    const w = await world()
    const [a, b, c] = [await w.issue(`Lag`), await w.issue(`Jank`), await w.issue(`Typo`)]
    const root = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, issueIds: [a, b], text: `perf`, name: `Perf pass` },
    })
    expect(await row(root.sessionId)).toMatchObject({ title: `Perf pass` })
    const grouped = await sendSession({
      userId: w.owner,
      input: { parentId: root.sessionId, text: `Fix both`, issueIds: [a, b] },
    })
    const named = await sendSession({
      userId: w.owner,
      input: { parentId: root.sessionId, text: `Fix the typo`, issueIds: [c], name: `Copy fixes` },
    })
    const links = await db.select().from(sessionIssues).where(eq(sessionIssues.sessionId, root.sessionId))
    const assigned = Object.fromEntries(links.map((link) => [link.issueId, link.assignedSessionId]))
    // c was new: linked onto the ROOT, assigned to its child.
    expect(assigned).toEqual({ [a]: grouped.sessionId, [b]: grouped.sessionId, [c]: named.sessionId })

    const tree = await getSessionTree(w.owner, root.sessionId)
    expect(tree.root.displayTitle).toBe(`Perf pass`)
    const titles = Object.fromEntries(tree.children.map((s) => [s.id, s.displayTitle]))
    expect(titles[grouped.sessionId]).toMatch(/^#CR-\d+ Lag \+ 1$/)
    expect(titles[named.sessionId]).toBe(`Copy fixes`)
    // The child's brief lists only its share.
    const brief = await composeSessionBrief(grouped.sessionId)
    expect(brief.brief).toContain(`Lag`)
    expect(brief.brief).not.toContain(`Typo`)

    // Forward: c moves to the grouped child.
    await sendSession({ userId: w.owner, input: { sessionId: grouped.sessionId, issueIds: [c] } })
    const [moved] = await db
      .select()
      .from(sessionIssues)
      .where(and(eq(sessionIssues.sessionId, root.sessionId), eq(sessionIssues.issueId, c)))
    expect(moved!.assignedSessionId).toBe(grouped.sessionId)
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

    // EXP-1281: a wall the device rotates past (`handled`) is stored without
    // the flag and pushes nobody; an unhandled one notifies.
    const wall = {
      kind: `rate_limit`,
      agent: `claude`,
      window: `session`,
      resetsAt: `2026-10-17T12:00:00.000Z`,
      since: `2026-10-17T10:00:00.000Z`,
    } as const
    notifyBlocked.mockClear()
    await host.report({ sessionId: sent.sessionId, blocked: { ...wall, handled: true } })
    expect((await row(sent.sessionId)).blocked).toEqual(wall)
    expect(notifyBlocked).not.toHaveBeenCalled()
    await host.report({ sessionId: sent.sessionId, blocked: null })
    await host.report({ sessionId: sent.sessionId, blocked: wall })
    expect(notifyBlocked).toHaveBeenCalledTimes(1)
    await host.report({ sessionId: sent.sessionId, blocked: null })

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

  it(`EXP-1334: configure changes a live session's options mid-session`, async () => {
    const w = await world()
    const [server] = await db
      .insert(mcpServers)
      .values({ teamId: w.teamId, name: `linear`, url: `https://mcp.linear.app/mcp` })
      .returning({ id: mcpServers.id })
    const root = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, cli: `claude`, text: `root` },
    })
    const child = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `work` } })
    const owner = caller(w.owner)

    // The row's synced pickers move; the MCP pick stays server-only and
    // reads back through the device's `mcpPicks`.
    const out = await owner.configure({
      sessionId: child.sessionId,
      model: `sonnet`,
      planMode: true,
      effort: `high`,
      ultracode: true,
      computerUse: true,
      mcpServerIds: [server!.id, server!.id],
    })
    expect(out.txid).toBeTypeOf(`number`)
    expect(await row(child.sessionId)).toMatchObject({
      model: `sonnet`,
      planMode: true,
      effort: `high`,
      ultracode: true,
      computerUse: true,
      mcpServerIds: [server!.id],
    })
    expect(await owner.mcpPicks({ sessionId: child.sessionId })).toEqual({ mcpServerIds: [server!.id] })
    // Omitted = unchanged; a null effort = the agent's default; [] = none.
    await owner.configure({ sessionId: child.sessionId, effort: null, mcpServerIds: [] })
    expect(await row(child.sessionId)).toMatchObject({ planMode: true, effort: null, mcpServerIds: null })
    expect(await owner.mcpPicks({ sessionId: child.sessionId })).toEqual({ mcpServerIds: [] })

    // Validation: the agent's own effort set, ultracode on claude only, the
    // team's own MCP servers.
    await expect(owner.configure({ sessionId: child.sessionId, effort: `ludicrous` })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
    })
    await expect(owner.configure({ sessionId: child.sessionId, model: `gpt-5.6-sol` })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
    })
    await expect(
      owner.configure({ sessionId: child.sessionId, mcpServerIds: [crypto.randomUUID()] })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    const codex = await sendSession({
      userId: w.owner,
      input: { parentId: root.sessionId, cli: `codex`, text: `codex work` },
    })
    await expect(owner.configure({ sessionId: codex.sessionId, ultracode: true })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
    })
    await owner.configure({ sessionId: codex.sessionId, model: `gpt-5.6-luna` })
    expect((await row(codex.sessionId)).model).toBe(`gpt-5.6-luna`)

    // A dispatcher root keeps its own runner: the picks are its WORK picks.
    await owner.configure({ sessionId: root.sessionId, model: `opus`, planMode: true, effort: `low` })
    const dispatcher = await row(root.sessionId)
    expect(dispatcher).toMatchObject({ dispatch: true, model: null, planMode: false, effort: null })
    expect(dispatcher.workLaunch).toMatchObject({ model: `opus`, planMode: true, effort: `low` })

    // The steer rule: a private session refuses a teammate; the device-only
    // read refuses anyone but the host.
    await expect(caller(w.mate).configure({ sessionId: child.sessionId, planMode: false })).rejects.toMatchObject({
      code: `NOT_FOUND`,
    })
    // An ended session refuses.
    await endSession({ userId: w.owner, sessionId: child.sessionId, summary: `Done.` })
    await expect(owner.configure({ sessionId: child.sessionId, planMode: false })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
    })
  })

  it(`EXP-1334: the device reports the agent's own plan-mode move onto the row`, async () => {
    const w = await world()
    const root = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, cli: `claude`, text: `root` },
    })
    const child = await sendSession({
      userId: w.owner,
      input: { parentId: root.sessionId, planMode: true, text: `plan first` },
    })
    const host = caller(w.owner)
    expect((await row(child.sessionId)).planMode).toBe(true)
    // The plan was approved: claude left plan mode, the row follows.
    await host.report({ sessionId: child.sessionId, planMode: false })
    expect((await row(child.sessionId)).planMode).toBe(false)
    // Omitted = unchanged; a dispatcher never plans, whatever is reported.
    await host.report({ sessionId: child.sessionId, busy: true })
    expect((await row(child.sessionId)).planMode).toBe(false)
    await host.report({ sessionId: root.sessionId, planMode: true })
    expect((await row(root.sessionId)).planMode).toBe(false)

    // The model the run reports (a `/model` switch) follows onto the row; a
    // value outside the agent's vocabulary is ignored, never refused.
    await host.report({ sessionId: child.sessionId, model: `haiku` })
    expect((await row(child.sessionId)).model).toBe(`haiku`)
    await host.report({ sessionId: child.sessionId, model: `claude-opus-4-1-20250805`, busy: false })
    expect(await row(child.sessionId)).toMatchObject({ model: `haiku`, busy: false })
    await host.report({ sessionId: root.sessionId, model: `opus` })
    expect((await row(root.sessionId)).model).toBeNull()
  })

  it(`EXP-1334: mcpPicks is readable by whoever may steer the session`, async () => {
    const shared = await world({ shared: true })
    const [a, b] = await db
      .insert(mcpServers)
      .values([
        { teamId: shared.teamId, name: `sentry`, url: `https://mcp.sentry.dev/mcp` },
        { teamId: shared.teamId, name: `linear`, url: `https://mcp.linear.app/mcp` },
      ])
      .returning({ id: mcpServers.id })
    const sent = await sendSession({
      userId: shared.mate,
      input: { teamId: shared.teamId, deviceId: shared.deviceId, text: `start` },
    })
    // A shared session: every member steers it, so every member reads the
    // pick (the composer's MCP submenu); the hosting device too.
    await caller(shared.mate).configure({ sessionId: sent.sessionId, mcpServerIds: [b!.id, a!.id] })
    const sorted = [a!.id, b!.id].sort()
    expect(await caller(shared.mate).mcpPicks({ sessionId: sent.sessionId })).toEqual({ mcpServerIds: sorted })
    expect(await caller(shared.owner).mcpPicks({ sessionId: sent.sessionId })).toEqual({ mcpServerIds: sorted })

    // A private one: only its starter.
    const priv = await world()
    const mine = await sendSession({
      userId: priv.owner,
      input: { teamId: priv.teamId, deviceId: priv.deviceId, text: `mine` },
    })
    expect(await caller(priv.owner).mcpPicks({ sessionId: mine.sessionId })).toEqual({ mcpServerIds: [] })
    await expect(caller(priv.mate).mcpPicks({ sessionId: mine.sessionId })).rejects.toMatchObject({
      code: `NOT_FOUND`,
    })
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
      viaMcp: true,
    })
    const [question] = await db.select().from(messages).where(eq(messages.id, asked.messageId!))
    expect(question).toMatchObject({ authorKind: `agent`, authorSessionId: child.sessionId, consumedAt: null })
    // Over tRPC the same header only STAMPS a person's row: never "from A".
    const typed = await sendSession({
      userId: w.owner,
      input: { sessionId: root.sessionId, text: `a person typed this` },
      callerSessionId: child.sessionId,
    })
    const [personRow] = await db.select().from(messages).where(eq(messages.id, typed.messageId!))
    expect(personRow).toMatchObject({ authorKind: `user`, authorId: w.owner, authorSessionId: child.sessionId })

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

    // An aside forks a RUNNING parent: a queued one has no run yet, an ended
    // one none anymore.
    await expect(
      sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `q`, aside: true } })
    ).rejects.toMatchObject({ code: `BAD_REQUEST`, message: `Asides need a running session` })
    await expect(
      sendSession({ userId: w.owner, input: { parentId: child.sessionId, text: `q`, aside: true } })
    ).rejects.toMatchObject({ code: `BAD_REQUEST`, message: `Asides need a running session` })
    await host.claim({ sessionId: root.sessionId })
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


// ── EXP-1252: moving a session to another device (+ #FEED-84 / #FEED-85) ────

const signedIn = (id: string, email: string) => ({
  claude: { signedIn: true, email, profiles: [{ id, signedIn: true, email, active: true }] },
})

describe.skipIf(!ready.ok)(`EXP-1252: moving a session to another device`, async () => {
  const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
  const { putHandoffBlob, getHandoffBlob } = await import(`@/lib/sessions/handoff`)
  const { MOVE_COPY } = await import(`@/lib/sessions/move`)
  const caller = (userId: string) =>
    sessionsRouter.createCaller({
      session: { user: { id: userId } },
      db,
      request: new Request(`http://x.test`),
    } as never)

  /** The owner's second machine, online, signed in to claude. */
  async function moveWorld() {
    const w = await world()
    await db
      .update(devices)
      .set({ agentAccounts: signedIn(`pa`, `me@move.test`) })
      .where(eq(devices.deviceId, w.deviceId))
    const target = `dev-b-${id8()}`
    await db.insert(devices).values({
      userId: w.owner,
      deviceId: target,
      label: `b-box`,
      kind: `desktop`,
      agentAccounts: signedIn(`pb`, `ME@move.test`),
    })
    const root = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, cli: `claude`, account: `pa`, text: `work` },
    })
    await caller(w.owner).claim({ sessionId: root.sessionId, deviceId: w.deviceId })
    return { ...w, target, sessionId: root.sessionId, firstMessageId: root.messageId! }
  }

  it(`requests a move (carried login, caption, nothing posted) and cancels it`, async () => {
    const w = await moveWorld()
    const before = await db.select().from(messages).where(eq(messages.sessionId, w.sessionId))
    const sent = await sendSession({ userId: w.owner, input: { sessionId: w.sessionId, deviceId: w.target } })
    expect(sent).toMatchObject({ sessionId: w.sessionId, messageId: null, status: `running` })
    const moving = await row(w.sessionId)
    expect(moving).toMatchObject({
      deviceId: w.deviceId,
      moveToDeviceId: w.target,
      caption: `Moving to b-box…`,
      account: `pa`,
    })
    // The same address on the target carries (case-insensitive).
    expect(moving.handoff?.request).toMatchObject({ deviceId: w.target, account: `pb`, requestedBy: w.owner })
    expect(await db.select().from(messages).where(eq(messages.sessionId, w.sessionId))).toHaveLength(
      before.length
    )
    // The current device while moving = cancel.
    await sendSession({ userId: w.owner, input: { sessionId: w.sessionId, deviceId: w.deviceId } })
    expect(await row(w.sessionId)).toMatchObject({ moveToDeviceId: null, handoff: null, caption: null })
    // The current device with nothing pending = nothing at all.
    await sendSession({ userId: w.owner, input: { sessionId: w.sessionId, deviceId: w.deviceId, text: `hi` } })
    expect(await row(w.sessionId)).toMatchObject({ deviceId: w.deviceId, moveToDeviceId: null })
  })

  it(`refuses with the client copy: offline, visibility, no login, aside, agent, ended`, async () => {
    const w = await moveWorld()
    const add = async (values: Partial<typeof devices.$inferInsert>) => {
      const deviceId = `dev-x-${id8()}`
      await db.insert(devices).values({
        userId: w.owner,
        deviceId,
        label: `x`,
        kind: `server`,
        agentAccounts: signedIn(`px`, `me@move.test`),
        ...values,
      })
      return deviceId
    }
    const refuse = (deviceId: string, message: string, sessionId = w.sessionId) =>
      expect(sendSession({ userId: w.owner, input: { sessionId, deviceId } })).rejects.toMatchObject({
        message,
      })
    await refuse(await add({ lastSeenAt: new Date(Date.now() - 3_600_000) }), MOVE_COPY.offline)
    await refuse(await add({ sharedTeamIds: [w.teamId] }), MOVE_COPY.visibility)
    await refuse(await add({ agentAccounts: null }), MOVE_COPY.noLogin)
    // A teammate's machine nobody shared is invisible to the starter.
    await db.insert(devices).values({ userId: w.mate, deviceId: `dev-m-${id8()}`, label: `m`, kind: `desktop` })
    const mateDevice = (
      await db.select({ deviceId: devices.deviceId }).from(devices).where(eq(devices.userId, w.mate))
    )[0]!.deviceId
    await refuse(mateDevice, MOVE_COPY.visibility)
    // An explicit login must be one of the target's.
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: w.sessionId, deviceId: w.target, account: `nope` } })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })

    const aside = await sendSession({
      userId: w.owner,
      input: { parentId: w.sessionId, text: `why?`, aside: true },
    })
    await refuse(w.target, MOVE_COPY.aside, aside.sessionId)
    const agent = await sendSession({
      userId: w.owner,
      input: {
        teamId: w.teamId,
        deviceId: w.deviceId,
        agent: { name: `Bot`, icon: `bot`, color: `#336699`, instructions: ``, triggers: [] },
      },
    })
    await refuse(w.target, MOVE_COPY.agent, agent.sessionId)
    await endSession({ userId: w.owner, sessionId: w.sessionId })
    await refuse(w.target, MOVE_COPY.ended)
  })

  it(`hands over: package up, moved re-stamps queued, the target restores without re-feeding`, async () => {
    const w = await moveWorld()
    const host = caller(w.owner)
    await sendSession({ userId: w.owner, input: { sessionId: w.sessionId, deviceId: w.target } })
    // A message sent while moving waits for the target.
    const waiting = await sendSession({ userId: w.owner, input: { sessionId: w.sessionId, text: `after` } })

    // Only the source, only with a move pending.
    await expect(
      host.handoff({ sessionId: w.sessionId, deviceId: w.target, outcome: `moved` })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
    const tooBig = await putHandoffBlob(
      w.owner,
      w.sessionId,
      new Request(`http://x.test`, { method: `PUT`, body: new Uint8Array(10) }),
      4
    )
    expect(tooBig.status).toBe(413)
    const put = await putHandoffBlob(
      w.owner,
      w.sessionId,
      new Request(`http://x.test`, { method: `PUT`, body: new Uint8Array([31, 139, 8]) })
    )
    expect(put.status).toBe(201)
    const blob = (await row(w.sessionId)).handoff?.blob
    expect(blob).toMatchObject({ sizeBytes: 3, fromDeviceId: w.deviceId, fromDeviceName: `box` })
    expect(storage.uploaded).toContain(blob!.storageKey)

    await host.handoff({ sessionId: w.sessionId, deviceId: w.deviceId, outcome: `moved` })
    const moved = await row(w.sessionId)
    expect(moved).toMatchObject({
      deviceId: w.target,
      hostUserId: null,
      account: `pb`,
      status: `queued`,
      busy: false,
      moveToDeviceId: null,
      caption: null,
    })
    expect(moved.startedAt).not.toBeNull()
    expect(moved.handoff).toMatchObject({ request: null, blob: { storageKey: blob!.storageKey } })
    const system = await db
      .select()
      .from(messages)
      .where(and(eq(messages.sessionId, w.sessionId), eq(messages.authorKind, `system`)))
    expect(system.map((m) => m.body)).toContain(`Moved from box to b-box`)

    // The target's brief names the package; the old device is shut out.
    expect((await host.brief({ sessionId: w.sessionId })).handoff).toEqual({
      sizeBytes: 3,
      fromDeviceId: w.deviceId,
      fromDeviceName: `box`,
    })
    await expect(host.claim({ sessionId: w.sessionId, deviceId: w.deviceId })).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
    await host.report({ sessionId: w.sessionId, deviceId: w.deviceId, status: `idle` })
    expect((await row(w.sessionId)).status).toBe(`queued`)
    await host.claim({ sessionId: w.sessionId, deviceId: w.target })
    expect((await row(w.sessionId)).status).toBe(`running`)
    const [pending] = await db.select().from(messages).where(eq(messages.id, waiting.messageId!))
    expect(pending!.consumedAt).toBeNull()

    const got = await getHandoffBlob(w.owner, w.sessionId)
    expect(got.status).toBe(200)
    expect(got.headers.get(`content-type`)).toBe(`application/gzip`)
    await host.handoffDone({ sessionId: w.sessionId })
    expect((await row(w.sessionId)).handoff).toBeNull()
    expect(storage.deleted).toContain(blob!.storageKey)
    expect((await getHandoffBlob(w.owner, w.sessionId)).status).toBe(404)
    // No move pending: the upload is refused.
    const late = await putHandoffBlob(
      w.owner,
      w.sessionId,
      new Request(`http://x.test`, { method: `PUT`, body: new Uint8Array([1]) })
    )
    expect(late.status).toBe(412)
  })

  it(`a failed handoff clears the move and says why; a never-started session moves at once`, async () => {
    const w = await moveWorld()
    await sendSession({ userId: w.owner, input: { sessionId: w.sessionId, deviceId: w.target } })
    await caller(w.owner).handoff({
      sessionId: w.sessionId,
      deviceId: w.deviceId,
      outcome: `failed`,
      reason: `disk full`,
    })
    expect(await row(w.sessionId)).toMatchObject({ deviceId: w.deviceId, moveToDeviceId: null, handoff: null })
    const system = await db
      .select()
      .from(messages)
      .where(and(eq(messages.sessionId, w.sessionId), eq(messages.authorKind, `system`)))
    expect(system.map((m) => m.body)).toContain(`Couldn't move the session: disk full`)

    const fresh = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, cli: `claude`, text: `not yet` },
    })
    await sendSession({ userId: w.owner, input: { sessionId: fresh.sessionId, deviceId: w.target } })
    expect(await row(fresh.sessionId)).toMatchObject({
      deviceId: w.target,
      moveToDeviceId: null,
      status: `queued`,
      // No login picked at the start: the target settles on its own.
      account: null,
    })
  })

  it(`#FEED-84: an ended CHILD continues as its sibling with its branch, title and issues`, async () => {
    const w = await world()
    const issueId = await w.issue(`Sibling`)
    const root = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `r` } })
    const c1 = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `c1` } })
    const c2 = await sendSession({
      userId: w.owner,
      input: { parentId: c1.sessionId, text: `c2`, name: `Deep work`, issueIds: [issueId] },
    })
    await caller(w.owner).claim({ sessionId: c2.sessionId, branch: `exp/deep` })
    await endSession({ userId: w.owner, sessionId: c2.sessionId })
    const cont = await sendSession({ userId: w.owner, input: { sessionId: c2.sessionId, text: `more` } })
    expect(await row(cont.sessionId)).toMatchObject({
      parentId: c1.sessionId,
      rootId: root.sessionId,
      depth: 2,
      branch: `exp/deep`,
      title: `Deep work`,
      status: `queued`,
    })
    const [link] = await db
      .select()
      .from(sessionIssues)
      .where(and(eq(sessionIssues.sessionId, root.sessionId), eq(sessionIssues.issueId, issueId)))
    expect(link!.assignedSessionId).toBe(cont.sessionId)
    // An ended ROOT keeps continuing as its own child.
    await endSession({ userId: w.owner, sessionId: root.sessionId })
    const rootCont = await sendSession({ userId: w.owner, input: { sessionId: root.sessionId, text: `again` } })
    expect(await row(rootCont.sessionId)).toMatchObject({ parentId: root.sessionId, depth: 1, branch: null })
  })

  it(`#FEED-84: sessions_list and sessions_get show the agent and its login`, async () => {
    const w = await moveWorld()
    const [listed] = await listSessions(w.owner, { teamId: w.teamId, kind: `thread`, limit: 10, offset: 0 })
    expect(listed).toMatchObject({
      id: w.sessionId,
      cli: `claude`,
      account: `pa`,
      accountEmail: `me@move.test`,
      accountHealth: `ok`,
      blocked: null,
    })
    expect(listed).not.toHaveProperty(`hostUserId`)
    const tree = await getSessionTree(w.owner, w.sessionId)
    expect(tree.root).toMatchObject({ accountEmail: `me@move.test`, accountHealth: `ok` })
    expect(tree.root).not.toHaveProperty(`handoff`)
  })

  it(`#FEED-85: idle clears busy, the sweep clears a dead device's busy, Stop frees a ghost`, async () => {
    const { stopSession, sweepStaleSessions } = await import(`@/lib/sessions/lifecycle`)
    const w = await moveWorld()
    const host = caller(w.owner)
    await host.report({ sessionId: w.sessionId, busy: true })
    expect((await row(w.sessionId)).busy).toBe(true)
    await host.report({ sessionId: w.sessionId, status: `idle` })
    expect(await row(w.sessionId)).toMatchObject({ status: `idle`, busy: false })

    // A dead device's idle row with a stale spinner.
    const old = new Date(Date.now() - 3 * 3_600_000)
    await db.update(devices).set({ lastSeenAt: old }).where(eq(devices.deviceId, w.deviceId))
    // (the updated_at trigger would re-stamp it: write past the triggers)
    await db.transaction(async (tx) => {
      await tx.execute(sql`SET LOCAL session_replication_role = replica`)
      await tx.update(sessions).set({ busy: true, updatedAt: old }).where(eq(sessions.id, w.sessionId))
    })
    await sweepStaleSessions()
    expect((await row(w.sessionId)).busy).toBe(false)

    // Stop on a running row whose device is offline frees it.
    await db.update(sessions).set({ status: `running`, busy: true }).where(eq(sessions.id, w.sessionId))
    await stopSession({ userId: w.owner, sessionId: w.sessionId })
    expect(await row(w.sessionId)).toMatchObject({ status: `idle`, busy: false })
    // An online device's Stop only interrupts.
    await db.update(devices).set({ lastSeenAt: new Date() }).where(eq(devices.deviceId, w.deviceId))
    await db.update(sessions).set({ status: `running`, busy: true }).where(eq(sessions.id, w.sessionId))
    await stopSession({ userId: w.owner, sessionId: w.sessionId })
    expect(await row(w.sessionId)).toMatchObject({ status: `running`, busy: true })
  })
})

// EXP-1280 custom plan mode: the card a session posts on ITSELF
// (`exponential_plan_propose` / `exponential_sessions_ask`), its pictures,
// the answer (approval leaves plan mode) and the device's consume.
describe.skipIf(!ready.ok)(`plan mode: the plan card`, async () => {
  const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
  const { buildPlanCard, buildQuestionCard, postAskCard, publishCardPicture, assertCardPictureOpen } =
    await import(`@/lib/sessions/plan`)
  const { getSessionAttachmentTeamContext } = await import(`@/lib/team-membership`)
  const caller = (userId: string) =>
    sessionsRouter.createCaller({
      session: { user: { id: userId } },
      db,
      request: new Request(`http://x.test`),
    } as never)
  const message = async (id: string) => (await db.select().from(messages).where(eq(messages.id, id)))[0]!
  const picture = (label: string) => ({
    attachmentId: crypto.randomUUID(),
    filename: `${label}.png`,
    contentType: `image/png`,
    sizeBytes: 3,
    storageKey: `test/${label}-${id8()}.png`,
    url: `/api/attachments/x`,
    width: 640,
    height: 400,
  })

  it(`proposes, takes its picture, is approved with answers and consumed by the device`, async () => {
    const w = await world({ shared: true })
    const sent = await sendSession({
      userId: w.mate,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `plan it` },
    })
    await db.update(sessions).set({ planMode: true, dispatch: false }).where(eq(sessions.id, sent.sessionId))
    const host = caller(w.owner)
    await host.claim({ sessionId: sent.sessionId })

    // The brief carries the Plan section for a planning worker.
    const { brief } = await host.brief({ sessionId: sent.sessionId })
    expect(brief).toContain(`## Plan`)

    const card = buildPlanCard({
      plan: `1. Tiles\n\n![Grid](upload:grid)`,
      pictures: [{ label: `grid` }],
      questions: [{ id: `layout`, prompt: `Layout?`, custom: false, options: [{ label: `Grid`, picture: `grid` }, { label: `List` }] }],
      surface: { root: { id: `r`, component: `Text`, props: { text: { path: `/name` } } }, data: { name: `x` } },
    })
    const posted = await postAskCard({ userId: w.mate, sessionId: sent.sessionId, card })
    expect(posted.uploads).toHaveLength(1)
    expect(posted.uploads[0]!.uploadUrl).toContain(`/api/guide-uploads/`)
    const stored = await message(posted.messageId)
    expect(stored).toMatchObject({
      authorKind: `agent`,
      authorSessionId: sent.sessionId,
      consumedAt: null,
      answeredAt: null,
      card: { kind: `plan`, decision: null, pictures: [{ label: `grid`, attachmentId: null }] },
    })
    expect((await row(sent.sessionId)).needsInput).toBe(true)

    // ONE open plan per session.
    await expect(postAskCard({ userId: w.mate, sessionId: sent.sessionId, card })).rejects.toMatchObject({
      code: `CONFLICT`,
    })
    // An unanswered card is never consumed.
    await host.consume({ sessionId: sent.sessionId, messageIds: [posted.messageId] })
    expect((await message(posted.messageId)).consumedAt).toBeNull()

    // The picture lands on the card's message and resolves the label.
    const shot = picture(`grid`)
    await publishCardPicture(
      { messageId: posted.messageId, sessionId: sent.sessionId, teamId: w.teamId, label: `grid`, uploaderId: w.mate },
      shot
    )
    const resolved = (await message(posted.messageId)).card as Extract<MessageCardRow, { kind: `plan` }>
    expect(resolved.plan).toBe(`1. Tiles\n\n![Grid](/api/attachments/${shot.attachmentId})`)
    expect(resolved.pictures).toEqual([
      { label: `grid`, attachmentId: shot.attachmentId, width: 640, height: 400, caption: null },
    ])
    const [file] = await db.select().from(attachments).where(eq(attachments.id, shot.attachmentId))
    expect(file).toMatchObject({ messageId: posted.messageId, teamId: w.teamId, issueId: null })
    // Readable through the session-file path (shared session → every member).
    expect(await getSessionAttachmentTeamContext(shot.attachmentId)).toMatchObject({
      teamId: w.teamId,
      messageId: posted.messageId,
      messageShared: true,
    })
    // A picture is final; an unknown label is refused before storage work.
    await expect(assertCardPictureOpen(posted.messageId, sent.sessionId, `grid`)).rejects.toMatchObject({
      code: `CONFLICT`,
    })
    await expect(assertCardPictureOpen(posted.messageId, sent.sessionId, `nope`)).rejects.toMatchObject({
      code: `NOT_FOUND`,
    })

    // Approving needs every question answered.
    await expect(
      caller(w.mate).answer({ messageId: posted.messageId, decision: `approved` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await caller(w.mate).answer({
      messageId: posted.messageId,
      decision: `approved`,
      answers: { layout: [`Grid`] },
      surfaceData: { name: `Ada` },
      feedback: ` go `,
    })
    const answered = await message(posted.messageId)
    expect(answered.answeredAt).not.toBeNull()
    expect(answered.card).toMatchObject({
      decision: `approved`,
      fresh: false,
      feedback: `go`,
      answers: { layout: [`Grid`] },
      surfaceData: { name: `Ada` },
    })
    expect(await row(sent.sessionId)).toMatchObject({ planMode: false, needsInput: false })

    // Only the host device consumes it, once the answer text was delivered.
    await expect(
      caller(w.mate).consume({ sessionId: sent.sessionId, messageIds: [posted.messageId] })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
    await host.consume({ sessionId: sent.sessionId, messageIds: [posted.messageId] })
    expect((await message(posted.messageId)).consumedAt).not.toBeNull()

    // A rejection keeps planning; a new plan may follow once one is answered.
    await db.update(sessions).set({ planMode: true }).where(eq(sessions.id, sent.sessionId))
    const second = await postAskCard({ userId: w.mate, sessionId: sent.sessionId, card: buildPlanCard({ plan: `2. Again` }) })
    await caller(w.mate).answer({ messageId: second.messageId, decision: `rejected` })
    expect(await row(sent.sessionId)).toMatchObject({ planMode: true, needsInput: false })
  })

  it(`asks questions (several may wait) and stamps the picked labels`, async () => {
    const w = await world()
    const sent = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `ask` },
    })
    const ask = (question: string) =>
      postAskCard({
        userId: w.owner,
        sessionId: sent.sessionId,
        card: buildQuestionCard({ question, options: [{ label: `A` }, { label: `B` }], multi: true, custom: false }),
      })
    const first = await ask(`Which?`)
    const second = await ask(`And?`)
    await caller(w.owner).answer({ messageId: first.messageId, answers: [`A`, `B`] })
    expect((await message(first.messageId)).card).toMatchObject({ answer: `A, B`, answers: [`A`, `B`] })
    // The other question still waits on a person.
    expect((await row(sent.sessionId)).needsInput).toBe(true)
    await expect(
      caller(w.owner).answer({ messageId: second.messageId, answer: `C` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await caller(w.owner).answer({ messageId: second.messageId, answers: [`B`] })
    expect((await row(sent.sessionId)).needsInput).toBe(false)

    // An ended session takes no card.
    await db.update(sessions).set({ status: `ended` }).where(eq(sessions.id, sent.sessionId))
    await expect(ask(`Late?`)).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
  })
})

type MessageCardRow = import("@exp/db-schema/domain").MessageCard
