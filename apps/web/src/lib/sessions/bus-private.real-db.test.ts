// @vitest-environment node
import { readFileSync } from "node:fs"
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, asc, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, boards, conversations, devices, messages, repositories, sessions, teamMembers, teams } from "@/db/schema"
import { inRollback, probeRealDb } from "@/test/real-db"

// #EXP-1408 against the REAL migrated Postgres (harness copied from
// escalate.real-db.test.ts): a PRIVATE chat's bus lines (its turn replies,
// its end report, its PR line) never carry its text into a conversation
// whose readers may not see it (`askHidden`), only the neutral
// `privateText` naming the session id; a visible origin keeps its text.
// Also the read paths: the Chats preview, MCP `sessions get` (the tree)
// and the 0199 scrub of rows written before.

vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
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
  SELECT (
    EXISTS (SELECT 1 FROM information_schema.columns
      WHERE table_name = 'sessions' AND column_name = 'wake_on')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
  ) AS ok
`)
if (!ready.ok) console.info(`[bus-private.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { endSession } = await import(`@/lib/sessions/lifecycle`)
const { BUS_COPY, busLine, privateText } = await import(`@/lib/sessions/bus`)
const { insertMessage } = await import(`@/lib/sessions/messages`)
const { recordPullRequestOpened } = await import(`@/lib/integrations/pr-sync`)
const { getSessionTree } = await import(`@/lib/sessions/tree`)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
const { PRIVATE_PR_LEAD, PRIVATE_REPLY_LEAD, PRIVATE_REPORT_LEAD } = await import(
  `@exp/domain-contract/chats`
)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `bus-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@bus.test`}, true, false, now(), now())
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

const caller = (userId: string) =>
  sessionsRouter.createCaller({
    session: { user: { id: userId } },
    db,
    request: new Request(`http://x.test`),
  } as never)

const row = async (id: string) =>
  (await db.select().from(sessions).where(eq(sessions.id, id)).limit(1))[0]!

/** An agent conversation that started a thread and watches it, plus a
 *  child of the thread and a teammate. `shared` = the agent runs on a
 *  device shared with the team (its conversation is shared; the thread on
 *  the owner's private device is not). */
async function managed(shared: boolean) {
  const owner = await user()
  const [team] = await db
    .insert(teams)
    .values({ name: `Bus`, slug: `bus-${id8()}` })
    .returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  const mate = await user()
  await db.insert(teamMembers).values([
    { teamId, userId: owner, role: `owner` },
    { teamId, userId: mate, role: `member` },
  ])
  await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `BU` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const agentDevice = shared ? `dev-${id8()}` : deviceId
  if (shared) {
    await db
      .insert(devices)
      .values({ userId: owner, deviceId: agentDevice, label: `team box`, kind: `server`, sharedTeamIds: [teamId] })
  }
  const [agent] = await db
    .insert(agents)
    .values({ teamId, name: `Big Brother`, deviceId: agentDevice, ownerId: owner })
    .returning({ id: agents.id })
  const conversation = (await sendSession({ userId: owner, input: { agentId: agent!.id, text: `watch` } })).sessionId
  const thread = (
    await sendSession({
      userId: owner,
      callerSessionId: conversation,
      viaMcp: true,
      input: { teamId, deviceId, text: `secret work` },
    })
  ).sessionId
  const childId = (await sendSession({ userId: owner, input: { parentId: thread, text: `child` } })).sessionId
  expect((await row(conversation)).shared).toBe(shared)
  expect((await row(thread)).shared).toBe(false)
  expect((await row(conversation)).wakeOn).toContain(thread)
  return { owner, mate, teamId, deviceId, conversation, thread, childId, host: caller(owner) }
}

const SECRET = `Rotated the prod key xyz-123.`

/** The rows `from` posted into `sessionId` (the bus's), oldest first. */
const fromSession = (sessionId: string, from: string) =>
  db
    .select()
    .from(messages)
    .where(and(eq(messages.sessionId, sessionId), eq(messages.authorSessionId, from)))
    .orderBy(asc(messages.createdAt), asc(messages.id))

const kindOf = (m: { card: unknown }) => (m.card as { kind?: string } | null)?.kind ?? null
const eventOf = (m: { card: unknown }) => (m.card as { event?: string } | null)?.event ?? null

async function openPr(w: Awaited<ReturnType<typeof managed>>, sessionId: string, number: number) {
  const fullName = `o/bus-${id8()}`
  const [repo] = await db
    .insert(repositories)
    .values({ teamId: w.teamId, fullName, defaultBranch: `main` })
    .returning({ id: repositories.id })
  return (
    await recordPullRequestOpened({
      teamId: w.teamId,
      rootSessionId: w.thread,
      sessionId,
      repositoryId: repo!.id,
      repoFullName: fullName,
      number,
      url: `https://github.com/${fullName}/pull/${number}`,
      title: SECRET,
      head: `exp/secret-${id8()}`,
      base: `main`,
      issueIds: [],
      actorUserId: w.owner,
    })
  ).pullRequest
}

describe.skipIf(!ready.ok)(`#EXP-1408: a private chat's bus lines into a shared conversation`, () => {
  it(`S-3: a turn reply never reaches the watching agent conversation; the thread's own parent keeps the text`, async () => {
    const w = await managed(true)
    await w.host.claim({ sessionId: w.thread, deviceId: w.deviceId })
    await w.host.reply({ sessionId: w.thread, body: SECRET })
    expect((await fromSession(w.conversation, w.thread)).filter((m) => m.card === null)).toEqual([])
    // Inside the private tree (same reader) the child's reply reaches its parent whole.
    await w.host.claim({ sessionId: w.childId, deviceId: w.deviceId })
    await w.host.reply({ sessionId: w.childId, body: SECRET })
    expect((await fromSession(w.thread, w.childId)).filter((m) => m.card === null)).toMatchObject([{ body: SECRET }])
  })

  it(`the end report arrives neutral (body and card summary); the summary from the last reply too`, async () => {
    const w = await managed(true)
    await w.host.claim({ sessionId: w.thread, deviceId: w.deviceId })
    await w.host.reply({ sessionId: w.thread, body: `Last words: ${SECRET}` })
    await endSession({ userId: w.owner, sessionId: w.thread, summary: SECRET })
    const reports = (await fromSession(w.conversation, w.thread)).filter((m) => kindOf(m) === `report`)
    expect(reports).toMatchObject([
      {
        authorKind: `agent`,
        body: privateText({ id: w.thread }, PRIVATE_REPORT_LEAD),
        consumedAt: null,
        card: { kind: `report`, sessionId: w.thread, summary: PRIVATE_REPORT_LEAD, final: true },
      },
    ])
    // The child's report into the private thread keeps its text.
    const [childReport] = (await fromSession(w.thread, w.childId)).filter((m) => kindOf(m) === `report`)
    expect(childReport!.card).toMatchObject({ kind: `report`, summary: expect.not.stringContaining(PRIVATE_REPORT_LEAD) })
    const all = await db.select().from(messages).where(eq(messages.sessionId, w.conversation))
    expect(JSON.stringify(all)).not.toContain(`xyz-123`)
  })

  it(`a PR line names the session only (no pullRequestId); every other event is the server's own copy`, async () => {
    const w = await managed(true)
    await openPr(w, w.thread, 11)
    const lines = (await fromSession(w.conversation, w.thread)).filter((m) => eventOf(m) === `pr_opened`)
    expect(lines).toEqual([
      expect.objectContaining({
        authorKind: `system`,
        body: privateText({ id: w.thread }, PRIVATE_PR_LEAD),
        consumedAt: null,
        card: { kind: `event`, event: `pr_opened`, sessionId: w.thread },
      }),
    ])
    await db.transaction((tx) => busLine(tx, { id: w.thread, parentId: null }, `session_stuck`, BUS_COPY.session_stuck(31), { waking: true }))
    const stuck = (await fromSession(w.conversation, w.thread)).filter((m) => eventOf(m) === `session_stuck`)
    expect(stuck).toMatchObject([{ body: BUS_COPY.session_stuck(31) }])
    // The claim line likewise.
    await w.host.claim({ sessionId: w.thread, deviceId: w.deviceId })
    expect((await fromSession(w.conversation, w.thread)).find((m) => eventOf(m) === `session_claimed`)).toMatchObject({
      body: BUS_COPY.session_claimed,
    })
  })

  it(`MCP sessions get of the conversation (a teammate, or its own agent) reads no private text`, async () => {
    const w = await managed(true)
    await w.host.claim({ sessionId: w.thread, deviceId: w.deviceId })
    await w.host.reply({ sessionId: w.thread, body: SECRET })
    await openPr(w, w.thread, 12)
    await endSession({ userId: w.owner, sessionId: w.thread, summary: SECRET })
    for (const reader of [w.mate, w.owner]) {
      const tree = await getSessionTree(reader, w.conversation, { messages: 100 })
      expect(JSON.stringify(tree)).not.toContain(`xyz-123`)
      expect(JSON.stringify(tree)).toContain(w.thread)
    }
    // The owner's agent reads the thread itself with its own auth; the teammate may not.
    expect(JSON.stringify(await getSessionTree(w.owner, w.thread, { messages: 100 }))).toContain(`xyz-123`)
    await expect(getSessionTree(w.mate, w.thread, { messages: 100 })).rejects.toBeTruthy()
  })
})

describe.skipIf(!ready.ok)(`#EXP-1408: a visible origin keeps its text`, () => {
  it(`a private conversation of the same member gets the reply, the report and the PR line whole`, async () => {
    const w = await managed(false)
    await w.host.claim({ sessionId: w.thread, deviceId: w.deviceId })
    await w.host.reply({ sessionId: w.thread, body: SECRET })
    const pr = await openPr(w, w.thread, 13)
    await endSession({ userId: w.owner, sessionId: w.thread, summary: SECRET })
    const rows = await fromSession(w.conversation, w.thread)
    // S-3: no turn relay into a watching agent conversation.
    expect(rows.find((m) => m.card === null && m.authorKind === `agent`)).toBeUndefined()
    expect(rows.find((m) => kindOf(m) === `report`)).toMatchObject({
      body: SECRET,
      card: { kind: `report`, summary: SECRET },
    })
    expect(rows.find((m) => eventOf(m) === `pr_opened`)).toMatchObject({
      body: BUS_COPY.pr_opened(13, pr.url),
      card: { pullRequestId: pr.id },
    })
  })
})

describe.skipIf(!ready.ok)(`0199 migration: private bus lines from before lose their text`, () => {
  const scrub = readFileSync(new URL(`../../db/out/0199_exp1408_private_bus_lines.sql`, import.meta.url), `utf8`)
    .split(`--> statement-breakpoint`)
    .map((statement) => statement.trim())
    .filter(Boolean)

  it(`relays, reports and PR lines in a shared conversation become neutral; sends and visible rows stay`, async () => {
    const shared = await managed(true)
    const visible = await managed(false)
    // What master's bus wrote: the text, in plaintext (the reply's own row
    // and its relay in one transaction, so one instant).
    const legacy = async (w: Awaited<ReturnType<typeof managed>>) =>
      db.transaction(async (tx) => {
        await insertMessage(tx, w.thread, { authorKind: `agent`, body: SECRET })
        const reply = await insertMessage(tx, w.conversation, { authorKind: `agent`, authorSessionId: w.thread, body: SECRET })
        const report = await insertMessage(tx, w.conversation, {
          authorKind: `agent`,
          authorSessionId: w.thread,
          body: SECRET,
          card: { kind: `report`, sessionId: w.thread, summary: SECRET, final: true },
        })
        const pr = await insertMessage(tx, w.conversation, {
          authorKind: `system`,
          authorSessionId: w.thread,
          body: `Opened PR #9: https://github.com/o/secret/pull/9`,
          card: { kind: `event`, event: `pr_opened`, sessionId: w.thread, pullRequestId: crypto.randomUUID() },
        })
        return { reply, report, pr }
      })
    const hidden = await legacy(shared)
    const kept = await legacy(visible)
    // A session's own send into the conversation (another instant): its author wrote it there.
    const send = await db.transaction((tx) =>
      insertMessage(tx, shared.conversation, { authorKind: `agent`, authorSessionId: shared.thread, body: `Sent on purpose.` })
    )
    await inRollback(async (tx) => {
      for (const statement of scrub) await tx.execute(sql.raw(statement))
      const byId = async (id: string) => (await tx.select().from(messages).where(eq(messages.id, id)))[0]!
      expect(await byId(hidden.reply)).toMatchObject({ body: privateText({ id: shared.thread }, PRIVATE_REPLY_LEAD) })
      expect(await byId(hidden.report)).toMatchObject({
        body: privateText({ id: shared.thread }, PRIVATE_REPORT_LEAD),
        card: { kind: `report`, sessionId: shared.thread, summary: PRIVATE_REPORT_LEAD, final: true },
      })
      expect(await byId(hidden.pr)).toMatchObject({
        body: privateText({ id: shared.thread }, PRIVATE_PR_LEAD),
        card: { kind: `event`, event: `pr_opened`, sessionId: shared.thread },
      })
      expect((await byId(hidden.pr)).card).not.toHaveProperty(`pullRequestId`)
      expect(await byId(send)).toMatchObject({ body: `Sent on purpose.` })
      for (const id of Object.values(kept)) expect((await byId(id)).body).not.toContain(`A private chat`)
      const [conv] = await tx
        .select({ digest: conversations.digest })
        .from(conversations)
        .where(eq(conversations.sessionId, shared.conversation))
      expect(JSON.stringify(conv!.digest)).not.toContain(`xyz-123`)
      // Idempotent: a second run rewrites nothing.
      for (const statement of scrub) {
        const again = await tx.execute(sql.raw(statement))
        expect((again as unknown as { rowCount: number }).rowCount).toBe(0)
      }
    })
  })
})
