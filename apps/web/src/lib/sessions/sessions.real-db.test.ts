// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, inArray, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  agents,
  attachments,
  boards,
  branchPushes,
  conversations,
  devices,
  apps,
  events,
  issues,
  mcpServers,
  messages,
  prompts,
  pullRequests,
  repositories,
  installs,
  sessionIssues,
  sessionKeys,
  sessions,
  teamMembers,
  teams,
  userVoiceSettings,
} from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { inSession, messageWithSession } from "@/lib/sessions/messages"
import { createTreeKeyInTx } from "@/lib/sessions/tree-keys"

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
  notifySessionAsk: async () => {},
  fireAndForgetSessionMessageNotify: () => {},
  notifySessionBlocked: notifyBlocked,
  // EXP-1328: File as issue runs issues.create.
  fireAndForgetAssignmentNotify: () => {},
  fireAndForgetIssueMentionNotify: () => {},
}))

// The object store (never reached here: card pictures arrive as rows).
vi.mock(`@/lib/storage`, () => ({
  uploadObject: async () => {},
  copyObject: async () => {},
  deleteObject: async () => {},
  getObject: async () => ({ Body: {}, ContentLength: 3 }),
  toResponseBody: async () => new Uint8Array([1, 2, 3]).buffer,
}))
vi.mock(`@/lib/storage/issue-attachment-cleanup`, () => ({
  deleteStorageObjects: async () => {},
}))
// Chats: `sessions.stop`'s `delivered` with a relay configured (off = the
// real module).
const relay = vi.hoisted(() => ({ on: false, delivered: true, interrupted: [] as string[] }))
vi.mock(`@/lib/steer`, async (importOriginal) => {
  const real = await importOriginal<typeof import("@/lib/steer")>()
  return {
    ...real,
    getSteerRelayConfig: () => (relay.on ? { url: `http://relay.test`, secret: `s` } : real.getSteerRelayConfig()),
    relayPostInterrupt: async (...args: Parameters<typeof real.relayPostInterrupt>) => {
      if (!relay.on) return real.relayPostInterrupt(...args)
      relay.interrupted.push(args[1])
      return { delivered: relay.delivered }
    },
    relayPostKill: async (...args: Parameters<typeof real.relayPostKill>) =>
      relay.on ? undefined : real.relayPostKill(...args),
  }
})

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

// #EXP-1540: a branch's latest push (`branch_pushes`).
const latestPush = async (repositoryId: string, branch: string) =>
  (
    await db
      .select()
      .from(branchPushes)
      .where(and(eq(branchPushes.repositoryId, repositoryId), eq(branchPushes.branch, branch)))
  )[0]
const row = async (id: string) =>
  (await db.select().from(sessions).where(eq(sessions.id, id)).limit(1))[0]!

describe.skipIf(!ready.ok)(`sessions.send — a new root`, () => {
  it(`queues a private chat on the caller's own device with its issues and first message`, async () => {
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
    const [message] = await db.select(messageWithSession()).from(messages).where(eq(messages.id, sent.messageId!))
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
    const [message] = await db.select(messageWithSession()).from(messages).where(eq(messages.id, sent.messageId!))
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
    const [message] = await db.select(messageWithSession()).from(messages).where(eq(messages.id, sent.messageId!))
    expect(message!.body).toBe(`look\n\n![shot.png](/api/attachments/${upload!.id})`)
    // #EXP-1682: sending it again FORWARDS a copy (the upload stays put).
    const again = await sendSession({
      userId: w.owner,
      input: { sessionId: sent.sessionId, attachmentIds: [upload!.id] },
    })
    const [forwarded] = await db.select(messageWithSession()).from(messages).where(eq(messages.id, again.messageId!))
    expect(forwarded!.attachmentIds).toHaveLength(1)
    expect(forwarded!.attachmentIds![0]).not.toBe(upload!.id)
    expect((await db.select().from(attachments).where(eq(attachments.id, upload!.id)))[0]!.messageId).toBe(sent.messageId)
    // Someone else's unsent upload is still refused.
    const mate = `mate-${id8()}`
    await db.execute(sql`
      INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
      VALUES (${mate}, 'Mate', ${`${mate}@x.test`}, true, false, now(), now())
    `)
    const [theirs] = await db
      .insert(attachments)
      .values({ teamId: w.teamId, uploaderId: mate, filename: `t.png`, contentType: `image/png`, sizeBytes: 3, storageKey: `k-${id8()}`, url: `x` })
      .returning({ id: attachments.id })
    await expect(
      sendSession({
        userId: w.owner,
        input: { sessionId: sent.sessionId, attachmentIds: [theirs!.id] },
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await db.execute(sql`DELETE FROM users WHERE id = ${mate}`)
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
    expect(await row(sent.sessionId)).toMatchObject({ title: `Triage` })
    const [first] = await db.select({ body: messages.body }).from(messages).where(inSession(sent.sessionId))
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
        ultracode: true,
        computerUse: true,
        text: `build it`,
      },
    })
    const picks = {
      cli: `claude`,
      model: `opus`,
      effort: `high`,
      ultracode: true,
      computerUse: true,
    }
    expect(await row(root.sessionId)).toMatchObject(picks)

    const child = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `part 1` } })
    expect(await row(child.sessionId)).toMatchObject(picks)
    // The child's own picks win over the inherited ones.
    const picked = await sendSession({
      userId: w.owner,
      input: { parentId: root.sessionId, text: `part 2`, model: `haiku` },
    })
    expect(await row(picked.sessionId)).toMatchObject({ model: `haiku`, effort: `high` })
    // A grandchild inherits from ITS parent.
    const grand = await sendSession({ userId: w.owner, input: { parentId: picked.sessionId, text: `deeper` } })
    expect(await row(grand.sessionId)).toMatchObject({ model: `haiku`, effort: `high` })
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

    // FEED-134: an ended root revives on the same picks (the same row).
    await endSession({ userId: w.owner, sessionId: root.sessionId, summary: `Done.` })
    const cont = await sendSession({ userId: w.owner, input: { sessionId: root.sessionId, text: `again` } })
    expect(cont.sessionId).toBe(root.sessionId)
    expect(await row(root.sessionId)).toMatchObject({ parentId: null, status: `idle`, ...picks })
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
      .where(and(inSession(root.sessionId), eq(messages.authorKind, `system`)))
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

  // A private tree's child on a teammate's shared box would have that
  // box's daemon host prompts the teammate's account cannot read.
  it(`a private tree's child runs on the caller's own device, never a teammate's shared box`, async () => {
    const w = await world()
    const mateBox = `dev-${id8()}`
    await db
      .insert(devices)
      .values({ userId: w.mate, deviceId: mateBox, label: `mate box`, kind: `server`, sharedTeamIds: [w.teamId] })
    const root = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `root` },
    })
    expect((await row(root.sessionId)).shared).toBe(false)
    await expect(
      sendSession({ userId: w.owner, input: { parentId: root.sessionId, deviceId: mateBox, text: `sub` } })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    const own = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `sub` } })
    expect(await row(own.sessionId)).toMatchObject({ deviceId: w.deviceId, shared: false })
    // A shared tree fans out onto the shared box.
    const shared = await sendSession({
      userId: w.mate,
      input: { teamId: w.teamId, deviceId: mateBox, text: `shared root` },
    })
    const child = await sendSession({
      userId: w.owner,
      input: { parentId: shared.sessionId, deviceId: mateBox, text: `sub` },
    })
    expect(await row(child.sessionId)).toMatchObject({ deviceId: mateBox, shared: true })
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
          .select({ sessionId: sessionIssues.sessionId })
          .from(sessionIssues)
          .where(eq(sessionIssues.issueId, issueId))
      )
        .map((link) => link.sessionId)
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

  it(`a message to a live session lands on it; to an ended chat it revives the SAME row (FEED-134)`, async () => {
    const w = await world()
    const issueId = await w.issue(`Kept`)
    const root = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `first`, issueIds: [issueId] },
    })
    const again = await sendSession({ userId: w.owner, input: { sessionId: root.sessionId, text: `more` } })
    expect(again.sessionId).toBe(root.sessionId)
    const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
    const host = sessionsRouter.createCaller({
      session: { user: { id: w.owner } },
      db,
      request: new Request(`http://x.test`),
    } as never)
    await host.claim({ sessionId: root.sessionId, deviceId: w.deviceId, branch: `exp/kept` })
    await host.report({ sessionId: root.sessionId, status: `idle`, needsInput: true })

    const ended = await endSession({ userId: w.owner, sessionId: root.sessionId, summary: `Done.` })
    expect(ended.endedIds).toEqual([root.sessionId])
    const before = await row(root.sessionId)
    expect(before).toMatchObject({ status: `ended`, busy: false, needsInput: false })
    expect(before.endedAt).not.toBeNull()

    // The same row, idle again with the message pending: its startedAt,
    // branch, title and issue rows stay; no child is spawned.
    const cont = await sendSession({ userId: w.owner, input: { sessionId: root.sessionId, text: `one more` } })
    expect(cont).toMatchObject({ sessionId: root.sessionId, status: `idle` })
    const revived = await row(root.sessionId)
    expect(revived).toMatchObject({
      status: `idle`,
      endedAt: null,
      busy: false,
      needsInput: false,
      blocked: null,
      branch: `exp/kept`,
      startedAt: before.startedAt,
      deviceId: w.deviceId,
    })
    expect(await db.select().from(sessions).where(eq(sessions.rootId, root.sessionId))).toHaveLength(1)
    const [pending] = await db.select(messageWithSession()).from(messages).where(eq(messages.id, cont.messageId!))
    expect(pending).toMatchObject({ sessionId: root.sessionId, rootId: root.sessionId, body: `one more`, consumedAt: null })
    const links = await db.select().from(sessionIssues).where(eq(sessionIssues.issueId, issueId))
    expect(links).toMatchObject([{ sessionId: root.sessionId }])

    // The device's `ended` report of a ROOT (its process went away: an app
    // quit, a crash) reads idle; only a person's End ends it. A CHILD's
    // still ends it.
    await host.report({ sessionId: root.sessionId, status: `running`, busy: true })
    await host.report({ sessionId: root.sessionId, status: `ended` })
    expect(await row(root.sessionId)).toMatchObject({ status: `idle`, busy: false, endedAt: null })
    const child = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `c` } })
    await host.claim({ sessionId: child.sessionId, deviceId: w.deviceId })
    await host.report({ sessionId: child.sessionId, status: `ended` })
    expect(await row(child.sessionId)).toMatchObject({ status: `ended` })
    expect(await row(root.sessionId)).toMatchObject({ status: `idle` })
  })

  it(`a root's \`ended\` report before its first turn ends it; after a turn it reads idle (FEED-134)`, async () => {
    // Release train 2026-10-23: a refused launch reports `ended` for a root
    // that never ran a turn; it must end, or the watchdog nudges it forever.
    const w = await world()
    const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
    const host = sessionsRouter.createCaller({
      session: { user: { id: w.owner } },
      db,
      request: new Request(`http://x.test`),
    } as never)
    const refused = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `refused` },
    })
    await host.claim({ sessionId: refused.sessionId, deviceId: w.deviceId })
    expect(await row(refused.sessionId)).toMatchObject({ lastTurnAt: null })
    await host.report({ sessionId: refused.sessionId, status: `ended` })
    expect(await row(refused.sessionId)).toMatchObject({ status: `ended` })

    const ran = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `ran` },
    })
    await host.claim({ sessionId: ran.sessionId, deviceId: w.deviceId })
    await host.report({ sessionId: ran.sessionId, status: `running`, busy: true })
    await host.report({ sessionId: ran.sessionId, busy: false })
    expect((await row(ran.sessionId)).lastTurnAt).not.toBeNull()
    await host.report({ sessionId: ran.sessionId, status: `ended` })
    expect(await row(ran.sessionId)).toMatchObject({ status: `idle`, busy: false, endedAt: null })
  })

  it(`an account on a message switches the live session's login; alone it posts no message`, async () => {
    const w = await world()
    const root = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, cli: `claude`, account: `aaaa1111`, text: `first` },
    })
    const before = await db.select().from(messages).where(inSession(root.sessionId))

    // Account-only: the row's login moves (the device acts on it), no bubble.
    const alone = await sendSession({ userId: w.owner, input: { sessionId: root.sessionId, account: `bbbb2222` } })
    expect(alone).toMatchObject({ sessionId: root.sessionId, messageId: null })
    expect(alone.txid).toBeTypeOf(`number`)
    expect(await row(root.sessionId)).toMatchObject({ account: `bbbb2222`, status: `queued` })
    const after = await db.select().from(messages).where(inSession(root.sessionId))
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

    // An ended chat: a switch alone is refused; with a message the
    // revived root runs on the new login (FEED-134).
    await endSession({ userId: w.owner, sessionId: root.sessionId, summary: `Done.` })
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: root.sessionId, account: `eeee5555` } })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    const cont = await sendSession({
      userId: w.owner,
      input: { sessionId: root.sessionId, account: `eeee5555`, text: `again` },
    })
    expect(cont.sessionId).toBe(root.sessionId)
    expect(await row(root.sessionId)).toMatchObject({ account: `eeee5555`, status: `idle`, endedAt: null })
  })

  // EXP-1376 (FEED-152): a message's pickers apply to the session, a
  // blocked one included; a pick that cannot lift the wall refuses.
  it(`a message's model and pickers apply to an existing session, through a usage wall`, async () => {
    const w = await world()
    const start = (text: string) =>
      sendSession({
        userId: w.owner,
        input: { teamId: w.teamId, deviceId: w.deviceId, cli: `claude`, account: `aaaa1111`, model: `fable`, text },
      })
    const wall = (window: string) => ({
      kind: `rate_limit`,
      agent: `claude`,
      window,
      resetsAt: `2026-10-29T00:00:00.000Z`,
      since: `2026-10-26T00:00:00.000Z`,
    })
    const walled = async (id: string, window: string) =>
      db.update(sessions).set({ status: `idle`, blocked: wall(window), needsInput: true }).where(eq(sessions.id, id))

    // A live session: model + effort land with no message, never dropped.
    const live = await start(`live`)
    const alone = await sendSession({
      userId: w.owner,
      input: { sessionId: live.sessionId, model: `opus`, effort: `high` },
    })
    expect(alone.messageId).toBeNull()
    expect(await row(live.sessionId)).toMatchObject({ model: `opus`, effort: `high`, account: `aaaa1111` })
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: live.sessionId, model: `gpt-5` } })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })

    // The release-coordinator case: a session wall, account + model in one
    // call. Both move; the host lifts the wall as it switches logins.
    const coord = await start(`coordinate`)
    await walled(coord.sessionId, `session`)
    const both = await sendSession({
      userId: w.owner,
      input: { sessionId: coord.sessionId, account: `bbbb2222`, model: `opus`, text: `carry on` },
    })
    expect(both.messageId).not.toBeNull()
    expect(await row(coord.sessionId)).toMatchObject({ account: `bbbb2222`, model: `opus` })
    expect((await row(coord.sessionId)).blocked).toMatchObject({ window: `session` })

    // A model alone cannot lift a login's wall: refused, nothing moves.
    const login = await start(`login wall`)
    await walled(login.sessionId, `weekly`)
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: login.sessionId, model: `opus` } })
    ).rejects.toThrow(/a model change does not lift: pass account/)
    expect(await row(login.sessionId)).toMatchObject({ model: `fable`, blocked: wall(`weekly`) })
    // A non-model pick (effort) still applies behind the wall.
    await sendSession({ userId: w.owner, input: { sessionId: login.sessionId, effort: `low` } })
    expect(await row(login.sessionId)).toMatchObject({ effort: `low`, blocked: wall(`weekly`) })

    // A MODEL wall: the same model refuses, another lifts it at once (the
    // queued messages run on it).
    const model = await start(`model wall`)
    await walled(model.sessionId, `model`)
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: model.sessionId, model: `fable` } })
    ).rejects.toThrow(/fable usage limit/)
    await sendSession({ userId: w.owner, input: { sessionId: model.sessionId, model: `opus` } })
    expect(await row(model.sessionId)).toMatchObject({ model: `opus`, blocked: null, needsInput: false })

    // An agent's `/model <alias>` over MCP is the pick, never a turn.
    const child = await sendSession({ userId: w.owner, input: { parentId: live.sessionId, text: `kid` } })
    const viaAgent = await start(`agent model`)
    await walled(viaAgent.sessionId, `model`)
    const sent = await sendSession({
      userId: w.owner,
      callerSessionId: child.sessionId,
      viaMcp: true,
      input: { sessionId: viaAgent.sessionId, text: `/model sonnet` },
    })
    expect(sent.messageId).toBeNull()
    expect(await row(viaAgent.sessionId)).toMatchObject({ model: `sonnet`, blocked: null })
    await expect(
      sendSession({
        userId: w.owner,
        callerSessionId: child.sessionId,
        viaMcp: true,
        input: { sessionId: viaAgent.sessionId, text: `/model gpt-5` },
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    // EXP-1401: a PERSON's `/model <alias>` (any client's ModelPicker) is
    // the same pick, never a turn: on a model-walled session it lifts the
    // wall at once; the same model refuses, saying what lifts it.
    await walled(viaAgent.sessionId, `model`)
    const person = await sendSession({ userId: w.owner, input: { sessionId: viaAgent.sessionId, text: `/model haiku` } })
    expect(person.messageId).toBeNull()
    expect(await row(viaAgent.sessionId)).toMatchObject({ model: `haiku`, blocked: null, needsInput: false })
    await walled(viaAgent.sessionId, `model`)
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: viaAgent.sessionId, text: `/model haiku` } })
    ).rejects.toThrow(/haiku usage limit/)
    await walled(viaAgent.sessionId, `weekly`)
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: viaAgent.sessionId, text: `/model opus` } })
    ).rejects.toThrow(/a model change does not lift: pass account/)
    // A person's bare `/model` stays their message (the CLI's own menu).
    const bare = await sendSession({ userId: w.owner, input: { sessionId: viaAgent.sessionId, text: `/model` } })
    expect(bare.messageId).not.toBeNull()

    // An ended child continues on the message's pickers.
    await endSession({ userId: w.owner, sessionId: child.sessionId, summary: `Done.` })
    const cont = await sendSession({
      userId: w.owner,
      input: { sessionId: child.sessionId, model: `haiku`, effort: `max`, text: `again` },
    })
    expect(cont.sessionId).not.toBe(child.sessionId)
    expect(await row(cont.sessionId)).toMatchObject({ model: `haiku`, effort: `max` })

    // Review: text with the model it already runs on queues behind a login
    // wall (no pick, no refusal).
    const queuedBehind = await sendSession({
      userId: w.owner,
      input: { sessionId: login.sessionId, model: `fable`, text: `when it resets` },
    })
    expect(queuedBehind.messageId).not.toBeNull()
    expect((await row(login.sessionId)).blocked).toEqual(wall(`weekly`))

    // Review: the clear lifts only a MODEL wall (a session/weekly wall the
    // device reported in between is never wiped).
    const { clearBlockedInTx } = await import(`@/lib/sessions/watchdog`)
    await db.transaction((tx) => clearBlockedInTx(tx, [login.sessionId], `rate_limit`, `model`))
    expect((await row(login.sessionId)).blocked).toEqual(wall(`weekly`))

    // Review: a switch names a login signed in for the agent on the device.
    await db
      .update(devices)
      .set({
        agentAccounts: {
          claude: {
            signedIn: true,
            email: `a@x.test`,
            health: `ok`,
            profiles: [
              { id: `aaaa1111`, signedIn: true, active: true, email: `a@x.test`, health: `ok` },
              { id: `cccc3333`, signedIn: true, email: `c@x.test`, health: `ok` },
            ],
          },
        },
      })
      .where(eq(devices.deviceId, w.deviceId))
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: live.sessionId, account: `zzzz9999`, model: `opus` } })
    ).rejects.toThrow(/Account "zzzz9999" is not signed in for claude on this session's device/)
    expect((await row(live.sessionId)).account).toBe(`aaaa1111`)
    await sendSession({ userId: w.owner, input: { sessionId: live.sessionId, account: `cccc3333` } })
    expect((await row(live.sessionId)).account).toBe(`cccc3333`)
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

    // The agent spawns a chat: it lands in the team, on the agent's device,
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
    })
    // NEW-1: the agent WATCHES its spawn (the bus reports back to it).
    expect((await row(agent.sessionId)).wakeOn).toEqual([spawned.sessionId])
    const cards = await db
      .select({ card: messages.card })
      .from(messages)
      .where(and(inSession(agent.sessionId), sql`${messages.card} IS NOT NULL`))
    expect(cards.map((c) => c.card)).toEqual([{ kind: `thread_started`, sessionId: spawned.sessionId }])
    const brief = await composeSessionBrief(agent.sessionId)
    expect(brief.agentInstructions).toBe(`Be brief.`)
    // Only the member's conversation with their VOICE agent is the voice
    // conversation; the chat it spawned never is.
    expect(brief.voicePolicy).toBe(false)
    await db.insert(userVoiceSettings).values({ userId: w.owner, agentId: ada!.id })
    expect((await composeSessionBrief(agent.sessionId)).voicePolicy).toBe(true)
    expect((await composeSessionBrief(spawned.sessionId)).voicePolicy).toBe(false)
    // NEW-58 (screen-memory.json TOOL): screen memory only while the voice
    // conversation is private: the agent's box shared with no team (ANY
    // team), the conversation not `shared`; never a spawned chat.
    expect((await composeSessionBrief(agent.sessionId)).screenMemory).toBe(true)
    expect((await composeSessionBrief(spawned.sessionId)).screenMemory).toBe(false)
    const elsewhere = await world()
    await db.update(devices).set({ sharedTeamIds: [elsewhere.teamId] }).where(eq(devices.deviceId, w.deviceId))
    expect(await composeSessionBrief(agent.sessionId)).toMatchObject({ voicePolicy: true, screenMemory: false })
    await db.update(devices).set({ sharedTeamIds: [] }).where(eq(devices.deviceId, w.deviceId))
    await db.update(sessions).set({ shared: true }).where(eq(sessions.id, agent.sessionId))
    expect(await composeSessionBrief(agent.sessionId)).toMatchObject({ voicePolicy: true, screenMemory: false })
    await db.update(sessions).set({ shared: false }).where(eq(sessions.id, agent.sessionId))
    // A teammate whose voice agent lives on the owner's shared box never
    // reads the owner's screen.
    await db.update(devices).set({ sharedTeamIds: [w.teamId] }).where(eq(devices.deviceId, w.deviceId))
    const mates = await sendSession({ userId: w.mate, input: { agentId: ada!.id, text: `hi` } })
    await db.insert(userVoiceSettings).values({ userId: w.mate, agentId: ada!.id })
    expect(await composeSessionBrief(mates.sessionId)).toMatchObject({ voicePolicy: true, screenMemory: false })
    await db.update(devices).set({ sharedTeamIds: [] }).where(eq(devices.deviceId, w.deviceId))
    await db.delete(userVoiceSettings).where(inArray(userVoiceSettings.userId, [w.owner, w.mate]))
  })
})

describe.skipIf(!ready.ok)(`chat issues, the Guide and the trigger mirrors`, () => {
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
    expect(links).toMatchObject([{ sessionId: child.sessionId, source: `agent` }])
    // The root's "−" reaches the row wherever its tree holds it.
    const out = await db.transaction((tx) => unlinkIssue(tx, { sessionId: root.sessionId, issueId: a }))
    expect(out).toEqual({ unlinked: true })
    expect(await db.select().from(sessionIssues).where(eq(sessionIssues.issueId, a))).toEqual([])
    const events = await db
      .select({ card: messages.card })
      .from(messages)
      .where(and(inSession(root.sessionId), eq(messages.authorKind, `system`)))
      .orderBy(messages.createdAt, messages.id)
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

  // EXP-1397 (FEED-150/151): ownership across the chat.
  it(`EXP-1397: a writer's open PR tags its topics, a parent tags a child's, a continuation owns its predecessor's`, async () => {
    const w = await world()
    const root = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `r` } })
    const child = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `c` } })
    await db.update(sessions).set({ branch: `exp/session-child` }).where(eq(sessions.id, child.sessionId))
    const [repo] = await db
      .insert(repositories)
      .values({ teamId: w.teamId, fullName: `o/r` })
      .returning({ id: repositories.id })
    const pr = async (number: number, sessionId: string, state: `open` | `merged` = `open`) => {
      const url = `https://github.com/o/r/pull/${number}`
      await db.insert(pullRequests).values({
        teamId: w.teamId,
        rootSessionId: root.sessionId,
        sessionId,
        repositoryId: repo!.id,
        number,
        url,
        // #EXP-1402: the default follows the PR on the writer's head.
        head: number === 40 || number === 41 ? `exp/session-child` : `exp/h-${number}`,
        base: `master`,
        state,
      })
      return url
    }
    const entry = (topic: string) =>
      (async () => (await row(root.sessionId)).guide!.find((e) => e.topic === topic && !e.attachmentId))()

    // No PR yet: untagged. Once the child has an open PR, a new topic is
    // that PR's, and a rewrite of an untagged one takes the tag too.
    // FEED-166 (1): a MERGED PR (or one on another head) is never a default.
    await pr(40, child.sessionId, `merged`)
    await pr(42, child.sessionId)
    await writeGuide({ userId: w.owner, sessionId: child.sessionId, topic: `Before`, text: `Untagged.`, files: [`a.ts`] })
    expect((await entry(`Before`))?.prUrl).toBeUndefined()
    const url = await pr(41, child.sessionId)
    await writeGuide({ userId: w.owner, sessionId: child.sessionId, topic: `After`, text: `Tagged.` })
    expect(await entry(`After`)).toMatchObject({ prUrl: url, sessionId: child.sessionId })

    // The parent tags the child's topic (prUrl only): in place, the child
    // keeps it, its files stay.
    await db.update(sessions).set({ status: `ended` }).where(eq(sessions.id, child.sessionId))
    await writeGuide({ userId: w.owner, sessionId: root.sessionId, topic: `Before`, prUrl: url })
    expect(await entry(`Before`)).toMatchObject({ prUrl: url, sessionId: child.sessionId, files: [`a.ts`], text: `Untagged.` })
    expect((await row(root.sessionId)).guide!.filter((e) => e.topic === `Before`)).toHaveLength(1)
    // A tag of a topic nobody in the subtree filed fails loudly.
    await expect(
      writeGuide({ userId: w.owner, sessionId: root.sessionId, topic: `Nowhere`, prUrl: url })
    ).rejects.toThrow(/Nothing in this tree's Guide matched topic "Nowhere"/)
    // A text write never takes a child's topic over: the parent files its own.
    await writeGuide({ userId: w.owner, sessionId: root.sessionId, topic: `After`, text: `The root's words.` })
    expect((await row(root.sessionId)).guide!.filter((e) => e.topic === `After`)).toHaveLength(2)

    // The ended child continues as a sibling on the same branch: it owns
    // its predecessor's rows (edit and remove).
    const cont = await sendSession({ userId: w.owner, input: { sessionId: child.sessionId, text: `again` } })
    expect(cont.sessionId).not.toBe(child.sessionId)
    expect((await row(cont.sessionId)).branch).toBe(`exp/session-child`)
    // #EXP-1402: the lineage, not the branch, makes it the same logical session.
    expect((await row(cont.sessionId)).continuedFromId).toBe(child.sessionId)
    await writeGuide({ userId: w.owner, sessionId: cont.sessionId, topic: `Before`, text: `Rewritten.` })
    expect(await entry(`Before`)).toMatchObject({ text: `Rewritten.`, sessionId: cont.sessionId, prUrl: url })
    // #EXP-1402: once that PR merged, a rewrite drops its tag for the open
    // PR on the head, so the new text shows in the live body.
    const next = `https://github.com/o/r/pull/42`
    await db.update(pullRequests).set({ state: `merged` }).where(eq(pullRequests.url, url))
    await db.update(pullRequests).set({ head: `exp/session-child` }).where(eq(pullRequests.url, next))
    await writeGuide({ userId: w.owner, sessionId: cont.sessionId, topic: `Before`, text: `After merge.` })
    expect(await entry(`Before`)).toMatchObject({ text: `After merge.`, prUrl: next })
    // A CLOSED tag with no open PR left: untagged, not sticky.
    await db.update(pullRequests).set({ state: `closed` }).where(eq(pullRequests.url, next))
    await writeGuide({ userId: w.owner, sessionId: cont.sessionId, topic: `Before`, text: `After close.` })
    expect(await entry(`Before`)).toMatchObject({ text: `After close.` })
    expect((await entry(`Before`))?.prUrl).toBeUndefined()
    expect((await entry(`Before`))?.untagged).toBeUndefined()
    expect((await row(root.sessionId)).guide!.filter((e) => e.topic === `Before`)).toHaveLength(1)
    await writeGuide({ userId: w.owner, sessionId: cont.sessionId, topic: `After`, remove: true })
    expect((await row(root.sessionId)).guide!.filter((e) => e.topic === `After`).map((e) => e.sessionId)).toEqual([
      root.sessionId,
    ])
  })
})

// EXP-1397 review: a merge stamps ITS session's untagged topics with its URL
// (bodies exclude by prUrl only, never by time); sibling/cross-chat writes
// never reach another's entries; a parent may tag or replace a child's.
describe.skipIf(!ready.ok)(`EXP-1397: Guide ownership across PRs`, async () => {
  const { publishGuidePicture } = await import(`@/lib/sessions/guide`)
  const { applyPullRequestMerged } = await import(`@/lib/integrations/pr-sync`)
  const { prBodyFromGuide } = await import(`@/lib/pr-body`)

  async function chat() {
    const w = await world()
    const root = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `r` } })
    const kid = async (branch: string) => {
      const child = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: branch } })
      await db.update(sessions).set({ branch }).where(eq(sessions.id, child.sessionId))
      return child.sessionId
    }
    const [repo] = await db
      .insert(repositories)
      .values({ teamId: w.teamId, fullName: `o/r-${id8()}` })
      .returning({ id: repositories.id, fullName: repositories.fullName })
    const pr = async (number: number, sessionId: string | null, head: string) => {
      const url = `https://github.com/${repo!.fullName}/pull/${number}`
      const [inserted] = await db
        .insert(pullRequests)
        .values({
          teamId: w.teamId,
          rootSessionId: root.sessionId,
          sessionId,
          repositoryId: repo!.id,
          number,
          url,
          head,
          base: `master`,
          state: `open`,
        })
        .returning({ id: pullRequests.id })
      return { id: inserted!.id, url }
    }
    const guide = async () => (await row(root.sessionId)).guide ?? []
    const body = async (prUrl: string) => prBodyFromGuide(await guide(), { prUrl }) ?? ``
    const write = (sessionId: string, topic: string, extra: Record<string, unknown> = {}) =>
      writeGuide({ userId: w.owner, sessionId, topic, ...extra })
    return { w, root: root.sessionId, repo: repo!, kid, pr, guide, body, write }
  }

  const picture = () => {
    const attachmentId = crypto.randomUUID()
    return {
      attachmentId,
      filename: `shot.png`,
      contentType: `image/png`,
      sizeBytes: 3,
      storageKey: `test/${attachmentId}`,
      url: `/api/attachments/${attachmentId}`,
      width: 10,
      height: 10,
    }
  }

  it(`two sibling PRs: one merges, the other's body keeps its topics`, async () => {
    const t = await chat()
    const a = await t.kid(`exp/a`)
    const b = await t.kid(`exp/b`)
    // Untagged words, as filed before the PRs existed (or before deploy).
    await t.write(t.root, `Summary`, { text: `The chat.` })
    await t.write(a, `Alpha`, { text: `A's work.` })
    await t.write(b, `Beta`, { text: `B's work.` })
    const prA = await t.pr(1, a, `exp/a`)
    const prB = await t.pr(2, b, `exp/b`)
    expect(await t.body(prB.url)).toContain(`A's work.`)

    await applyPullRequestMerged({ pullRequestId: prA.id })
    const alpha = (await t.guide()).find((e) => e.topic === `Alpha`)
    expect(alpha).toMatchObject({ prUrl: prA.url, sessionId: a })
    const bodyB = await t.body(prB.url)
    expect(bodyB).toContain(`B's work.`)
    expect(bodyB).toContain(`The chat.`)
    expect(bodyB).not.toContain(`A's work.`)
    expect((await t.guide()).find((e) => e.topic === `Beta`)?.prUrl).toBeUndefined()
    // A later write by B (new `at` everywhere else) changes nothing of that.
    await t.write(b, `Beta`, { text: `B's work, again.` })
    expect(await t.body(prB.url)).toContain(`The chat.`)
  })

  it(`legacy entries without at or sessionId stay in every open body after a merge and a write`, async () => {
    const t = await chat()
    const a = await t.kid(`exp/a`)
    const b = await t.kid(`exp/b`)
    const legacy = (topic: string, text: string) => ({
      topic,
      label: null,
      attachmentId: null,
      width: null,
      height: null,
      text,
    })
    await db
      .update(sessions)
      .set({ guide: [legacy(`Summary`, `Legacy summary.`), legacy(`Old`, `Legacy topic.`)] })
      .where(eq(sessions.id, t.root))
    const prA = await t.pr(1, a, `exp/a`)
    const prB = await t.pr(2, b, `exp/b`)
    await applyPullRequestMerged({ pullRequestId: prA.id })
    await t.write(b, `Beta`, { text: `B's work.` })
    const bodyB = await t.body(prB.url)
    expect(bodyB).toContain(`Legacy summary.`)
    expect(bodyB).toContain(`Legacy topic.`)
    expect(bodyB).toContain(`B's work.`)
    expect((await t.guide()).filter((e) => e.prUrl)).toEqual([expect.objectContaining({ topic: `Beta`, prUrl: prB.url })])
  })

  it(`a UI-opened PR (no session) merging stamps the session on its head, nobody else's`, async () => {
    const t = await chat()
    const a = await t.kid(`exp/a`)
    const b = await t.kid(`exp/b`)
    await t.write(a, `Alpha`, { text: `A's work.` })
    await t.write(b, `Beta`, { text: `B's work.` })
    const prA = await t.pr(1, null, `exp/a`)
    const prB = await t.pr(2, null, `exp/b`)
    // A head no session of the tree is on: nothing moves.
    const stray = await t.pr(3, null, `feature/elsewhere`)
    const before = await t.guide()
    await applyPullRequestMerged({ pullRequestId: stray.id })
    expect(await t.guide()).toEqual(before)

    await applyPullRequestMerged({ pullRequestId: prA.id })
    expect((await t.guide()).find((e) => e.topic === `Alpha`)?.prUrl).toBe(prA.url)
    const bodyB = await t.body(prB.url)
    expect(bodyB).toContain(`B's work.`)
    expect(bodyB).not.toContain(`A's work.`)
  })

  it(`#EXP-1540: a UI-opened PR on a shared head resolves to the session that pushed it`, async () => {
    const { attributePush } = await import(`@/lib/sessions/push-attribution`)
    const t = await chat()
    const a = await t.kid(`exp/shared`)
    const b = await t.kid(`exp/shared`)
    await db
      .update(sessions)
      .set({ status: `running`, repositoryId: t.repo.id })
      .where(inArray(sessions.id, [a, b]))
    await t.write(a, `Alpha`, { text: `A's work.` })
    await t.write(b, `Beta`, { text: `B's work.` })
    // Two idle lineages on the branch: the push credits nobody.
    expect(await attributePush({ fullName: t.repo.fullName, branch: `exp/shared` })).toBeNull()
    // B is mid-turn when the push lands: B pushed. updated_at stays put.
    await db.update(sessions).set({ busy: true }).where(eq(sessions.id, b))
    const before = (await row(b)).updatedAt
    // Another repository's branch of the same name credits nobody here.
    expect(await attributePush({ fullName: `other/repo`, branch: `exp/shared` })).toBeNull()
    expect(await attributePush({ fullName: t.repo.fullName, branch: `exp/shared` })).toBe(b)
    expect((await row(b)).updatedAt).toEqual(before)
    expect(await latestPush(t.repo.id, `exp/shared`)).toMatchObject({ sessionId: b })

    const pr = await t.pr(1, null, `exp/shared`)
    await applyPullRequestMerged({ pullRequestId: pr.id })
    const guide = await t.guide()
    expect(guide.find((e) => e.topic === `Beta`)?.prUrl).toBe(pr.url)
    expect(guide.find((e) => e.topic === `Alpha`)?.prUrl).toBeUndefined()
  })

  it(`#EXP-1540: a later push credited to nobody withdraws the earlier credit`, async () => {
    const { attributePush } = await import(`@/lib/sessions/push-attribution`)
    const t = await chat()
    const a = await t.kid(`exp/stale`)
    const b = await t.kid(`exp/stale`)
    await db.update(sessions).set({ status: `running`, repositoryId: t.repo.id }).where(inArray(sessions.id, [a, b]))
    await t.write(a, `Alpha`, { text: `A's work.` })
    const t1 = new Date(Date.now() - 60_000)
    const t2 = new Date(Date.now() - 30_000)
    await db.update(sessions).set({ busy: true }).where(eq(sessions.id, a))
    expect(await attributePush({ fullName: t.repo.fullName, branch: `exp/stale`, at: t1 })).toBe(a)
    // Both idle now; B pushes: nobody is credited, and A's credit is gone.
    await db.update(sessions).set({ busy: false }).where(eq(sessions.id, a))
    expect(await attributePush({ fullName: t.repo.fullName, branch: `exp/stale`, at: t2 })).toBeNull()
    expect(await latestPush(t.repo.id, `exp/stale`)).toMatchObject({ sessionId: null, pushedAt: t2 })

    const pr = await t.pr(1, null, `exp/stale`)
    await applyPullRequestMerged({ pullRequestId: pr.id })
    expect((await t.guide()).find((e) => e.topic === `Alpha`)?.prUrl).toBeUndefined()
  })

  it(`#EXP-1540: a push is dated by its payload; a redelivered older push never overrides a newer credit`, async () => {
    const { attributePush } = await import(`@/lib/sessions/push-attribution`)
    const t = await chat()
    const a = await t.kid(`exp/redeliver`)
    const b = await t.kid(`exp/redeliver`)
    await db.update(sessions).set({ status: `running`, repositoryId: t.repo.id }).where(inArray(sessions.id, [a, b]))
    await t.write(a, `Alpha`, { text: `A's work.` })
    await t.write(b, `Beta`, { text: `B's work.` })
    const t0 = new Date(Date.now() - 90_000)
    const t1 = new Date(Date.now() - 60_000)
    const t2 = new Date(Date.now() - 30_000)
    const push = (at: Date) => attributePush({ fullName: t.repo.fullName, branch: `exp/redeliver`, at })
    await db.update(sessions).set({ busy: true }).where(eq(sessions.id, b))
    expect(await push(t2)).toBe(b)
    expect(await latestPush(t.repo.id, `exp/redeliver`)).toMatchObject({ sessionId: b, pushedAt: t2 })
    // B's own older push, redelivered: B's credit stays at t2.
    expect(await push(t1)).toBeNull()
    expect(await latestPush(t.repo.id, `exp/redeliver`)).toMatchObject({ sessionId: b, pushedAt: t2 })
    // An older uncredited push never withdraws the newer credit.
    await db.update(sessions).set({ busy: false }).where(eq(sessions.id, b))
    expect(await push(t0)).toBeNull()
    expect(await latestPush(t.repo.id, `exp/redeliver`)).toMatchObject({ sessionId: b, pushedAt: t2 })
    // An older push redelivered while A is busy credits nobody: B stays the head's author.
    await db.update(sessions).set({ busy: true }).where(eq(sessions.id, a))
    expect(await push(t1)).toBeNull()
    expect(await latestPush(t.repo.id, `exp/redeliver`)).toMatchObject({ sessionId: b, pushedAt: t2 })

    const pr = await t.pr(1, null, `exp/redeliver`)
    await applyPullRequestMerged({ pullRequestId: pr.id })
    const guide = await t.guide()
    expect(guide.find((e) => e.topic === `Beta`)?.prUrl).toBe(pr.url)
    expect(guide.find((e) => e.topic === `Alpha`)?.prUrl).toBeUndefined()
  })

  it(`#EXP-1540: only the delivering installation's repository counts`, async () => {
    const { attributePush } = await import(`@/lib/sessions/push-attribution`)
    const t = await chat()
    const other = await chat()
    const installation = 100_000 + Math.floor(Math.random() * 1_000_000)
    await db.update(repositories).set({ providerConfig: { installationId: installation } }).where(eq(repositories.id, t.repo.id))
    // Another team registered the same full name under another installation.
    const [theirs] = await db
      .insert(repositories)
      .values({ teamId: other.w.teamId, fullName: t.repo.fullName, providerConfig: { installationId: installation + 1 } })
      .returning({ id: repositories.id })
    const mine = await t.kid(`exp/scoped`)
    const foreign = await other.kid(`exp/scoped`)
    await db.update(sessions).set({ status: `running`, repositoryId: t.repo.id }).where(eq(sessions.id, mine))
    await db.update(sessions).set({ status: `running`, repositoryId: theirs!.id }).where(eq(sessions.id, foreign))
    // Unscoped, the two teams' sessions are ambiguous.
    expect(await attributePush({ fullName: t.repo.fullName, branch: `exp/scoped` })).toBeNull()
    expect(await attributePush({ fullName: t.repo.fullName, branch: `exp/scoped`, installationId: installation })).toBe(
      mine
    )
    expect(await latestPush(theirs!.id, `exp/scoped`)).toMatchObject({ sessionId: null })
    expect(
      await attributePush({ fullName: t.repo.fullName, branch: `exp/scoped`, installationId: installation + 1 })
    ).toBe(foreign)
    expect(await latestPush(t.repo.id, `exp/scoped`)).toMatchObject({ sessionId: mine })
    expect(await latestPush(theirs!.id, `exp/scoped`)).toMatchObject({ sessionId: foreign })
  })

  it(`#EXP-1540: an ended session is credited for two minutes after it ended`, async () => {
    const { attributePush, PUSH_GRACE_MS } = await import(`@/lib/sessions/push-attribution`)
    const t = await chat()
    const a = await t.kid(`exp/grace`)
    const now = new Date()
    const ended = (ms: number) =>
      db
        .update(sessions)
        .set({ status: `ended`, endedAt: new Date(now.getTime() - ms), repositoryId: t.repo.id })
        .where(eq(sessions.id, a))
    const push = (at: Date) => attributePush({ fullName: t.repo.fullName, branch: `exp/grace`, at, now })
    await ended(PUSH_GRACE_MS + 1_000)
    expect(await push(new Date(now.getTime() - 1_000))).toBeNull()
    await ended(PUSH_GRACE_MS - 1_000)
    expect(await push(now)).toBe(a)
    expect(await latestPush(t.repo.id, `exp/grace`)).toMatchObject({ sessionId: a, pushedAt: now })
  })

  it(`#EXP-1540: a newer uncredited push delivered first wins over an older credited push delivered late`, async () => {
    const { attributePush } = await import(`@/lib/sessions/push-attribution`)
    const t = await chat()
    const a = await t.kid(`exp/late`)
    const b = await t.kid(`exp/late`)
    await db.update(sessions).set({ status: `running`, repositoryId: t.repo.id }).where(inArray(sessions.id, [a, b]))
    await t.write(a, `Alpha`, { text: `A's work.` })
    const t1 = new Date(Date.now() - 60_000)
    const t2 = new Date(Date.now() - 30_000)
    const push = (at: Date) => attributePush({ fullName: t.repo.fullName, branch: `exp/late`, at })
    // The newer push (both idle: nobody's) arrives first.
    expect(await push(t2)).toBeNull()
    // A's older push, delivered late while A is busy: still nobody's.
    await db.update(sessions).set({ busy: true }).where(eq(sessions.id, a))
    expect(await push(t1)).toBeNull()
    expect(await latestPush(t.repo.id, `exp/late`)).toMatchObject({ sessionId: null, pushedAt: t2 })

    const pr = await t.pr(1, null, `exp/late`)
    await applyPullRequestMerged({ pullRequestId: pr.id })
    expect((await t.guide()).find((e) => e.topic === `Alpha`)?.prUrl).toBeUndefined()
  })

  it(`an explicit prUrl: null sticks across rewrites; a NEW topic takes the open PR`, async () => {
    const t = await chat()
    const a = await t.kid(`exp/a`)
    const prA = await t.pr(1, a, `exp/a`)
    await t.write(a, `Shared`, { text: `For every PR.`, prUrl: null })
    await t.write(a, `Shared`, { text: `Still for every PR.` })
    expect((await t.guide()).find((e) => e.topic === `Shared`)).toMatchObject({ text: `Still for every PR.` })
    expect((await t.guide()).find((e) => e.topic === `Shared`)?.prUrl).toBeUndefined()
    await t.write(a, `New`, { text: `Mine.` })
    expect((await t.guide()).find((e) => e.topic === `New`)?.prUrl).toBe(prA.url)
  })

  it(`a parent's files: null clears a child's files; a sibling and another chat are denied`, async () => {
    const t = await chat()
    const a = await t.kid(`exp/a`)
    const b = await t.kid(`exp/b`)
    await t.write(a, `Alpha`, { text: `A's work.`, files: [`a.ts`] })
    await t.write(t.root, `Alpha`, { files: null })
    const alpha = (await t.guide()).find((e) => e.topic === `Alpha`)!
    expect(alpha).toMatchObject({ sessionId: a, text: `A's work.` })
    expect(alpha.files).toBeUndefined()

    const before = await t.guide()
    await expect(t.write(b, `Alpha`, { prUrl: `https://github.com/o/r/pull/9` })).rejects.toThrow(
      /Nothing in this tree's Guide matched topic "Alpha"/
    )
    await expect(t.write(b, `Alpha`, { remove: true })).rejects.toMatchObject({ code: `NOT_FOUND` })
    const other = await chat()
    await expect(other.write(other.root, `Alpha`, { prUrl: `https://github.com/o/r/pull/9` })).rejects.toThrow(
      /Nothing in this tree's Guide matched topic "Alpha"/
    )
    expect(await t.guide()).toEqual(before)
  })

  it(`pictures: a sibling never replaces or removes another's; a parent replaces a child's in place`, async () => {
    const t = await chat()
    const a = await t.kid(`exp/a`)
    const b = await t.kid(`exp/b`)
    const publish = (sessionId: string, prepared: ReturnType<typeof picture>) =>
      publishGuidePicture(
        { sessionId, teamId: t.w.teamId, topic: `Screen`, label: `web: home`, uploaderId: t.w.owner },
        prepared
      )
    const first = picture()
    await publish(a, first)
    const pics = async () => (await t.guide()).filter((e) => e.attachmentId)

    // A sibling's same (topic, label) = its own new entry, A's untouched.
    const sib = picture()
    expect((await publish(b, sib)).replaced).toBe(false)
    expect((await pics()).map((e) => [e.sessionId, e.attachmentId])).toEqual([
      [a, first.attachmentId],
      [b, sib.attachmentId],
    ])
    await expect(t.write(b, `Screen`, { label: `web: home`, remove: true })).resolves.toBeTruthy()
    expect((await pics()).map((e) => e.sessionId)).toEqual([a])
    await expect(t.write(b, `Screen`, { label: `web: home`, remove: true })).rejects.toMatchObject({
      code: `NOT_FOUND`,
    })

    // The parent replaces the child's picture in place; the child keeps it.
    const next = picture()
    expect((await publish(t.root, next)).replaced).toBe(true)
    expect((await pics()).map((e) => [e.sessionId, e.attachmentId])).toEqual([[a, next.attachmentId]])
    expect(await db.select().from(attachments).where(eq(attachments.id, first.attachmentId))).toHaveLength(0)
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

  it(`#NEW-79: only its member answers a voice conversation's proposal card; never a session`, async () => {
    const w = await world({ shared: true })
    const [ada] = await db
      .insert(agents)
      .values({ teamId: w.teamId, name: `Ada`, icon: `rocket`, color: `#ff0000`, deviceId: w.deviceId, ownerId: w.owner })
      .returning({ id: agents.id })
    const voice = await sendSession({ userId: w.owner, input: { agentId: ada!.id, text: `hello` } })
    // A shared conversation: the teammate can read and steer it.
    await db.update(sessions).set({ shared: true }).where(eq(sessions.id, voice.sessionId))
    const host = caller(w.owner)
    await host.claim({ sessionId: voice.sessionId, deviceId: w.deviceId })
    const card = () =>
      host.reply({
        sessionId: voice.sessionId,
        body: ``,
        card: { kind: `question`, question: `Issue · delete CR-1. Say yes to go ahead.`, options: [`Allow`, `Deny`], questionId: `toolu_${id8()}` },
      })

    // Not the voice agent (yet): any member who may steer answers it.
    const plain = await card()
    await caller(w.mate).answer({ messageId: plain.messageId, answer: `Allow` })

    await db.insert(userVoiceSettings).values({ userId: w.owner, agentId: ada!.id })
    const proposal = await card()
    await expect(caller(w.mate).answer({ messageId: proposal.messageId, answer: `Allow` })).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
    await expect(caller(w.mate).resolveCard({ messageId: proposal.messageId, answer: `Allow` })).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
    // Over the bus nobody answers it, its member's own sessions included.
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: voice.sessionId, answer: { messageId: proposal.messageId, answer: `Allow` } } })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
    const [open] = await db.select().from(messages).where(eq(messages.id, proposal.messageId))
    expect(open!.answeredAt).toBeNull()
    // Its member answers it.
    await caller(w.owner).answer({ messageId: proposal.messageId, answer: `Allow` })
    const [answered] = await db.select().from(messages).where(eq(messages.id, proposal.messageId))
    expect(answered!.answeredAt).not.toBeNull()
    await db.delete(userVoiceSettings).where(eq(userVoiceSettings.userId, w.owner))
  })

  it(`claims, reports, asks, is answered and consumes — only from the device's account`, async () => {
    const w = await world({ shared: true })
    const sent = await sendSession({
      userId: w.mate,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `start` },
    })
    // The starter is not the device: only the host runs the device procedures.
    await expect(caller(w.mate).claim({ sessionId: sent.sessionId, deviceId: w.deviceId })).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
    const host = caller(w.owner)
    await host.claim({ sessionId: sent.sessionId, deviceId: w.deviceId, branch: `exp/x` })
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
      .where(and(inSession(sent.sessionId), eq(messages.body, `b`)))
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
      title: `Ada 2`,
      account: `acc1`,
    })
    // The conversation row's visibility mirror follows the move (the
    // conversations shape filters on it).
    const [convo] = await db.select().from(conversations).where(eq(conversations.sessionId, conversation.sessionId))
    expect(convo).toMatchObject({ shared: true, ownerId: w.owner })
    // A member's own rename of the chat survives the agent's next rename.
    await db.update(sessions).set({ title: `My Ada` }).where(eq(sessions.id, conversation.sessionId))
    await agentsOf(w.owner).update({ id: agent.id, name: `Ada 3` })
    expect(await row(conversation.sessionId)).toMatchObject({ title: `My Ada` })
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
      effort: `high`,
      ultracode: true,
      computerUse: true,
      mcpServerIds: [server!.id, server!.id],
    })
    expect(out.txid).toBeTypeOf(`number`)
    expect(await row(child.sessionId)).toMatchObject({
      model: `sonnet`,
      effort: `high`,
      ultracode: true,
      computerUse: true,
      mcpServerIds: [server!.id],
    })
    expect(await owner.mcpPicks({ sessionId: child.sessionId })).toEqual({ mcpServerIds: [server!.id] })
    // Omitted = unchanged; a null effort = the agent's default; [] = none.
    await owner.configure({ sessionId: child.sessionId, effort: null, mcpServerIds: [] })
    expect(await row(child.sessionId)).toMatchObject({ effort: null, mcpServerIds: null })
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
    await owner.configure({ sessionId: root.sessionId, model: `opus`, effort: `low` })
    expect(await row(root.sessionId)).toMatchObject({ model: `opus`, effort: `low` })

    // The steer rule: a private session refuses a teammate; the device-only
    // read refuses anyone but the host.
    await expect(caller(w.mate).configure({ sessionId: child.sessionId, effort: `low` })).rejects.toMatchObject({
      code: `NOT_FOUND`,
    })
    // An ended session refuses.
    await endSession({ userId: w.owner, sessionId: child.sessionId, summary: `Done.` })
    await expect(owner.configure({ sessionId: child.sessionId, effort: `low` })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
    })
  })

  it(`the model the device reports follows`, async () => {
    const w = await world()
    const root = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, cli: `claude`, text: `root` },
    })
    const child = await sendSession({
      userId: w.owner,
      input: { parentId: root.sessionId, text: `plan first` },
    })
    const host = caller(w.owner)

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
    db.select().from(messages).where(inSession(sessionId))

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
    await host.claim({ sessionId: child.sessionId, deviceId: w.deviceId })
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
    await host.claim({ sessionId: quiet.sessionId, deviceId: w.deviceId })
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

  it(`setRead / setArchived (#EXP-1388): bulk read marks and the per-viewer archive`, async () => {
    const w = await world()
    const { postAgentMessage } = await import(`@/lib/sessions/messages`)
    const root = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `r` } })
    const quiet = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `q` } })
    await db.transaction((tx) => postAgentMessage(tx, root.sessionId, `done`))
    const readOf = async (sessionId: string) =>
      (
        await db
          .select()
          .from(sessionReads)
          .where(and(eq(sessionReads.sessionId, sessionId), eq(sessionReads.userId, w.owner)))
      )[0]
    // Archive first: a fresh row starts unread (epoch) and carries archived_at.
    await caller(w.owner).setArchived({ sessionIds: [root.sessionId, quiet.sessionId], archived: true })
    expect((await readOf(root.sessionId))!.archivedAt).toBeInstanceOf(Date)
    expect((await readOf(root.sessionId))!.readAt.getTime()).toBe(0)
    await caller(w.owner).setRead({ sessionIds: [root.sessionId, quiet.sessionId], read: true })
    expect((await readOf(root.sessionId))!.readAt.getTime()).toBeGreaterThan(0)
    expect((await readOf(root.sessionId))!.archivedAt).toBeInstanceOf(Date)
    // Unread = just before the newest agent message; none = unchanged.
    const quietRead = (await readOf(quiet.sessionId))!.readAt.getTime()
    await caller(w.owner).setRead({ sessionIds: [root.sessionId, quiet.sessionId], read: false })
    const [agent] = await db
      .select({ createdAt: messages.createdAt })
      .from(messages)
      .where(and(inSession(root.sessionId), eq(messages.authorKind, `agent`)))
    expect((await readOf(root.sessionId))!.readAt.getTime()).toBe(agent!.createdAt.getTime() - 1)
    expect((await readOf(quiet.sessionId))!.readAt.getTime()).toBe(quietRead)
    await caller(w.owner).setArchived({ sessionIds: [root.sessionId], archived: false })
    expect((await readOf(root.sessionId))!.archivedAt).toBeNull()
    // A child is not a chat; a private chat is not the teammate's.
    const child = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `c` } })
    await expect(caller(w.owner).setArchived({ sessionIds: [child.sessionId], archived: true })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
    })
    await expect(caller(w.mate).setRead({ sessionIds: [root.sessionId], read: true })).rejects.toMatchObject({
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
  const install = async (userId: string) =>
    (
      await db
        .insert(installs)
        .values({ userId, kind: `desktop`, endpointId: `ep-${crypto.randomUUID()}`, x25519Public: `x`.repeat(43) })
        .returning({ id: installs.id })
    )[0]!.id
  const systemBodies = async (sessionId: string) =>
    (
      await db
        .select()
        .from(messages)
        .where(and(inSession(sessionId), eq(messages.authorKind, `system`)))
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
    })
    const [moved] = await db
      .select()
      .from(messages)
      .where(and(inSession(fresh.sessionId), eq(messages.body, `Moved from box to b-box`)))
    expect(moved).toMatchObject({ authorKind: `system`, card: null })
    expect(moved!.consumedAt).not.toBeNull()
    // The current device = nothing at all.
    await sendSession({ userId: w.owner, input: { sessionId: fresh.sessionId, deviceId: w.target, text: `hi` } })
    expect(await row(fresh.sessionId)).toMatchObject({ deviceId: w.target, status: `queued` })
  })

  it(`#FEED-177: a QUEUED root off an offline box moves to its starter's team-shared box and takes its sharing`, async () => {
    const w = await moveWorld()
    // 930b6eb8 → 98b9eaa8: the private run's box went offline before it
    // claimed; the starter's other machine is shared with the team.
    const shared = `dev-s-${id8()}`
    await db.insert(devices).values({
      userId: w.owner,
      deviceId: shared,
      label: `s-box`,
      kind: `server`,
      version: `0.14.74`,
      sharedTeamIds: [w.teamId],
      agentAccounts: signedIn(`ps`, `me@move.test`),
    })
    const issueId = await w.issue(`Moving box`)
    const queued = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, cli: `claude`, account: `pa`, issueIds: [issueId], text: `not yet` },
    })
    expect(await row(queued.sessionId)).toMatchObject({ shared: false, status: `queued` })
    // The visibility mirrors copied at insert time (the messages shape and
    // the linked issues filter on them) follow the root both ways.
    const mirrors = async () => ({
      messages: [
        ...new Set((await db.select().from(messages).where(eq(messages.rootId, queued.sessionId))).map((m) => m.shared)),
      ],
      issues: (await db.select().from(sessionIssues).where(eq(sessionIssues.sessionId, queued.sessionId))).map(
        (l) => l.shared
      ),
      conversations: (
        await db.select().from(conversations).where(eq(conversations.sessionId, queued.sessionId))
      ).map((c) => c.shared),
    })
    expect(await mirrors()).toEqual({ messages: [false], issues: [false], conversations: [false] })
    await db.update(devices).set({ lastSeenAt: new Date(Date.now() - 3_600_000) }).where(eq(devices.deviceId, w.deviceId))
    await sendSession({ userId: w.owner, input: { sessionId: queued.sessionId, deviceId: shared } })
    expect(await row(queued.sessionId)).toMatchObject({ deviceId: shared, shared: true, status: `queued`, account: `ps` })
    expect(await mirrors()).toEqual({ messages: [true], issues: [true], conversations: [true] })
    // A teammate may steer it now, but never make it private again.
    await expect(
      sendSession({ userId: w.mate, input: { sessionId: queued.sessionId, deviceId: w.target } })
    ).rejects.toMatchObject({ message: MOVE_COPY.visibility })
    // The tree is keyed; every teammate's install holds a wrap.
    const ownerInstall = await install(w.owner)
    const mateInstall = await install(w.mate)
    await db.update(sessions).set({ sealEpoch: 1, sealPublicKey: `k`.repeat(43) }).where(eq(sessions.id, queued.sessionId))
    await db.insert(sessionKeys).values(
      [ownerInstall, mateInstall].map((installId) => ({ rootId: queued.sessionId, epoch: 1, installId, wrapped: `w` }))
    )
    const rotation = (installId: string) => ({
      rootId: queued.sessionId,
      installId,
      epoch: 2,
      publicKey: `${`B`.repeat(42)}A`,
      wraps: [{ installId, wrapped: `r`.repeat(108) }],
    })
    // A reader that is neither the starter nor the executing device never
    // rotates.
    await expect(db.transaction((tx) => createTreeKeyInTx(tx, w.mate, rotation(mateInstall)))).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
    expect(await row(queued.sessionId)).toMatchObject({ sealEpoch: 1, sealRotateEpoch: null })
    // Its starter may (their own personal box): the tree's key keeps only
    // its readers' wraps (the teammate's install loses its row).
    await sendSession({ userId: w.owner, input: { sessionId: queued.sessionId, deviceId: w.target } })
    expect(await row(queued.sessionId)).toMatchObject({ deviceId: w.target, shared: false })
    expect(await mirrors()).toEqual({ messages: [false], issues: [false], conversations: [false] })
    expect(
      (await db.select().from(sessionKeys).where(eq(sessionKeys.rootId, queued.sessionId))).map((k) => k.installId)
    ).toEqual([ownerInstall])
    // The mate's install may have unwrapped epoch 1: the root asks its
    // holders to rotate it (#FEED-177).
    expect(await row(queued.sessionId)).toMatchObject({ sealEpoch: 1, sealRotateEpoch: 1 })
    // The teammate (no longer a reader, never the starter) cannot rotate.
    await expect(db.transaction((tx) => createTreeKeyInTx(tx, w.mate, rotation(mateInstall)))).rejects.toMatchObject({
      code: `NOT_FOUND`,
    })
    expect(await row(queued.sessionId)).toMatchObject({ sealEpoch: 1, sealRotateEpoch: 1 })
    // The starter's install rotates (wrapped for itself; `pending` brings
    // the other current readers theirs): the flag clears.
    await db.transaction((tx) => createTreeKeyInTx(tx, w.owner, rotation(ownerInstall)))
    expect(await row(queued.sessionId)).toMatchObject({
      sealEpoch: 2,
      sealPublicKey: `${`B`.repeat(42)}A`,
      sealRotateEpoch: null,
    })
    // A second rotation at the same epoch is a CONFLICT (someone rotated).
    await expect(
      db.transaction((tx) => createTreeKeyInTx(tx, w.owner, rotation(ownerInstall)))
    ).rejects.toMatchObject({ code: `CONFLICT` })
    await db.update(sessions).set({ sealEpoch: null, sealPublicKey: null }).where(eq(sessions.id, queued.sessionId))

    // A private root never lands on a teammate's shared box.
    const mateBox = `dev-ms-${id8()}`
    await db.insert(devices).values({
      userId: w.mate,
      deviceId: mateBox,
      label: `mate-box`,
      kind: `server`,
      version: `0.14.74`,
      sharedTeamIds: [w.teamId],
      agentAccounts: signedIn(`pm`, `mate@move.test`),
    })
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: queued.sessionId, deviceId: mateBox } })
    ).rejects.toMatchObject({ message: MOVE_COPY.visibility })
    // A tree with a sub-session keeps its one visibility.
    await sendSession({ userId: w.owner, input: { parentId: queued.sessionId, deviceId: w.target, text: `kid` } })
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: queued.sessionId, deviceId: shared } })
    ).rejects.toMatchObject({ message: MOVE_COPY.visibility })
    expect(await row(queued.sessionId)).toMatchObject({ deviceId: w.target, shared: false })
  })

  it(`a LIVE child ends and continues on the target on its branch; children and watchers follow`, async () => {
    const w = await moveWorld()
    const moving = await sendSession({ userId: w.owner, input: { parentId: w.sessionId, text: `child` } })
    await caller(w.owner).claim({ sessionId: moving.sessionId, deviceId: w.deviceId, branch: `exp/child` })
    const issueId = await w.issue(`Moving work`)
    await sendSession({ userId: w.owner, input: { sessionId: moving.sessionId, issueIds: [issueId] } })
    const kid = await sendSession({ userId: w.owner, input: { parentId: moving.sessionId, text: `kid` } })
    // A second root watching the moving one.
    const watcher = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `w` } })
    await sendSession({
      userId: w.owner,
      callerSessionId: watcher.sessionId,
      viaMcp: true,
      input: { sessionId: watcher.sessionId, wakeOn: [moving.sessionId] },
    })
    expect((await row(watcher.sessionId)).wakeOn).toEqual([moving.sessionId])

    const sent = await sendSession({
      userId: w.owner,
      input: { sessionId: moving.sessionId, deviceId: w.target, text: `carry on there` },
    })
    expect(sent.sessionId).not.toBe(moving.sessionId)
    expect(await row(moving.sessionId)).toMatchObject({ status: `ended`, busy: false, deviceId: w.deviceId })
    expect(await row(sent.sessionId)).toMatchObject({
      rootId: w.sessionId,
      deviceId: w.target,
      branch: `exp/child`,
      account: `pb`,
      status: `queued`,
    })
    expect(await systemBodies(moving.sessionId)).toContain(`Moved from box to b-box`)
    // The message waits on the continuation.
    const [pending] = await db.select(messageWithSession()).from(messages).where(eq(messages.id, sent.messageId!))
    expect(pending).toMatchObject({ sessionId: sent.sessionId, body: `carry on there`, consumedAt: null })
    // Its live child re-parents, its watcher follows, its issue rows move.
    expect(await row(kid.sessionId)).toMatchObject({ parentId: sent.sessionId, status: `queued` })
    expect((await row(watcher.sessionId)).wakeOn).toEqual([sent.sessionId])
    const [link] = await db.select().from(sessionIssues).where(eq(sessionIssues.issueId, issueId))
    expect(link!.sessionId).toBe(sent.sessionId)
    // A moved child continues as its sibling.
    expect((await row(sent.sessionId)).parentId).toBe(w.sessionId)
  })

  // #EXP-1737: ONE worker per root. A live root moves IN PLACE: the row
  // re-queues on the target (its old device releases the engine and hands the
  // branch on, shipped daemons do both), so the next send cannot revive a
  // second worker beside a continuation.
  it(`EXP-1737: a LIVE root moves in place: one row, one worker, its tree stays`, async () => {
    const { SESSION_MOVED_EVENT } = await import(`@/lib/sessions/move`)
    const w = await moveWorld()
    const kid = await sendSession({ userId: w.owner, input: { parentId: w.sessionId, text: `kid` } })
    await db
      .update(sessions)
      .set({ busy: true, needsInput: true, blocked: { kind: `rate_limit`, agent: `claude`, window: `session`, resetsAt: null, since: new Date().toISOString() } })
      .where(eq(sessions.id, w.sessionId))

    const moved = await sendSession({ userId: w.owner, input: { sessionId: w.sessionId, deviceId: w.target } })
    expect(moved).toMatchObject({ sessionId: w.sessionId, status: `queued` })
    expect(await row(w.sessionId)).toMatchObject({
      status: `queued`,
      endedAt: null,
      busy: false,
      blocked: null,
      needsInput: false,
      parentId: null,
      deviceId: w.target,
      branch: `exp/move`,
      account: `pb`,
    })
    expect((await row(w.sessionId)).startedAt).not.toBeNull()
    expect(await row(kid.sessionId)).toMatchObject({ parentId: w.sessionId, status: `queued` })
    expect(await systemBodies(w.sessionId)).toContain(`Moved from box to b-box`)
    // The old device hands the branch on; the target waits for it.
    const records = await db
      .select()
      .from(events)
      .where(and(eq(events.teamId, w.teamId), eq(events.name, SESSION_MOVED_EVENT)))
    expect(records.map((record) => record.payload)).toEqual([
      { sessionId: w.sessionId, continuationId: w.sessionId, fromDeviceId: w.deviceId, toDeviceId: w.target },
    ])

    // The old engine's late write (shipped daemons name no device) is a no-op
    // until the target claims the row; its own report still lands after that.
    const host = caller(w.owner)
    await host.report({ sessionId: w.sessionId, status: `idle`, busy: false })
    expect(await row(w.sessionId)).toMatchObject({ status: `queued` })

    // The next message stays on the ONE root, the target claims it pending.
    const next = await sendSession({ userId: w.owner, input: { sessionId: w.sessionId, text: `carry on` } })
    expect(next.sessionId).toBe(w.sessionId)
    await host.claim({ sessionId: w.sessionId, deviceId: w.target })
    expect(await row(w.sessionId)).toMatchObject({ status: `running`, deviceId: w.target })
    const [pending] = await db.select(messageWithSession()).from(messages).where(eq(messages.id, next.messageId!))
    expect(pending).toMatchObject({ sessionId: w.sessionId, consumedAt: null })
    await host.report({ sessionId: w.sessionId, status: `idle`, busy: false })
    expect(await row(w.sessionId)).toMatchObject({ status: `idle` })
    const tree = await db.select().from(sessions).where(eq(sessions.rootId, w.sessionId))
    expect(tree.map((session) => session.id).sort()).toEqual([w.sessionId, kid.sessionId].sort())
  })

  it(`EXP-1737: a live editor root moved with a message stays THE editor session`, async () => {
    const w = await moveWorld()
    const [app] = await db
      .insert(apps)
      .values({ teamId: w.teamId, slug: `s-${id8()}`, name: `Tool`, createdBy: w.owner })
      .returning({ id: apps.id })
    const started = await sendSession({
      userId: w.owner,
      input: { appId: app!.id, deviceId: w.deviceId, cli: `claude`, account: `pa`, text: `build` },
    })
    await caller(w.owner).claim({ sessionId: started.sessionId, deviceId: w.deviceId })
    const moved = await sendSession({
      userId: w.owner,
      input: { appId: app!.id, deviceId: w.target, text: `go on` },
    })
    expect(moved.sessionId).toBe(started.sessionId)
    expect(await row(started.sessionId)).toMatchObject({ status: `queued`, appId: app!.id, deviceId: w.target })
    const [message] = await db.select(messageWithSession()).from(messages).where(eq(messages.id, moved.messageId!))
    expect(message).toMatchObject({ sessionId: started.sessionId, body: `go on`, consumedAt: null })
    // Leaving the editor ends the one worker, nothing survives it.
    await endSession({ userId: w.owner, sessionId: started.sessionId })
    const tree = await db.select().from(sessions).where(eq(sessions.rootId, started.sessionId))
    expect(tree.map((session) => session.status)).toEqual([`ended`])
  })

  it(`EXP-1737: a root that already ran never adopts a box's sharing when it re-queues`, async () => {
    const w = await moveWorld()
    await sendSession({ userId: w.owner, input: { sessionId: w.sessionId, deviceId: w.target } })
    const shared = `dev-s-${id8()}`
    await db.insert(devices).values({
      userId: w.owner,
      deviceId: shared,
      label: `shared`,
      kind: `server`,
      version: `0.14.74`,
      sharedTeamIds: [w.teamId],
      agentAccounts: signedIn(`ps`, `me@move.test`),
    })
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: w.sessionId, deviceId: shared } })
    ).rejects.toMatchObject({ message: MOVE_COPY.visibility })
  })

  // #FEED-182 (#EXP-1532): a child started on ANOTHER device never takes the
  // parent's device-local profile id: a named login resolves on the target
  // (id or email, or the parent's id carried by email), else the send is
  // refused naming the logins signed in there; unnamed = the parent's login
  // by email, else the device's own pick.
  it(`a child on another device runs on a login of THAT device`, async () => {
    const w = await moveWorld()
    const child = (account?: string) =>
      sendSession({
        userId: w.owner,
        input: { parentId: w.sessionId, deviceId: w.target, text: `there`, ...(account ? { account } : {}) },
      })
    expect(await row((await child()).sessionId)).toMatchObject({ deviceId: w.target, account: `pb` })
    expect(await row((await child(`pa`)).sessionId)).toMatchObject({ account: `pb` })
    expect(await row((await child(`me@move.test`)).sessionId)).toMatchObject({ account: `pb` })
    await expect(child(`dennis@elsewhere.test`)).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: `Account "dennis@elsewhere.test" is not signed in for claude on b-box. Signed in there: ME@move.test.`,
    })
    // A login signed out there is no target either.
    await db
      .update(devices)
      .set({
        agentAccounts: {
          claude: {
            signedIn: true,
            email: `other@move.test`,
            profiles: [
              { id: `pb`, signedIn: false, email: `ME@move.test`, health: `signed_out` },
              { id: `pc`, signedIn: true, email: `other@move.test`, active: true },
            ],
          },
        },
      })
      .where(eq(devices.deviceId, w.target))
    expect(await row((await child()).sessionId)).toMatchObject({ deviceId: w.target, account: null })
    await expect(child(`pa`)).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`refuses with the client copy: offline, visibility, no login, agent; an ended chat revives there`, async () => {
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
    // An ENDED chat: a bare device pick has nothing to run; with a
    // message the SAME row revives ON the named device (FEED-134: no engine
    // runs it, so it re-targets like a queued row and says so).
    await endSession({ userId: w.owner, sessionId: w.sessionId })
    await expect(sendSession({ userId: w.owner, input: { sessionId: w.sessionId, deviceId: w.target } })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: `This session has ended: send a message to continue it`,
    })
    const cont = await sendSession({
      userId: w.owner,
      input: { sessionId: w.sessionId, deviceId: w.target, text: `go on` },
    })
    expect(cont.sessionId).toBe(w.sessionId)
    expect(await row(w.sessionId)).toMatchObject({
      parentId: null,
      deviceId: w.target,
      branch: `exp/move`,
      status: `idle`,
      endedAt: null,
    })
    const moved = (await db.select().from(messages).where(inSession(w.sessionId))).filter(
      (m) => m.authorKind === `system` && m.body.startsWith(`Moved from`)
    )
    expect(moved).toHaveLength(1)
    // A named device that is offline refuses, and the row stays ended.
    await endSession({ userId: w.owner, sessionId: w.sessionId })
    const offline = await add({ lastSeenAt: new Date(Date.now() - 3_600_000) })
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: w.sessionId, deviceId: offline, text: `go on` } })
    ).rejects.toMatchObject({ message: MOVE_COPY.offline })
    expect(await row(w.sessionId)).toMatchObject({ status: `ended` })
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
    await expect(raced.claim({ sessionId: fresh.sessionId, deviceId: w.deviceId })).rejects.toMatchObject({ code: `CONFLICT` })
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
    await caller(w.owner).claim({ sessionId: c2.sessionId, deviceId: w.deviceId, branch: `exp/deep` })
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
    expect(links).toMatchObject([{ sessionId: cont.sessionId }])
    // FEED-134: an ended ROOT never continues as a child: it revives.
    await endSession({ userId: w.owner, sessionId: root.sessionId })
    const rootCont = await sendSession({ userId: w.owner, input: { sessionId: root.sessionId, text: `again` } })
    expect(rootCont.sessionId).toBe(root.sessionId)
    expect(await row(root.sessionId)).toMatchObject({ parentId: null, depth: 0, status: `idle` })
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

    // Stop on a running row whose device is offline frees it (EXP-1374:
    // `background` goes with `busy`).
    await db
      .update(devices)
      .set({ lastSeenAt: new Date(Date.now() - 3 * 3_600_000) })
      .where(eq(devices.deviceId, w.deviceId))
    await db
      .update(sessions)
      .set({ status: `running`, busy: true, background: true })
      .where(eq(sessions.id, w.sessionId))
    // Chats: nothing to deliver to a ghost: `delivered` stays true.
    expect(await stopSession({ userId: w.owner, sessionId: w.sessionId })).toMatchObject({ delivered: true })
    expect(await row(w.sessionId)).toMatchObject({ status: `idle`, busy: false, background: false })
    expect((await row(w.sessionId)).stoppedAt).not.toBeNull()
    // An online device's Stop only interrupts; `delivered` = the relay's word.
    await db.update(devices).set({ lastSeenAt: new Date() }).where(eq(devices.deviceId, w.deviceId))
    await db.update(sessions).set({ status: `running`, busy: true }).where(eq(sessions.id, w.sessionId))
    relay.on = true
    try {
      relay.delivered = false
      expect(await stopSession({ userId: w.owner, sessionId: w.sessionId })).toMatchObject({ delivered: false })
      relay.delivered = true
      expect(await stopSession({ userId: w.owner, sessionId: w.sessionId })).toMatchObject({ delivered: true })
      expect(relay.interrupted).toEqual([w.sessionId, w.sessionId])
    } finally {
      relay.on = false
      relay.interrupted = []
    }
    expect(await row(w.sessionId)).toMatchObject({ status: `running`, busy: true })
    // An idle (not busy) session had nothing to interrupt: delivered, even
    // with no relay configured. EXP-1413: nor does it stamp `stopped_at`
    // (the stamp reads the row's own `busy`): a Stop landing after the
    // turn ended by itself holds no merge.
    await db
      .update(sessions)
      .set({ status: `idle`, busy: false, stoppedAt: null })
      .where(eq(sessions.id, w.sessionId))
    expect(await stopSession({ userId: w.owner, sessionId: w.sessionId })).toMatchObject({ delivered: true })
    expect((await row(w.sessionId)).stoppedAt).toBeNull()
    await db
      .update(sessions)
      .set({ status: `running`, busy: false, stoppedAt: null })
      .where(eq(sessions.id, w.sessionId))
    expect(await stopSession({ userId: w.owner, sessionId: w.sessionId })).toMatchObject({ delivered: true })
    expect((await row(w.sessionId)).stoppedAt).toBeNull()
    // A busy one is interrupted: stamped.
    await db.update(sessions).set({ status: `running`, busy: true }).where(eq(sessions.id, w.sessionId))
    await stopSession({ userId: w.owner, sessionId: w.sessionId })
    expect((await row(w.sessionId)).stoppedAt).not.toBeNull()
  })

  it(`Chats: a queued session's Stop delivers nothing and reads delivered`, async () => {
    const { stopSession } = await import(`@/lib/sessions/lifecycle`)
    const w = await world()
    const sent = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `go` } })
    relay.on = true
    try {
      expect(await stopSession({ userId: w.owner, sessionId: sent.sessionId })).toMatchObject({ delivered: true })
      expect(relay.interrupted).toEqual([])
    } finally {
      relay.on = false
    }
  })

  // EXP-1542 (#FEED-169): a move that leaves work on its source records
  // `session.moved` (server-emitted, ids only); the source's hand-off
  // report records ONE `session.handed_off`; the continuation reads both
  // off its brief and `moveState`. A queued re-target records nothing.
  it(`EXP-1542: a move records its source device; the source alone hands it off, once`, async () => {
    const { SESSION_MOVED_EVENT, SESSION_HANDED_OFF_EVENT } = await import(`@/lib/sessions/move`)
    const w = await moveWorld()
    const movedEvents = async () =>
      (await db.select().from(events).where(and(eq(events.teamId, w.teamId), eq(events.name, SESSION_MOVED_EVENT))))
    // A queued row's re-target ran nowhere: no record.
    const queued = await sendSession({
      userId: w.owner,
      input: { teamId: w.teamId, deviceId: w.deviceId, cli: `claude`, account: `pa`, text: `not yet` },
    })
    await sendSession({ userId: w.owner, input: { sessionId: queued.sessionId, deviceId: w.target } })
    expect(await movedEvents()).toEqual([])

    // A live child's relocation: the record names its source and its continuation.
    const child = await sendSession({ userId: w.owner, input: { parentId: w.sessionId, text: `child` } })
    await caller(w.owner).claim({ sessionId: child.sessionId, deviceId: w.deviceId, branch: `exp/child` })
    const sent = await sendSession({
      userId: w.owner,
      input: { sessionId: child.sessionId, deviceId: w.target, text: `carry on there` },
    })
    const [moved] = await movedEvents()
    expect(moved).toMatchObject({
      emittedBy: `server`,
      payload: { sessionId: child.sessionId, continuationId: sent.sessionId, fromDeviceId: w.deviceId, toDeviceId: w.target },
    })
    // The continuation's brief carries it, not handed off yet.
    const host = caller(w.owner)
    const brief = await host.brief({ sessionId: sent.sessionId })
    expect(brief.moved).toMatchObject({
      moveId: moved!.id,
      sessionId: child.sessionId,
      fromDeviceId: w.deviceId,
      toDeviceId: w.target,
      handedOff: null,
    })
    expect((await host.brief({ sessionId: child.sessionId })).moved).toBeNull()

    // Only the account the SOURCE device runs under may hand it off.
    await expect(
      caller(w.mate).handedOff({ moveId: moved!.id, outcome: `pushed` })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
    expect(await host.handedOff({ moveId: moved!.id, outcome: `parked`, leftover: `exp/leftover-1a2b3c4d` })).toEqual({
      outcome: `parked`,
      leftover: `exp/leftover-1a2b3c4d`,
    })
    // A repeat (a restarted daemon, the finish and the pickup both) keeps the first.
    expect(await host.handedOff({ moveId: moved!.id, outcome: `pushed` })).toEqual({
      outcome: `parked`,
      leftover: `exp/leftover-1a2b3c4d`,
    })
    const handed = await db
      .select()
      .from(events)
      .where(and(eq(events.teamId, w.teamId), eq(events.name, SESSION_HANDED_OFF_EVENT)))
    expect(handed).toHaveLength(1)
    expect(handed[0]).toMatchObject({ emittedBy: `server`, payload: { moveId: moved!.id, outcome: `parked` } })
    expect(await host.moveState({ sessionId: sent.sessionId })).toMatchObject({
      moveId: moved!.id,
      handedOff: { outcome: `parked`, leftover: `exp/leftover-1a2b3c4d` },
    })
    // An agent's `events_emit` of the same name is no record.
    await db.insert(events).values({
      teamId: w.teamId,
      name: SESSION_MOVED_EVENT,
      payload: { sessionId: w.sessionId, continuationId: sent.sessionId, fromDeviceId: `dev-evil`, toDeviceId: w.target },
      emittedBy: `session`,
    })
    expect(await host.moveState({ sessionId: sent.sessionId })).toMatchObject({ moveId: moved!.id })
  })

  it(`EXP-1542: an ENDED root revived on another device records the move (its old device hands on)`, async () => {
    const { SESSION_MOVED_EVENT } = await import(`@/lib/sessions/move`)
    const w = await moveWorld()
    await endSession({ userId: w.owner, sessionId: w.sessionId })
    await sendSession({ userId: w.owner, input: { sessionId: w.sessionId, deviceId: w.target, text: `go on` } })
    const moves = await db
      .select()
      .from(events)
      .where(and(eq(events.teamId, w.teamId), eq(events.name, SESSION_MOVED_EVENT)))
    expect(moves).toHaveLength(1)
    expect(moves[0]!.payload).toEqual({
      sessionId: w.sessionId,
      continuationId: w.sessionId,
      fromDeviceId: w.deviceId,
      toDeviceId: w.target,
    })
    expect((await caller(w.owner).moveState({ sessionId: w.sessionId }))?.handedOff).toBeNull()
  })
})

// Chats (fixtures/chats.json menus): `sessions.delete` ends what is live,
// then deletes the root chat with its whole tree; the cascades run on the
// REAL Postgres (a fake DB hides them).
describe.skipIf(!ready.ok)(`sessions.delete (Chats)`, async () => {
  const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
  const { events, sessionReads } = await import(`@/db/schema`)
  const caller = (userId: string) =>
    sessionsRouter.createCaller({
      session: { user: { id: userId } },
      db,
      request: new Request(`http://x.test`),
    } as never)

  it(`refuses non-roots and other members, ends the live tree first, then cascades it away`, async () => {
    const w = await world({ shared: true })
    const other = await user()
    await db.insert(teamMembers).values({ teamId: w.teamId, userId: other, role: `member` })
    const [upload] = await db
      .insert(attachments)
      .values({
        teamId: w.teamId,
        uploaderId: w.mate,
        filename: `shot.png`,
        contentType: `image/png`,
        sizeBytes: 3,
        storageKey: `k-${id8()}`,
        url: `/api/attachments/x`,
      })
      .returning({ id: attachments.id })
    const issueId = await w.issue()
    const root = await sendSession({
      userId: w.mate,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `look`, attachmentIds: [upload!.id], issueIds: [issueId] },
    })
    const child = await sendSession({ userId: w.mate, input: { parentId: root.sessionId, text: `part` } })
    await caller(w.mate).markRead({ sessionId: root.sessionId })
    // A watcher outside the tree forgets the deleted ids.
    const watcher = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `w` } })
    await db
      .update(sessions)
      .set({ wakeOn: [child.sessionId] })
      .where(eq(sessions.id, watcher.sessionId))

    // Root chats only; a member who neither started it nor owns the team cannot.
    await expect(caller(w.mate).delete({ sessionId: child.sessionId })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(caller(other).delete({ sessionId: root.sessionId })).rejects.toMatchObject({ code: `FORBIDDEN` })
    await expect(caller(w.mate).delete({ sessionId: crypto.randomUUID() })).rejects.toMatchObject({ code: `NOT_FOUND` })

    // The team owner may (not the starter).
    const { txid } = await caller(w.owner).delete({ sessionId: root.sessionId })
    expect(txid).toBeTypeOf(`number`)
    const ids = [root.sessionId, child.sessionId]
    expect(await db.select().from(sessions).where(inArray(sessions.id, ids))).toEqual([])
    expect(await db.select().from(conversations).where(inArray(conversations.sessionId, ids))).toEqual([])
    expect(await db.select().from(messages).where(eq(messages.rootId, root.sessionId))).toEqual([])
    expect(await db.select().from(sessionIssues).where(inArray(sessionIssues.sessionId, ids))).toEqual([])
    expect(await db.select().from(sessionReads).where(inArray(sessionReads.sessionId, ids))).toEqual([])
    // The issue and the file stay (the file owner-less, for the orphan sweep).
    expect(await db.select({ id: issues.id }).from(issues).where(eq(issues.id, issueId))).toHaveLength(1)
    const [file] = await db.select().from(attachments).where(eq(attachments.id, upload!.id))
    expect(file).toMatchObject({ messageId: null })
    // Ended first: each live row emitted its session.ended event.
    const ended = await db
      .select({ payload: events.payload })
      .from(events)
      .where(and(eq(events.teamId, w.teamId), eq(events.name, `session.ended`)))
    expect(ended.map((row) => (row.payload as { sessionId: string }).sessionId).sort()).toEqual([...ids].sort())
    expect((await row(watcher.sessionId)).wakeOn).toEqual([])
    // A device that missed the kill: its late calls are no-ops, never NOT_FOUND.
    await expect(caller(w.mate).report({ sessionId: root.sessionId, status: `idle` })).resolves.toMatchObject({
      txid: expect.any(Number),
    })
    await expect(caller(w.mate).reply({ sessionId: root.sessionId, body: `late` })).resolves.toMatchObject({
      messageId: ``,
    })
    await expect(
      caller(w.mate).consume({ sessionId: root.sessionId, messageIds: [crypto.randomUUID()] })
    ).resolves.toMatchObject({ txid: expect.any(Number) })
  })

  it(`lets the starter delete an ended private chat, hides another's private one`, async () => {
    const w = await world()
    const root = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `x` } })
    await endSession({ userId: w.owner, sessionId: root.sessionId })
    await expect(caller(w.mate).delete({ sessionId: root.sessionId })).rejects.toMatchObject({ code: `NOT_FOUND` })
    await caller(w.owner).delete({ sessionId: root.sessionId })
    expect(await db.select().from(sessions).where(eq(sessions.id, root.sessionId))).toEqual([])
  })

  it(`refuses an agent's conversation`, async () => {
    const w = await world({ shared: true })
    const [agent] = await db
      .insert(agents)
      .values({ teamId: w.teamId, name: `Bot`, deviceId: w.deviceId, ownerId: w.owner })
      .returning({ id: agents.id })
    const sent = await sendSession({ userId: w.owner, input: { agentId: agent!.id, text: `hi` } })
    await expect(caller(w.owner).delete({ sessionId: sent.sessionId })).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })
})

// EXP-1280 custom plan mode: the card a session posts on ITSELF
// (`exponential_plan_propose` / `exponential_sessions_ask`), its pictures,
// the answer (approval leaves plan mode) and the device's consume.
describe.skipIf(!ready.ok)(`the plan card (S-3: exponential_show plan)`, async () => {
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

  it(`FEED-140: a second plan supersedes the waiting one, never delivered as an answer`, async () => {
    const w = await world({ shared: true })
    const sent = await sendSession({
      userId: w.mate,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `plan it` },
    })
    const host = caller(w.owner)
    await host.claim({ sessionId: sent.sessionId, deviceId: w.deviceId })
    const first = await postAskCard({ userId: w.mate, sessionId: sent.sessionId, card: buildPlanCard({ plan: `1. Old` }) })
    const second = await postAskCard({ userId: w.mate, sessionId: sent.sessionId, card: buildPlanCard({ plan: `1. New` }) })
    const old = await message(first.messageId)
    expect(old.card).toMatchObject({ kind: `plan`, plan: `1. Old`, decision: `superseded` })
    expect(old.answeredAt).not.toBeNull()
    // Consumed with it: the device never feeds a superseded card as an answer.
    expect(old.consumedAt).not.toBeNull()
    expect(await message(second.messageId)).toMatchObject({ answeredAt: null, consumedAt: null, card: { decision: null } })
    expect(await row(sent.sessionId)).toMatchObject({ needsInput: true })
    // A superseded card takes no answer.
    await expect(caller(w.mate).answer({ messageId: first.messageId, decision: `approved` })).rejects.toMatchObject({
      code: `CONFLICT`,
    })
    // A question never supersedes a plan.
    await postAskCard({
      userId: w.mate,
      sessionId: sent.sessionId,
      card: buildQuestionCard({ question: `Q?`, options: [{ label: `A` }] }),
    })
    expect((await message(second.messageId)).answeredAt).toBeNull()
  })

  it(`FEED-140: a tool plan never supersedes an ENGINE plan card (its parked ask stays answerable)`, async () => {
    const w = await world({ shared: true })
    const sent = await sendSession({
      userId: w.mate,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `plan it` },
    })
    await caller(w.owner).claim({ sessionId: sent.sessionId, deviceId: w.deviceId })
    const engine = await postAskCard({ userId: w.mate, sessionId: sent.sessionId, card: buildPlanCard({ plan: `1. Engine` }) })
    await db
      .update(messages)
      .set({ card: sql`${messages.card} || '{"questionId": "q-engine"}'::jsonb` })
      .where(eq(messages.id, engine.messageId))
    await postAskCard({ userId: w.mate, sessionId: sent.sessionId, card: buildPlanCard({ plan: `1. Tool` }) })
    const parked = await message(engine.messageId)
    expect(parked.answeredAt).toBeNull()
    expect(parked.consumedAt).toBeNull()
    expect(parked.card).toMatchObject({ questionId: `q-engine`, decision: null })
  })

  it(`proposes, takes its picture, is approved with answers and consumed by the device`, async () => {
    const w = await world({ shared: true })
    const sent = await sendSession({
      userId: w.mate,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `plan it` },
    })
    const host = caller(w.owner)
    await host.claim({ sessionId: sent.sessionId, deviceId: w.deviceId })

    // S-3: plan mode is gone: the brief has no Plan section.
    const { brief } = await host.brief({ sessionId: sent.sessionId })
    expect(brief).not.toContain(`## Plan`)

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
    expect(await row(sent.sessionId)).toMatchObject({ needsInput: false })

    // Only the host device consumes it, once the answer text was delivered.
    await expect(
      caller(w.mate).consume({ sessionId: sent.sessionId, messageIds: [posted.messageId] })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
    await host.consume({ sessionId: sent.sessionId, messageIds: [posted.messageId] })
    expect((await message(posted.messageId)).consumedAt).not.toBeNull()

    // A rejection keeps planning; a new plan may follow once one is answered.
    const second = await postAskCard({ userId: w.mate, sessionId: sent.sessionId, card: buildPlanCard({ plan: `2. Again` }) })
    await caller(w.mate).answer({ messageId: second.messageId, decision: `rejected` })
    expect(await row(sent.sessionId)).toMatchObject({ needsInput: false })
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

describe.skipIf(!ready.ok)(`S-3: one text + one card per turn, edited in place`, async () => {
  const { buildPlanCard, buildSurfaceCard, postShowCard } = await import(`@/lib/sessions/plan`)
  const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
  const caller = (userId: string) =>
    sessionsRouter.createCaller({
      session: { user: { id: userId } },
      db,
      request: new Request(`http://x.test`),
    } as never)
  const texts = async (sessionId: string) =>
    (await db.select().from(messages).where(inSession(sessionId)))
      .filter((m) => m.authorKind === `agent` && m.card === null)
      .sort((a, b) => a.createdAt.getTime() - b.createdAt.getTime())
      .map((m) => m.body)
  const surface = (text: string) => buildSurfaceCard({ surface: { root: { id: `t`, component: `Text`, props: { text } } } })
  // The device delivers every waiting input (the turn edge).
  const deliver = async (host: ReturnType<typeof caller>, sessionId: string) => {
    const waiting = (await db.select().from(messages).where(inSession(sessionId))).filter(
      (m) => m.authorKind === `user` && m.consumedAt === null
    )
    if (waiting.length) await host.consume({ sessionId, messageIds: waiting.map((m) => m.id) })
  }

  it(`an agent conversation's later turn text edits the one since the last person message; a person message starts a new one`, async () => {
    const w = await world()
    const [bot] = await db
      .insert(agents)
      .values({ teamId: w.teamId, name: `Bot`, deviceId: w.deviceId, ownerId: w.owner })
      .returning({ id: agents.id })
    const convo = await sendSession({ userId: w.owner, input: { agentId: bot!.id, text: `watch it` } })
    const host = caller(w.owner)
    await host.claim({ sessionId: convo.sessionId, deviceId: w.deviceId })
    const first = await host.reply({ sessionId: convo.sessionId, body: `Started.` })
    const second = await host.reply({ sessionId: convo.sessionId, body: `Half done.` })
    expect(second.messageId).toBe(first.messageId)
    expect(await texts(convo.sessionId)).toEqual([`Half done.`])
    await sendSession({ userId: w.owner, input: { sessionId: convo.sessionId, text: `and now?` } })
    await deliver(host, convo.sessionId)
    await host.reply({ sessionId: convo.sessionId, body: `Done.` })
    expect(await texts(convo.sessionId)).toEqual([`Half done.`, `Done.`])
  })

  it(`a message queued mid-turn belongs to the next turn: its reply never edits this turn's answer (F29)`, async () => {
    const w = await world()
    const [bot] = await db
      .insert(agents)
      .values({ teamId: w.teamId, name: `Bot`, deviceId: w.deviceId, ownerId: w.owner })
      .returning({ id: agents.id })
    const convo = await sendSession({ userId: w.owner, input: { agentId: bot!.id, text: `M1` } })
    const host = caller(w.owner)
    await host.claim({ sessionId: convo.sessionId, deviceId: w.deviceId })
    await deliver(host, convo.sessionId)
    // M2 arrives while turn N runs; N's answer lands after it.
    await sendSession({ userId: w.owner, input: { sessionId: convo.sessionId, text: `M2` } })
    const answer1 = await host.reply({ sessionId: convo.sessionId, body: `Answer to M1.` })
    await deliver(host, convo.sessionId)
    const answer2 = await host.reply({ sessionId: convo.sessionId, body: `Answer to M2.` })
    expect(answer2.messageId).not.toBe(answer1.messageId)
    expect(await texts(convo.sessionId)).toEqual([`Answer to M1.`, `Answer to M2.`])
  })

  it(`replace edits the latest card across turns in a chat session, whatever its title (#EXP-1446)`, async () => {
    const w = await world({ shared: true })
    const sent = await sendSession({ userId: w.mate, input: { teamId: w.teamId, deviceId: w.deviceId, text: `show it` } })
    const host = caller(w.owner)
    await host.claim({ sessionId: sent.sessionId, deviceId: w.deviceId })
    await deliver(host, sent.sessionId)
    const titled = (title: string, text: string) =>
      buildSurfaceCard({ title, surface: { root: { id: `t`, component: `Text`, props: { text } } } })
    // No earlier card: a new one.
    const first = await postShowCard({ userId: w.mate, sessionId: sent.sessionId, card: titled(`Green`, `a`), replace: true })
    expect(first.replaced).toBe(false)
    for (const [title, text] of [[`Red`, `b`], [`Shipped`, `c`]] as const) {
      await sendSession({ userId: w.mate, input: { sessionId: sent.sessionId, text: `next` } })
      await deliver(host, sent.sessionId)
      const next = await postShowCard({ userId: w.mate, sessionId: sent.sessionId, card: titled(title, text), replace: true })
      expect(next).toMatchObject({ messageId: first.messageId, replaced: true })
    }
    const cards = (await db.select().from(messages).where(inSession(sent.sessionId))).filter(
      (m) => m.card?.kind === `surface`
    )
    expect(cards).toHaveLength(1)
    expect(cards[0]!.card).toMatchObject({ title: `Shipped` })
  })

  it(`a replaced card moves to the bottom of the chat (#EXP-1446)`, async () => {
    const w = await world({ shared: true })
    const sent = await sendSession({ userId: w.mate, input: { teamId: w.teamId, deviceId: w.deviceId, text: `show it` } })
    const host = caller(w.owner)
    await host.claim({ sessionId: sent.sessionId, deviceId: w.deviceId })
    await deliver(host, sent.sessionId)
    const first = await postShowCard({ userId: w.mate, sessionId: sent.sessionId, card: surface(`a`), replace: false })
    await host.reply({ sessionId: sent.sessionId, body: `Working.` })
    await sendSession({ userId: w.mate, input: { sessionId: sent.sessionId, text: `next` } })
    await deliver(host, sent.sessionId)
    const before = (await db.select().from(messages).where(eq(messages.id, first.messageId)))[0]!
    const edited = await postShowCard({ userId: w.mate, sessionId: sent.sessionId, card: surface(`b`), replace: true })
    expect(edited).toMatchObject({ messageId: first.messageId, replaced: true })
    // The clients' transcript order: created_at, ties by id.
    const ordered = (await db.select().from(messages).where(inSession(sent.sessionId))).sort(
      (a, b) => a.createdAt.getTime() - b.createdAt.getTime() || a.id.localeCompare(b.id)
    )
    expect(ordered.length).toBeGreaterThan(2)
    expect(ordered.at(-1)!.id).toBe(first.messageId)
    expect(ordered.at(-1)!.createdAt.getTime()).toBeGreaterThan(before.createdAt.getTime())
    expect(JSON.stringify(ordered.at(-1)!.card)).toContain(`"b"`)
  })

  it(`replace edits the latest card across turns in an agent conversation, the Assistant's included (#EXP-1446)`, async () => {
    const w = await world()
    for (const role of [`agent`, `assistant`] as const) {
      const [bot] = await db
        .insert(agents)
        .values({ teamId: w.teamId, name: `Bot ${role}`, deviceId: w.deviceId, ownerId: w.owner, role })
        .returning({ id: agents.id })
      const convo = await sendSession({ userId: w.owner, input: { agentId: bot!.id, text: `M1` } })
      const host = caller(w.owner)
      await host.claim({ sessionId: convo.sessionId, deviceId: w.deviceId })
      await deliver(host, convo.sessionId)
      const first = await postShowCard({ userId: w.owner, sessionId: convo.sessionId, card: surface(`1/2`), replace: true })
      expect(first.replaced).toBe(false)
      // The member writes: a new turn (and, for the Assistant, a new status card window).
      await sendSession({ userId: w.owner, input: { sessionId: convo.sessionId, text: `M2` } })
      await deliver(host, convo.sessionId)
      const again = await postShowCard({ userId: w.owner, sessionId: convo.sessionId, card: surface(`2/2`), replace: true })
      expect(again).toMatchObject({ messageId: first.messageId, replaced: true })
      const row = (await db.select().from(messages).where(eq(messages.id, first.messageId)))[0]!
      expect(JSON.stringify(row.card)).toContain(`2/2`)
    }
  })

  it(`a chat's turn texts stay separate bubbles; its relayed copy in the parent follows an edit of the origin`, async () => {
    const w = await world()
    const root = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `root` } })
    const child = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `part` } })
    const host = caller(w.owner)
    await host.claim({ sessionId: child.sessionId, deviceId: w.deviceId })
    await host.reply({ sessionId: child.sessionId, body: `One.` })
    const two = await host.reply({ sessionId: child.sessionId, body: `Two.` })
    expect(await texts(child.sessionId)).toEqual([`One.`, `Two.`])
    const copies = (await db.select(messageWithSession()).from(messages).where(eq(messages.originMessageId, two.messageId)))
    expect(copies).toMatchObject([{ sessionId: root.sessionId, body: `Two.` }])
    const { patchBusCopies } = await import(`@/lib/sessions/messages`)
    await db.transaction((tx) => patchBusCopies(tx, two.messageId, `Two.`, `Two, edited.`))
    expect((await db.select().from(messages).where(eq(messages.id, copies[0]!.id)))[0]!.body).toBe(`Two, edited.`)
  })

  it(`replace edits the latest card of its family, across turns, and refuses an answered one`, async () => {
    const w = await world({ shared: true })
    const sent = await sendSession({ userId: w.mate, input: { teamId: w.teamId, deviceId: w.deviceId, text: `show it` } })
    const host = caller(w.owner)
    await host.claim({ sessionId: sent.sessionId, deviceId: w.deviceId })
    const first = await postShowCard({ userId: w.mate, sessionId: sent.sessionId, card: surface(`1/3`), replace: true })
    expect(first.replaced).toBe(false)
    const again = await postShowCard({ userId: w.mate, sessionId: sent.sessionId, card: surface(`2/3`), replace: true })
    expect(again).toMatchObject({ messageId: first.messageId, replaced: true })
    const row = (await db.select().from(messages).where(eq(messages.id, first.messageId)))[0]!
    expect(JSON.stringify(row.card)).toContain(`2/3`)
    // A plan is the other family: a new card, then replace edits IT.
    const plan = await postShowCard({ userId: w.mate, sessionId: sent.sessionId, card: buildPlanCard({ plan: `1. A` }), replace: true })
    expect(plan.replaced).toBe(false)
    const plan2 = await postShowCard({ userId: w.mate, sessionId: sent.sessionId, card: buildPlanCard({ plan: `1. B` }), replace: true })
    expect(plan2).toMatchObject({ messageId: plan.messageId, replaced: true })
    await caller(w.mate).answer({ messageId: plan.messageId, decision: `approved` })
    await expect(
      postShowCard({ userId: w.mate, sessionId: sent.sessionId, card: buildPlanCard({ plan: `1. C` }), replace: true })
    ).rejects.toMatchObject({ code: `CONFLICT` })
    // The person's next message delivered: a new turn still edits the
    // latest surface, past the answered plan (#EXP-1446).
    await sendSession({ userId: w.mate, input: { sessionId: sent.sessionId, text: `next` } })
    await deliver(host, sent.sessionId)
    const fresh = await postShowCard({ userId: w.mate, sessionId: sent.sessionId, card: surface(`3/3`), replace: true })
    expect(fresh).toMatchObject({ messageId: first.messageId, replaced: true })
  })
})

describe.skipIf(!ready.ok)(`S-3: seen marks, result pictures, PR pictures`, async () => {
  const { sessionReads } = await import(`@/db/schema`)
  const { buildSurfaceCard, postShowCard, publishCardPicture } = await import(`@/lib/sessions/plan`)
  const { loadPrPictures } = await import(`@/lib/pr-pictures`)
  const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
  const caller = (userId: string) =>
    sessionsRouter.createCaller({
      session: { user: { id: userId } },
      db,
      request: new Request(`http://x.test`),
    } as never)
  const surface = { root: { id: `t`, component: `Text`, props: { text: `Green` } } }

  it(`markStoriesSeen keeps the greatest mark on the ROOT's row and never marks the chat read`, async () => {
    const w = await world()
    const root = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `root` } })
    const child = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `part` } })
    const me = caller(w.owner)
    await me.markStoriesSeen({ sessionId: child.sessionId, at: 2_000_000_000_000 })
    await me.markStoriesSeen({ sessionId: root.sessionId, at: 1_000_000_000_000 })
    const [read] = await db
      .select()
      .from(sessionReads)
      .where(and(eq(sessionReads.sessionId, root.sessionId), eq(sessionReads.userId, w.owner)))
    // Capped at now, then never lowered.
    expect(read!.storySeenAt!.getTime()).toBeLessThanOrEqual(Date.now())
    expect(read!.storySeenAt!.getTime()).toBeGreaterThan(1_000_000_000_000)
    expect(read!.readAt.getTime()).toBe(0)
  })

  it(`a result's picture resolves on its card and is a PR picture of the opener's subtree only`, async () => {
    const w = await world()
    const root = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `root` } })
    const child = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `ios part` } })
    const card = buildSurfaceCard({ surface, pictures: [{ label: `ios chats` }] })
    const posted = await postShowCard({ userId: w.owner, sessionId: child.sessionId, card, replace: false })
    expect(posted.uploads.map((upload) => upload.label)).toEqual([`ios chats`])
    const attachmentId = crypto.randomUUID()
    await publishCardPicture(
      { messageId: posted.messageId, sessionId: child.sessionId, teamId: w.teamId, label: `ios chats`, uploaderId: w.owner },
      { attachmentId, storageKey: `s3/${attachmentId}`, url: ``, width: 10, height: 20, contentType: `image/png`, sizeBytes: 1, filename: `a.png` }
    )
    const row = (await db.select().from(messages).where(eq(messages.id, posted.messageId)))[0]!
    expect((row.card as { pictures: { attachmentId: string }[] }).pictures[0]!.attachmentId).toBe(attachmentId)
    expect((await loadPrPictures(root.sessionId, root.sessionId)).map((p) => p.label)).toEqual([`ios chats`])
    const sibling = await sendSession({ userId: w.owner, input: { parentId: root.sessionId, text: `web part` } })
    expect(await loadPrPictures(root.sessionId, sibling.sessionId)).toEqual([])
  })

  it(`replace keeps the pictures already uploaded and grants only the new labels (F37)`, async () => {
    const w = await world()
    const root = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `root` } })
    const posted = await postShowCard({
      userId: w.owner,
      sessionId: root.sessionId,
      card: buildSurfaceCard({ surface, pictures: [{ label: `ios chats` }] }),
      replace: false,
    })
    const attachmentId = crypto.randomUUID()
    await publishCardPicture(
      { messageId: posted.messageId, sessionId: root.sessionId, teamId: w.teamId, label: `ios chats`, uploaderId: w.owner },
      { attachmentId, storageKey: `s3/${attachmentId}`, url: ``, width: 10, height: 20, contentType: `image/png`, sizeBytes: 1, filename: `a.png` }
    )
    const replaced = await postShowCard({
      userId: w.owner,
      sessionId: root.sessionId,
      card: buildSurfaceCard({ surface, title: `Now`, pictures: [{ label: `ios chats` }, { label: `web chats` }] }),
      replace: true,
    })
    expect(replaced).toMatchObject({ messageId: posted.messageId, replaced: true })
    expect(replaced.uploads.map((upload) => upload.label)).toEqual([`web chats`])
    const row = (await db.select().from(messages).where(eq(messages.id, posted.messageId)))[0]!
    expect((row.card as { title: string; pictures: unknown[] }).title).toBe(`Now`)
    expect((row.card as { pictures: unknown[] }).pictures).toMatchObject([
      { label: `ios chats`, attachmentId, width: 10, height: 20 },
      { label: `web chats`, attachmentId: null },
    ])
    expect((await loadPrPictures(root.sessionId, root.sessionId)).map((p) => p.label)).toEqual([`ios chats`])
  })
})

describe.skipIf(!ready.ok)(`sessions.delete — a batch of chats`, () => {
  it(`deletes every selected root and its tree, one call per chat`, async () => {
    const { deleteSession } = await import(`@/lib/sessions/lifecycle`)
    const w = await world({ shared: true })
    const roots: string[] = []
    for (let i = 0; i < 10; i++) {
      const sent = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `chat ${i}` } })
      await sendSession({ userId: w.owner, input: { parentId: sent.sessionId, text: `child ${i}` } })
      roots.push(sent.sessionId)
    }
    for (const id of roots) await deleteSession({ userId: w.owner, sessionId: id })
    expect(await db.select({ id: sessions.id }).from(sessions).where(eq(sessions.teamId, w.teamId))).toEqual([])
  })
})
