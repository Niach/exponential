// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  agents,
  attachments,
  boards,
  conversations,
  devices,
  issues,
  mcpServers,
  messages,
  prompts,
  sessionIssues,
  sessions,
  teamMembers,
  teams,
} from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// EXP-1291/NEW-1: the session tree's core against the REAL migrated
// Postgres — sessions.send's three forms (a root that WORKS on its own
// picks), visibility, the derived host, the tree caps, lifecycle, moves,
// the Guide, the brief and the custom-trigger mirrors. The bus and the
// watchdog have their own suites (bus/watchdog.real-db.test.ts). These helpers open their
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

// The object store (never reached here: card pictures arrive as rows).
vi.mock(`@/lib/storage`, () => ({
  uploadObject: async () => {},
  getObject: async () => ({ Body: {}, ContentLength: 3 }),
  toResponseBody: async () => new Uint8Array([1, 2, 3]).buffer,
}))
vi.mock(`@/lib/storage/issue-attachment-cleanup`, () => ({
  deleteStorageObjects: async () => {},
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
const { resolveSessionHost } = await import(`@/lib/sessions/host`)

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
      cli: null,
    })
    // NEW-1: the host is derived from the devices rows: the starter's own.
    expect(await resolveSessionHost(root)).toBe(w.owner)
    const links = await db.select().from(sessionIssues).where(eq(sessionIssues.sessionId, root.id))
    expect(links).toMatchObject([{ issueId, ownerId: w.owner, shared: false, boardId: w.boardId }])
    const [message] = await db.select().from(messages).where(eq(messages.id, sent.messageId!))
    expect(message).toMatchObject({ rootId: root.id, teamId: w.teamId, ownerId: w.owner, authorKind: `user` })
    // Start facets post no event cards (the brief names them).
    const cards = await db.select().from(messages).where(eq(messages.rootId, root.id))
    expect(cards).toHaveLength(1)

    // Private: the teammate cannot see it.
    await expect(loadSessionForUser(w.mate, root.id)).rejects.toMatchObject({ code: `NOT_FOUND` })
    expect(await listSessions(w.mate, { limit: 10, offset: 0 })).toEqual([])
    const [listed] = await listSessions(w.owner, { teamId: w.teamId, limit: 10, offset: 0 })
    // EXP-1341: the shared chain: no name yet, so its issue names it.
    expect(listed).toMatchObject({ id: root.id })
    expect(listed!.title).toMatch(/^#CR-\d+ Fix login$/)
  })

  it(`NEW-3: a session owns ONE conversation of kind session; its messages name it`, async () => {
    const w = await world({ shared: true })
    const sent = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `hello` },
    })
    // The `create_session_conversation` trigger made it with the row, the
    // session's visibility mirrors stamped once.
    const [conversation] = await db
      .select()
      .from(conversations)
      .where(eq(conversations.sessionId, sent.sessionId))
    expect(conversation).toMatchObject({
      kind: `session`,
      teamId: w.teamId,
      shared: true,
      ownerId: w.owner,
      title: null,
    })
    const [message] = await db.select().from(messages).where(eq(messages.id, sent.messageId!))
    expect(message).toMatchObject({ conversationId: conversation!.id, sessionId: sent.sessionId })
    // A child's conversation is its own, never the root's.
    const child = await sendSession({
      userId: w.owner,
      input: { parentId: sent.sessionId, text: `fan out` },
    })
    const [childConversation] = await db
      .select()
      .from(conversations)
      .where(eq(conversations.sessionId, child.sessionId))
    expect(childConversation!.id).not.toBe(conversation!.id)
    const [childMessage] = await db.select().from(messages).where(eq(messages.id, child.messageId!))
    expect(childMessage!.conversationId).toBe(childConversation!.id)
    // Ending the tree keeps the rows; deleting the session cascades them.
    await db.delete(sessions).where(eq(sessions.id, sent.sessionId))
    expect(await db.select().from(conversations).where(eq(conversations.sessionId, sent.sessionId))).toEqual([])
  })

  it(`stamps shared from the device and lets a teammate start on the shared box`, async () => {
    const w = await world({ shared: true })
    const sent = await sendSession({
      userId: w.mate,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `hello` },
    })
    const root = await row(sent.sessionId)
    expect(root).toMatchObject({ shared: true, userId: w.mate })
    // The teammate owns no row for the box: the owner sharing it hosts.
    expect(await resolveSessionHost(root)).toBe(w.owner)
    // Visible to the owner (member + shared).
    expect((await loadSessionForUser(w.owner, root.id)).id).toBe(root.id)
  })

  it(`refuses a device that is neither yours nor shared, and an unknown model`, async () => {
    const w = await world()
    await expect(
      sendSession({ userId: w.mate, input: { teamId: w.teamId, deviceId: w.deviceId, text: `x` } })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
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

  it(`NEW-2: a prompt run's first message is the body plus the text, titled after the prompt`, async () => {
    const w = await world()
    const [prompt] = await db
      .insert(prompts)
      .values({ teamId: w.teamId, name: `Triage`, body: `Triage the backlog.\n` })
      .returning({ id: prompts.id })
    await expect(
      sendSession({
        userId: w.owner,
        input: { teamId: w.teamId, deviceId: w.deviceId, promptId: crypto.randomUUID() },
      })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
    const sent = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, promptId: prompt!.id, text: `Web board only.` },
    })
    expect(await row(sent.sessionId)).toMatchObject({ title: `Triage`, actionId: null, actionName: null })
    const [first] = await db.select({ body: messages.body }).from(messages).where(eq(messages.sessionId, sent.sessionId))
    expect(first!.body).toBe(`Triage the backlog.\n\nWeb board only.`)
    const brief = await composeSessionBrief(sent.sessionId)
    expect(brief.brief).toContain(`Triage the backlog.`)
    expect(brief.brief).toContain(`Web board only.`)
    expect(brief.brief).not.toContain(`## Action`)
    // A passed name wins over the prompt's.
    const named = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, promptId: prompt!.id, name: `Tidy` },
    })
    expect(await row(named.sessionId)).toMatchObject({ title: `Tidy` })
  })
})

// NEW-1 (no dispatcher): a new root RUNS on the composer's picks; a child
// inherits its parent's.
describe.skipIf(!ready.ok)(`sessions.send — a root works on its own picks`, () => {
  it(`a new root carries the composer's picks and children inherit them`, async () => {
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
    const picks = {
      cli: `claude`,
      model: `opus`,
      effort: `high`,
      planMode: true,
      ultracode: true,
      computerUse: true,
    }
    // The legacy shims stay unwritten.
    expect(await row(root.sessionId)).toMatchObject({ ...picks, dispatch: false, spawnedBySessionId: null })

    const child = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `part 1` } })
    expect(await row(child.sessionId)).toMatchObject({ ...picks, dispatch: false })
    // The child's own picks win over the inherited ones.
    const picked = await sendSession({
      userId: w.owner,
      input: { parentId: root.sessionId, text: `part 2`, model: `haiku`, planMode: false },
    })
    expect(await row(picked.sessionId)).toMatchObject({ model: `haiku`, planMode: false, effort: `high` })
    // A grandchild inherits from ITS parent.
    const grand = await sendSession({ userId: w.owner, input: { parentId: picked.sessionId, text: `deeper` } })
    expect(await row(grand.sessionId)).toMatchObject({ model: `haiku`, planMode: false, effort: `high` })
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
    expect(await row(blankRoot.sessionId)).toMatchObject({ model: null, effort: null })

    // An ended root's continuation runs on the same picks.
    await endSession({ userId: w.owner, sessionId: root.sessionId, summary: `Done.` })
    const cont = await sendSession({ userId: w.owner, input: { sessionId: root.sessionId, text: `again` } })
    expect(await row(cont.sessionId)).toMatchObject({ parentId: root.sessionId, ...picks })
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

  it(`NEW-1: a start's issues sit on the new session; a message FORWARDS one onto its target`, async () => {
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
    const holders = async (issueId: string) =>
      (
        await db
          .select({ sessionId: sessionIssues.sessionId, assigned: sessionIssues.assignedSessionId })
          .from(sessionIssues)
          .where(eq(sessionIssues.issueId, issueId))
      )
        .map((link) => {
          // The legacy shim is never written.
          expect(link.assigned).toBeNull()
          return link.sessionId
        })
        .sort()
    // The row sits on the session that holds the issue: each start its own.
    expect(await holders(a)).toEqual([root.sessionId, grouped.sessionId].sort())
    expect(await holders(c)).toEqual([named.sessionId])

    const tree = await getSessionTree(w.owner, root.sessionId)
    expect(tree.root.displayTitle).toBe(`Perf pass`)
    const titles = Object.fromEntries(tree.children.map((s) => [s.id, s.displayTitle]))
    expect(titles[grouped.sessionId]).toMatch(/^#CR-\d+ Lag \+ 1$/)
    expect(titles[named.sessionId]).toBe(`Copy fixes`)
    // sessions_get = the union over the tree, each row naming its holder.
    expect(new Set(tree.issues.map((link) => link.issueId))).toEqual(new Set([a, b, c]))
    expect(tree.issues.find((link) => link.issueId === c)!.sessionId).toBe(named.sessionId)
    // The child's brief lists only its own rows.
    const brief = await composeSessionBrief(grouped.sessionId)
    expect(brief.brief).toContain(`Lag`)
    expect(brief.brief).not.toContain(`Typo`)

    // Forward: c's row MOVES from the named child onto the grouped one.
    await sendSession({ userId: w.owner, input: { sessionId: grouped.sessionId, issueIds: [c] } })
    expect(await holders(c)).toEqual([grouped.sessionId])
    // Forwarding what the target already holds drops the tree's other copy.
    await sendSession({ userId: w.owner, input: { sessionId: grouped.sessionId, issueIds: [a] } })
    expect(await holders(a)).toEqual([grouped.sessionId])
    expect((await composeSessionBrief(grouped.sessionId)).brief).toContain(`Typo`)
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
    const grand = await sendSession({ userId: w.owner, input: { parentId: child.sessionId, text: `g` } })
    const sibling = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `s` } })
    // A child's End ends only its subtree.
    const sub = await endSession({ userId: w.owner, sessionId: child.sessionId })
    expect(sub.endedIds).toEqual([grand.sessionId, child.sessionId])
    expect(await row(sibling.sessionId)).toMatchObject({ status: `queued` })
    expect(await row(root.sessionId)).toMatchObject({ status: `queued` })
    // A root's End ends the rest of its tree, deepest first.
    const ended = await endSession({ userId: w.owner, sessionId: root.sessionId })
    expect(ended.endedIds).toEqual([sibling.sessionId, root.sessionId])

    // NEW-2: an agent is a definition; a message to it opens the caller's
    // conversation session (persistent: End refuses).
    const [ada] = await db
      .insert(agents)
      .values({ teamId: w.teamId, name: `Ada`, icon: `rocket`, color: `#ff0000`, instructions: `Be brief.`, deviceId: w.deviceId, ownerId: w.owner })
      .returning({ id: agents.id })
    const agent = await sendSession({ userId: w.owner, input: { agentId: ada!.id, text: `hello` } })
    expect(await row(agent.sessionId)).toMatchObject({
      agentId: ada!.id,
      kind: `agent`,
      name: `Ada`,
      title: `Ada`,
      userId: w.owner,
      deviceId: w.deviceId,
      repositoryId: null,
      status: `queued`,
    })
    await expect(endSession({ userId: w.owner, sessionId: agent.sessionId })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
    })
    // The same member's next message lands on the SAME conversation.
    const again = await sendSession({ userId: w.owner, input: { agentId: ada!.id, text: `more` } })
    expect(again.sessionId).toBe(agent.sessionId)
    // A teammate has no access to an agent on an unshared device.
    await expect(sendSession({ userId: w.mate, input: { agentId: ada!.id, text: `hi` } })).rejects.toMatchObject({
      code: `NOT_FOUND`,
    })

    // The agent spawns a thread: it lands in the team, on the agent's device,
    // with a thread_started card on the conversation.
    const spawned = await sendSession({
      userId: w.owner,
      callerSessionId: agent.sessionId,
      input: { text: `look into it` },
    })
    expect(await row(spawned.sessionId)).toMatchObject({
      teamId: w.teamId,
      deviceId: w.deviceId,
      parentId: null,
      agentId: null,
      spawnedBySessionId: null,
    })
    // NEW-1: the agent WATCHES its spawn (the bus reports back to it).
    expect((await row(agent.sessionId)).wakeOn).toEqual([spawned.sessionId])
    const cards = await db
      .select({ card: messages.card })
      .from(messages)
      .where(and(eq(messages.sessionId, agent.sessionId), sql`${messages.card} IS NOT NULL`))
    expect(cards.map((c) => c.card)).toEqual([{ kind: `thread_started`, sessionId: spawned.sessionId }])
    const brief = await composeSessionBrief(agent.sessionId)
    expect(brief.agentInstructions).toBe(`Be brief.`)
  })
})

describe.skipIf(!ready.ok)(`thread issues, the Guide and the trigger mirrors`, () => {
  it(`links on the HOLDER idempotently with event cards on the root, and unlinks`, async () => {
    const w = await world()
    const a = await w.issue(`A`)
    const root = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `r` } })
    const child = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `c` } })
    await db.transaction(async (tx) => {
      const first = await linkIssues(tx, {
        sessionId: child.sessionId,
        issueIds: [a],
        source: `agent`,
        addedBy: w.owner,
        addedBySessionId: child.sessionId,
        filed: true,
      })
      const second = await linkIssues(tx, {
        sessionId: child.sessionId,
        issueIds: [a],
        source: `user`,
        addedBy: w.owner,
        addedBySessionId: null,
      })
      expect(first.linked).toEqual([a])
      expect(second.linked).toEqual([])
    })
    const links = await db.select().from(sessionIssues).where(eq(sessionIssues.issueId, a))
    expect(links).toMatchObject([{ sessionId: child.sessionId, source: `agent`, assignedSessionId: null }])
    // The root's "−" reaches the row wherever its tree holds it.
    const out = await db.transaction((tx) => unlinkIssue(tx, { sessionId: root.sessionId, issueId: a }))
    expect(out).toEqual({ unlinked: true })
    expect(await db.select().from(sessionIssues).where(eq(sessionIssues.issueId, a))).toEqual([])
    const events = await db
      .select({ card: messages.card })
      .from(messages)
      .where(and(eq(messages.sessionId, root.sessionId), eq(messages.authorKind, `system`)))
    expect(events.map((e) => e.card as { event?: string; sessionId?: string | null }).filter((c) => c.event)).toEqual([
      { kind: `event`, event: `issue_filed`, issueId: a, sessionId: child.sessionId },
      { kind: `event`, event: `issue_unlinked`, issueId: a, sessionId: null },
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

  it(`EXP-1351: the root removes an ended child's Guide entries; a no-match remove fails`, async () => {
    const w = await world()
    const root = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `r` } })
    const child = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `c` } })
    await writeGuide({ userId: w.owner, sessionId: root.sessionId, topic: `Summary`, text: `Root did X.` })
    await writeGuide({ userId: w.owner, sessionId: child.sessionId, topic: `UI`, text: `Child did Y.` })
    const picture = {
      topic: `UI`,
      label: `web: old specimen`,
      attachmentId: null,
      width: 10,
      height: 10,
      text: null,
      at: Date.now(),
      sessionId: child.sessionId,
    }
    await db
      .update(sessions)
      .set({ guide: [...(await row(root.sessionId)).guide!, picture] })
      .where(eq(sessions.id, root.sessionId))
    await db.update(sessions).set({ status: `ended` }).where(eq(sessions.id, child.sessionId))

    // A child never reaches its parent's entries.
    await expect(
      writeGuide({ userId: w.owner, sessionId: child.sessionId, topic: `Summary`, remove: true })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
    await writeGuide({ userId: w.owner, sessionId: root.sessionId, topic: `UI`, label: `web: old specimen`, remove: true })
    expect((await row(root.sessionId)).guide!.map((e) => [e.topic, e.label])).toEqual([
      [`Summary`, null],
      [`UI`, null],
    ])
    await writeGuide({ userId: w.owner, sessionId: root.sessionId, topic: `UI`, remove: true })
    expect((await row(root.sessionId)).guide).toHaveLength(1)

    const before = (await row(root.sessionId)).guide
    await expect(
      writeGuide({ userId: w.owner, sessionId: root.sessionId, topic: `UI`, label: `gone`, remove: true })
    ).rejects.toMatchObject({ code: `NOT_FOUND`, message: expect.stringContaining(`label "gone"`) })
    expect((await row(root.sessionId)).guide).toEqual(before)
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

  it(`NEW-2 agents router: owner writes, members read, a device move re-stamps the conversations`, async () => {
    const w = await world()
    const { agentsRouter } = await import(`@/lib/trpc/agents`)
    const agentsOf = (userId: string) =>
      agentsRouter.createCaller({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) } as never)
    const { agent } = await agentsOf(w.owner).create({
      teamId: w.teamId,
      name: `Ada`,
      icon: `rocket`,
      color: `#123456`,
      instructions: `Hi`,
      triggers: [{ kind: `schedule`, enabled: true, cron: `0 9 * * *`, tz: `Europe/Vienna` }],
      deviceId: w.deviceId,
      cli: `claude`,
      model: `opus`,
    })
    expect(agent.triggers[0]!.id).toMatch(/^[0-9a-f-]{36}$/)
    expect(agent).toMatchObject({ ownerId: w.owner, deviceId: w.deviceId, model: `opus` })
    await expect(
      agentsOf(w.owner).create({ teamId: w.teamId, name: `Bad`, deviceId: w.deviceId, cli: `claude`, model: `nope` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(
      agentsOf(w.owner).update({ id: agent.id, triggers: [{ kind: `schedule`, cron: `every day` } as never] })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    // A member reads, never writes.
    expect((await agentsOf(w.mate).get({ id: agent.id })).agent.instructions).toBe(`Hi`)
    expect((await agentsOf(w.mate).list({ teamId: w.teamId })).agents.map((a) => a.id)).toEqual([agent.id])
    await expect(agentsOf(w.mate).update({ id: agent.id, name: `x` })).rejects.toMatchObject({ code: `FORBIDDEN` })
    await expect(
      agentsOf(w.mate).create({ teamId: w.teamId, name: `Bo`, deviceId: w.deviceId })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })

    // The owner's conversation, then a move onto a shared box: the
    // conversation follows (device, visibility, queued again, mirrors).
    const conversation = await sendSession({ userId: w.owner, input: { agentId: agent.id, text: `hi` } })
    const sharedDevice = `dev-${id8()}`
    await db.insert(devices).values({ userId: w.owner, deviceId: sharedDevice, label: `srv`, kind: `server`, sharedTeamIds: [w.teamId] })
    await agentsOf(w.owner).update({ id: agent.id, deviceId: sharedDevice, name: `Ada 2`, account: `acc1` })
    expect(await row(conversation.sessionId)).toMatchObject({
      deviceId: sharedDevice,
      shared: true,
      status: `queued`,
      name: `Ada 2`,
      account: `acc1`,
    })
    // Now the teammate may talk to it too: their OWN conversation.
    const mates = await sendSession({ userId: w.mate, input: { agentId: agent.id, text: `hey` } })
    expect(mates.sessionId).not.toBe(conversation.sessionId)
    expect(await row(mates.sessionId)).toMatchObject({ agentId: agent.id, userId: w.mate, deviceId: sharedDevice, shared: true })
    const mine = await listSessions(w.owner, { teamId: w.teamId, agentId: agent.id, limit: 10, offset: 0 })
    expect(mine.map((r) => r.id)).toEqual([conversation.sessionId])
    expect(await listSessions(w.owner, { teamId: w.teamId, limit: 10, offset: 0 })).toEqual([])

    await agentsOf(w.owner).delete({ id: agent.id })
    expect(await db.select().from(sessions).where(eq(sessions.id, conversation.sessionId))).toEqual([])
    expect(await db.select().from(sessions).where(eq(sessions.id, mates.sessionId))).toEqual([])
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

    // NEW-1: a root runs on its own picks: configure moves them like a child's.
    await owner.configure({ sessionId: root.sessionId, model: `opus`, planMode: true, effort: `low` })
    expect(await row(root.sessionId)).toMatchObject({ dispatch: false, model: `opus`, planMode: true, effort: `low` })

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
    // Omitted = unchanged; a root follows its run like any session.
    await host.report({ sessionId: child.sessionId, busy: true })
    expect((await row(child.sessionId)).planMode).toBe(false)
    await host.report({ sessionId: root.sessionId, planMode: true })
    expect((await row(root.sessionId)).planMode).toBe(true)

    // The model the run reports (a `/model` switch) follows onto the row; a
    // value outside the agent's vocabulary is ignored, never refused.
    await host.report({ sessionId: child.sessionId, model: `haiku` })
    expect((await row(child.sessionId)).model).toBe(`haiku`)
    await host.report({ sessionId: child.sessionId, model: `claude-opus-4-1-20250805`, busy: false })
    expect(await row(child.sessionId)).toMatchObject({ model: `haiku`, busy: false })
    await host.report({ sessionId: root.sessionId, model: `opus` })
    expect((await row(root.sessionId)).model).toBe(`opus`)
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

describe.skipIf(!ready.ok)(`EXP-1323: reports, reads and suggestions`, async () => {
  const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
  const { postReplies, postSuggestions } = await import(`@/lib/sessions/suggestions`)
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

  it(`the device's ended report falls back to the child's last reply`, async () => {
    const w = await world()
    const root = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `r` } })
    const child = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `c` } })
    const host = caller(w.owner)
    await host.claim({ sessionId: child.sessionId })
    await host.reply({ sessionId: child.sessionId, body: `All done.` })
    await host.report({ sessionId: child.sessionId, status: `ended` })
    expect(await row(child.sessionId)).toMatchObject({ status: `ended`, busy: false })
    const reports = (await sessionMessages(root.sessionId)).filter(
      (m) => (m.card as { kind?: string } | null)?.kind === `report`
    )
    expect(reports.map((m) => m.body)).toEqual([`All done.`])
    // A late report of an ended row is a no-op: the report is posted once.
    await host.report({ sessionId: child.sessionId, status: `ended` })
    expect(
      (await sessionMessages(root.sessionId)).filter((m) => (m.card as { kind?: string } | null)?.kind === `report`)
    ).toHaveLength(1)
    // With neither a summary nor a reply: the fallback.
    const { REPORT_FALLBACK } = await import(`@/lib/sessions/bus`)
    const quiet = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `q` } })
    await host.claim({ sessionId: quiet.sessionId })
    await host.report({ sessionId: quiet.sessionId, status: `ended` })
    const fallback = (await sessionMessages(root.sessionId)).find(
      (m) => (m.card as { sessionId?: string; kind?: string } | null)?.kind === `report` &&
        (m.card as { sessionId?: string }).sessionId === quiet.sessionId
    )
    expect(fallback!.body).toBe(REPORT_FALLBACK)
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

  it(`EXP-1211 quick replies: a tap sends the reply here, dismiss stamps the rest`, async () => {
    const w = await world()
    const root = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `r` },
    })
    const posted = await postReplies({
      userId: w.owner,
      callerSessionId: root.sessionId,
      replies: [`Raise the version numbers`, `Open the PR`, `Merge it`],
    })
    const [card] = await db.select().from(messages).where(eq(messages.id, posted.messageId))
    expect(card).toMatchObject({ authorKind: `agent`, authorSessionId: root.sessionId })
    expect(card!.consumedAt).not.toBeNull()
    const [a, b] = posted.itemIds as [string, string, string]
    const host = caller(w.owner)

    const sent = await host.sendReply({ messageId: posted.messageId, itemId: a })
    expect(sent.sessionId).toBe(root.sessionId)
    expect(
      (await sessionMessages(root.sessionId)).some(
        (m) => m.body === `Raise the version numbers` && m.authorKind === `user`
      )
    ).toBe(true)
    await expect(host.sendReply({ messageId: posted.messageId, itemId: a })).rejects.toMatchObject({
      code: `CONFLICT`,
    })
    // A teammate on a private session cannot tap it.
    await expect(caller(w.mate).sendReply({ messageId: posted.messageId, itemId: b })).rejects.toMatchObject({
      code: `NOT_FOUND`,
    })
    await host.dismissReplies({ messageId: posted.messageId })
    const [stamped] = await db.select().from(messages).where(eq(messages.id, posted.messageId))
    const items = (stamped!.card as unknown as { items: Array<Record<string, unknown>> }).items
    expect(items.map((item) => item.sent)).toEqual([true, true, true])
    await expect(host.sendReply({ messageId: posted.messageId, itemId: b })).rejects.toMatchObject({
      code: `CONFLICT`,
    })
  })
  })


// ── EXP-1252/NEW-1: moving a session to another device (+ #FEED-84 / #FEED-85) ────

const signedIn = (id: string, email: string) => ({
  claude: { signedIn: true, email, profiles: [{ id, signedIn: true, email, active: true }] },
})

describe.skipIf(!ready.ok)(`NEW-1: moving a session to another device`, async () => {
  const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
  const { MOVE_COPY } = await import(`@/lib/sessions/move`)
  const caller = (userId: string) =>
    sessionsRouter.createCaller({
      session: { user: { id: userId } },
      db,
      request: new Request(`http://x.test`),
    } as never)

  /** The owner's second machine, online, signed in to claude; a CLAIMED root. */
  async function moveWorld() {
    const w = await world()
    await db
      .update(devices)
      .set({ agentAccounts: signedIn(`pa`, `me@move.test`), version: `0.14.74` })
      .where(eq(devices.deviceId, w.deviceId))
    const target = `dev-b-${id8()}`
    await db.insert(devices).values({
      userId: w.owner,
      deviceId: target,
      label: `b-box`,
      kind: `desktop`,
      version: `0.14.74`,
      agentAccounts: signedIn(`pb`, `ME@move.test`),
    })
    const root = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, cli: `claude`, account: `pa`, text: `work` },
    })
    await caller(w.owner).claim({ sessionId: root.sessionId, deviceId: w.deviceId, branch: `exp/move` })
    return { ...w, target, sessionId: root.sessionId, firstMessageId: root.messageId! }
  }
  const systemBodies = async (sessionId: string) =>
    (
      await db
        .select()
        .from(messages)
        .where(and(eq(messages.sessionId, sessionId), eq(messages.authorKind, `system`)))
    ).map((m) => m.body)

  it(`a QUEUED row re-targets in place, carrying the login, and says so (pre-consumed)`, async () => {
    const w = await moveWorld()
    const fresh = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, cli: `claude`, account: `pa`, text: `not yet` },
    })
    const sent = await sendSession({ userId: w.owner, input: { sessionId: fresh.sessionId, deviceId: w.target } })
    expect(sent).toMatchObject({ sessionId: fresh.sessionId, messageId: null, status: `queued` })
    // The same address on the target carries (case-insensitive).
    expect(await row(fresh.sessionId)).toMatchObject({
      deviceId: w.target,
      status: `queued`,
      account: `pb`,
      moveToDeviceId: null,
    })
    const [moved] = await db
      .select()
      .from(messages)
      .where(and(eq(messages.sessionId, fresh.sessionId), eq(messages.body, `Moved from box to b-box`)))
    expect(moved).toMatchObject({ authorKind: `system`, card: null })
    expect(moved!.consumedAt).not.toBeNull()
    // The current device = nothing at all.
    await sendSession({ userId: w.owner, input: { sessionId: fresh.sessionId, deviceId: w.target, text: `hi` } })
    expect(await row(fresh.sessionId)).toMatchObject({ deviceId: w.target, status: `queued` })
  })

  it(`a LIVE row ends and continues on the target on its branch; children and watchers follow`, async () => {
    const w = await moveWorld()
    const issueId = await w.issue(`Moving work`)
    await sendSession({ userId: w.owner, input: { sessionId: w.sessionId, issueIds: [issueId] } })
    const kid = await sendSession({ userId: w.owner, input: { parentId: w.sessionId, text: `kid` } })
    // A second root watching the moving one.
    const watcher = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `w` } })
    await sendSession({
      userId: w.owner,
      callerSessionId: watcher.sessionId,
      viaMcp: true,
      input: { sessionId: watcher.sessionId, wakeOn: [w.sessionId] },
    })
    expect((await row(watcher.sessionId)).wakeOn).toEqual([w.sessionId])

    const sent = await sendSession({
      userId: w.owner,
      input: { sessionId: w.sessionId, deviceId: w.target, text: `carry on there` },
    })
    expect(sent.sessionId).not.toBe(w.sessionId)
    expect(await row(w.sessionId)).toMatchObject({ status: `ended`, busy: false, deviceId: w.deviceId })
    expect(await row(sent.sessionId)).toMatchObject({
      rootId: w.sessionId,
      deviceId: w.target,
      branch: `exp/move`,
      account: `pb`,
      status: `queued`,
    })
    expect(await systemBodies(w.sessionId)).toContain(`Moved from box to b-box`)
    // The message waits on the continuation.
    const [pending] = await db.select().from(messages).where(eq(messages.id, sent.messageId!))
    expect(pending).toMatchObject({ sessionId: sent.sessionId, body: `carry on there`, consumedAt: null })
    // Its live child re-parents, its watcher follows, its issue rows move.
    expect(await row(kid.sessionId)).toMatchObject({ parentId: sent.sessionId, status: `queued` })
    expect((await row(watcher.sessionId)).wakeOn).toEqual([sent.sessionId])
    const [link] = await db.select().from(sessionIssues).where(eq(sessionIssues.issueId, issueId))
    expect(link!.sessionId).toBe(sent.sessionId)
    // A moved ROOT continues as a child of itself, never of its own
    // continuation (the children's re-parent must skip the new row).
    expect((await row(sent.sessionId)).parentId).toBe(w.sessionId)
  })

  it(`refuses with the client copy: offline, visibility, no login, agent; an ended thread continues there`, async () => {
    const w = await moveWorld()
    const add = async (values: Partial<typeof devices.$inferInsert>) => {
      const deviceId = `dev-x-${id8()}`
      await db.insert(devices).values({
        userId: w.owner,
        deviceId,
        label: `x`,
        kind: `server`,
        version: `0.14.74`,
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
    const mateDevice = `dev-m-${id8()}`
    await db.insert(devices).values({ userId: w.mate, deviceId: mateDevice, label: `m`, kind: `desktop` })
    await refuse(mateDevice, MOVE_COPY.visibility)
    // An explicit login must be one of the target's.
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: w.sessionId, deviceId: w.target, account: `nope` } })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    // Nothing moved.
    expect(await row(w.sessionId)).toMatchObject({ status: `running`, deviceId: w.deviceId })

    const [bot] = await db
      .insert(agents)
      .values({ teamId: w.teamId, name: `Bot`, deviceId: w.deviceId, ownerId: w.owner })
      .returning({ id: agents.id })
    const agent = await sendSession({ userId: w.owner, input: { agentId: bot!.id, text: `hi` } })
    await refuse(w.target, MOVE_COPY.agent, agent.sessionId)
    // An ENDED thread: a bare device pick has nothing to run; with a
    // message the continuation starts ON the named device.
    await endSession({ userId: w.owner, sessionId: w.sessionId })
    await expect(sendSession({ userId: w.owner, input: { sessionId: w.sessionId, deviceId: w.target } })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: `This session has ended: send a message to continue it`,
    })
    const cont = await sendSession({
      userId: w.owner,
      input: { sessionId: w.sessionId, deviceId: w.target, text: `go on` },
    })
    expect(cont.sessionId).not.toBe(w.sessionId)
    expect(await row(cont.sessionId)).toMatchObject({
      parentId: w.sessionId,
      deviceId: w.target,
      branch: `exp/move`,
      status: `queued`,
    })
  })

  it(`a claim guards the device it read the row on: a re-target in between is CONFLICT (claim_lost)`, async () => {
    const w = await moveWorld()
    const fresh = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, cli: `claude`, text: `not yet` },
    })
    // The daemon's claim reads the row on A; a person's direct re-target to
    // B commits before its write.
    const raced = sessionsRouter.createCaller({
      session: { user: { id: w.owner } },
      request: new Request(`http://x.test`),
      db: {
        transaction: async (fn: Parameters<typeof db.transaction>[0]) => {
          await sendSession({ userId: w.owner, input: { sessionId: fresh.sessionId, deviceId: w.target } })
          return db.transaction(fn)
        },
      },
    } as never)
    // A 0.14.72 daemon sends no `deviceId`: the snapshot's device stands in.
    await expect(raced.claim({ sessionId: fresh.sessionId })).rejects.toMatchObject({ code: `CONFLICT` })
    expect(await row(fresh.sessionId)).toMatchObject({ deviceId: w.target, status: `queued`, startedAt: null })
    // B's claim takes it.
    await caller(w.owner).claim({ sessionId: fresh.sessionId, deviceId: w.target })
    expect(await row(fresh.sessionId)).toMatchObject({ deviceId: w.target, status: `running` })
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
    // NEW-1: the continuation TAKES OVER the ended child's rows.
    const links = await db.select().from(sessionIssues).where(eq(sessionIssues.issueId, issueId))
    expect(links).toMatchObject([{ sessionId: cont.sessionId, assignedSessionId: null }])
    // An ended ROOT keeps continuing as its own child.
    await endSession({ userId: w.owner, sessionId: root.sessionId })
    const rootCont = await sendSession({ userId: w.owner, input: { sessionId: root.sessionId, text: `again` } })
    expect(await row(rootCont.sessionId)).toMatchObject({ parentId: root.sessionId, depth: 1, branch: null })
  })

  it(`#FEED-84: sessions_list and sessions_get show the agent and its login`, async () => {
    const w = await moveWorld()
    const [listed] = await listSessions(w.owner, { teamId: w.teamId, limit: 10, offset: 0 })
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

  it(`#FEED-85: idle clears busy, Stop frees a ghost`, async () => {
    const { stopSession } = await import(`@/lib/sessions/lifecycle`)
    const w = await moveWorld()
    const host = caller(w.owner)
    await host.report({ sessionId: w.sessionId, busy: true })
    expect((await row(w.sessionId)).busy).toBe(true)
    await host.report({ sessionId: w.sessionId, status: `idle` })
    expect(await row(w.sessionId)).toMatchObject({ status: `idle`, busy: false })

    // Stop on a running row whose device is offline frees it.
    await db
      .update(devices)
      .set({ lastSeenAt: new Date(Date.now() - 3 * 3_600_000) })
      .where(eq(devices.deviceId, w.deviceId))
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
    await db.update(sessions).set({ planMode: true }).where(eq(sessions.id, sent.sessionId))
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
