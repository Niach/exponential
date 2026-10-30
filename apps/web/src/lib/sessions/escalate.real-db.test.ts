// @vitest-environment node
import { readFileSync } from "node:fs"
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, asc, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, boards, conversations, devices, messages, sessions, teamMembers, teams } from "@/db/schema"
import { inRollback, probeRealDb } from "@/test/real-db"
import { inSession, sessionConversation, messageWithSession } from "@/lib/sessions/messages"

// #EXP-1399: ESCALATED cards against the REAL migrated Postgres (copied
// harness from bus.real-db.test.ts). NEW-1 recap: THE server bus. Every
// transition of a session reaches its PARENT's conversation and every live
// session whose `wake_on` names it: a turn's reply, an open card (the `ask`
// mirror), a wall, a PR, its claim and its end (the `report` card). Waking
// rows are authored BY the child (`author_session_id`) with `consumed_at`
// NULL; informational ones are pre-consumed; with nobody listening an event
// line lands on the child itself. Also `sessions.send`'s `wakeOn` and
// `answer`, and End's deepest-first reports. Each case builds a fresh team;
// the suite deletes every team (and user) it made (cascade).

const notifyBlocked = vi.hoisted(() => vi.fn(async (..._args: unknown[]) => {}))
vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: notifyBlocked,
}))
// The relay frames only kill latency: never reached from here.
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
if (!ready.ok) console.info(`[escalate.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { escalateCard } = await import(`@/lib/sessions/escalate`)
const { buildPlanCard, buildQuestionCard, postAskCard } = await import(`@/lib/sessions/plan`)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
const { chatPreview } = await import(`@exp/domain-contract/chats`)
const { askHiddenText, watchSessions } = await import(`@/lib/sessions/bus`)

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

async function world() {
  const owner = await user()
  const [team] = await db
    .insert(teams)
    .values({ name: `Bus`, slug: `bus-${id8()}` })
    .returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `BU` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const root = async (text = `root`) =>
    (await sendSession({ userId: owner, input: { teamId, deviceId, text } })).sessionId
  const child = async (parentId: string, text = `child`) =>
    (await sendSession({ userId: owner, input: { parentId, text } })).sessionId
  return { owner, teamId, deviceId, root, child, host: caller(owner) }
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



/** An agent, its conversation with the owner, a thread the conversation
 *  started (watched by it) and a child of that thread. */
async function managed() {
  const w = await world()
  const [agent] = await db
    .insert(agents)
    .values({ teamId: w.teamId, name: `Big Brother`, deviceId: w.deviceId, ownerId: w.owner })
    .returning({ id: agents.id })
  const conversation = (await sendSession({ userId: w.owner, input: { agentId: agent!.id, text: `watch` } })).sessionId
  const thread = (
    await sendSession({
      userId: w.owner,
      callerSessionId: conversation,
      viaMcp: true,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `fix the cards` },
    })
  ).sessionId
  const childId = await w.child(thread)
  return { ...w, conversation, thread, childId }
}

const asks = (sessionId: string) =>
  db
    .select(messageWithSession())
    .from(messages)
    .where(and(inSession(sessionId), sql`${messages.card}->>'kind' = 'ask'`))
    .orderBy(asc(messages.createdAt), asc(messages.id))

describe.skipIf(!ready.ok)(`escalate: a managed thread's card in its agent's conversation`, () => {
  it(`the thread's own card reaches the conversation (it watches the root); answering closes every mirror`, async () => {
    const w = await managed()
    const own = await postAskCard({
      userId: w.owner,
      sessionId: w.thread,
      card: buildQuestionCard({ question: `Ship it?`, options: [{ label: `yes` }, { label: `no` }] }),
    })
    const [mirror] = await asks(w.conversation)
    expect(mirror).toMatchObject({ authorSessionId: w.thread, card: { kind: `ask`, sessionId: w.thread, messageId: own.messageId } })
    expect(mirror!.answeredAt).toBeNull()
    // Already there: escalate answers with the existing mirror.
    expect(await escalateCard({ userId: w.owner, caller: await row(w.thread), messageId: own.messageId })).toEqual({
      messageId: mirror!.id,
      sessionId: w.conversation,
      escalated: false,
    })
    await caller(w.owner).answer({ messageId: own.messageId, answers: [`yes`] })
    const [closed] = await asks(w.conversation)
    expect(closed!.answeredAt).not.toBeNull()
  })

  it(`a child's card escalates once into the conversation, waking it; the answer closes both mirrors`, async () => {
    const w = await managed()
    const asked = await postAskCard({
      userId: w.owner,
      sessionId: w.childId,
      card: buildQuestionCard({ question: `Which port?`, options: [{ label: `54398` }] }),
    })
    // The bus mirrored it into the child's parent (the thread) only.
    expect((await asks(w.thread)).map((m) => (m.card as { messageId: string }).messageId)).toEqual([asked.messageId])
    expect(await asks(w.conversation)).toEqual([])
    // The thread escalates its child's card by the mirror's id.
    const [threadMirror] = await asks(w.thread)
    const first = await escalateCard({ userId: w.owner, caller: await row(w.thread), messageId: threadMirror!.id })
    expect(first).toMatchObject({ sessionId: w.conversation, escalated: true })
    const [mirror] = await asks(w.conversation)
    expect(mirror).toMatchObject({
      id: first.messageId,
      authorKind: `agent`,
      authorSessionId: w.childId,
      consumedAt: null,
      card: { kind: `ask`, sessionId: w.childId, messageId: asked.messageId },
    })
    expect(mirror!.body).toContain(`Asks: Which port?`)
    // Idempotent; the conversation itself may lift it too (pre-consumed then).
    expect(await escalateCard({ userId: w.owner, caller: await row(w.conversation), messageId: asked.messageId })).toEqual({
      messageId: first.messageId,
      sessionId: w.conversation,
      escalated: false,
    })
    // ONE answer source: the origin card; every mirror closes with it.
    await caller(w.owner).answer({ messageId: asked.messageId, answer: `54398` })
    for (const m of [...(await asks(w.thread)), ...(await asks(w.conversation))]) expect(m.answeredAt).not.toBeNull()
    await expect(
      escalateCard({ userId: w.owner, caller: await row(w.thread), messageId: asked.messageId })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`refuses a caller outside the tree and a thread no agent manages`, async () => {
    const w = await managed()
    const asked = await postAskCard({
      userId: w.owner,
      sessionId: w.childId,
      card: buildQuestionCard({ question: `Private?`, options: [{ label: `no` }] }),
    })
    const stranger = await w.root(`elsewhere`)
    await expect(
      escalateCard({ userId: w.owner, caller: await row(stranger), messageId: asked.messageId })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
    const plainChild = await w.child(stranger)
    const plain = await postAskCard({
      userId: w.owner,
      sessionId: plainChild,
      card: buildQuestionCard({ question: `Nobody?`, options: [{ label: `x` }] }),
    })
    await expect(
      escalateCard({ userId: w.owner, caller: await row(plainChild), messageId: plain.messageId })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`a superseded plan closes its mirrors`, async () => {
    const w = await managed()
    const first = await postAskCard({ userId: w.owner, sessionId: w.thread, card: buildPlanCard({ plan: `One` }) })
    await postAskCard({ userId: w.owner, sessionId: w.thread, card: buildPlanCard({ plan: `Two` }) })
    const mirrors = await asks(w.conversation)
    const old = mirrors.find((m) => (m.card as { messageId: string }).messageId === first.messageId)
    expect(old!.answeredAt).not.toBeNull()
    expect(mirrors.filter((m) => m.answeredAt === null)).toHaveLength(1)
  })
})

/** #EXP-1399 review: the agent on a device SHARED with the team (so its
 *  conversation is shared) starts a thread on the owner's PRIVATE device. */
async function privateUnderShared() {
  const w = await world()
  const sharedDevice = `dev-${id8()}`
  await db.insert(devices).values({ userId: w.owner, deviceId: sharedDevice, label: `team box`, kind: `server`, sharedTeamIds: [w.teamId] })
  const [agent] = await db
    .insert(agents)
    .values({ teamId: w.teamId, name: `Big Brother`, deviceId: sharedDevice, ownerId: w.owner })
    .returning({ id: agents.id })
  const conversation = (await sendSession({ userId: w.owner, input: { agentId: agent!.id, text: `watch` } })).sessionId
  const thread = (
    await sendSession({
      userId: w.owner,
      callerSessionId: conversation,
      viaMcp: true,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `secret work` },
    })
  ).sessionId
  const childId = await w.child(thread)
  return { ...w, agentId: agent!.id, sharedDevice, conversation, thread, childId }
}

const SECRET = `Rotate the prod key xyz-123?`

const preview = (rootId: string, rows: (typeof messages.$inferSelect & { sessionId: string })[], visibleSessionIds?: string[]) =>
  chatPreview({
    rootId,
    messages: rows.map((m) => ({
      ...m,
      createdAt: m.createdAt.toISOString(),
      answeredAt: m.answeredAt?.toISOString() ?? null,
      consumedAt: m.consumedAt?.toISOString() ?? null,
    })) as never,
    needsYou: true,
    busy: false,
    visibleSessionIds,
  })

describe.skipIf(!ready.ok)(`escalate review: a private card never reaches readers who may not see it`, () => {
  it(`a private root's card mirrored into a shared conversation carries no text, its preview neither`, async () => {
    const w = await privateUnderShared()
    expect((await row(w.conversation)).shared).toBe(true)
    expect((await row(w.thread)).shared).toBe(false)
    const own = await postAskCard({
      userId: w.owner,
      sessionId: w.thread,
      card: buildQuestionCard({ question: SECRET, options: [{ label: `yes` }] }),
    })
    const mirrors = await asks(w.conversation)
    expect(mirrors).toHaveLength(1)
    expect(mirrors[0]).toMatchObject({ card: { kind: `ask`, sessionId: w.thread, messageId: own.messageId }, consumedAt: null })
    expect(mirrors[0]!.body).not.toContain(`xyz-123`)
    expect(mirrors[0]!.body).toContain(w.thread)
    // A teammate's list (they sync the conversation, not the thread).
    expect(preview(w.conversation, mirrors, [w.conversation])).toEqual({ kind: `question`, text: `Question in a private chat` })
    // Even with no visibility input, the neutral body reads as private.
    expect(preview(w.conversation, mirrors).text).not.toContain(`xyz-123`)
    // A plan: the same.
    await postAskCard({ userId: w.owner, sessionId: w.thread, card: buildPlanCard({ plan: `## ${SECRET}` }) })
    for (const m of await asks(w.conversation)) expect(m.body).not.toContain(`xyz-123`)
  })

  it(`escalate never picks a shared conversation for a private origin`, async () => {
    const w = await privateUnderShared()
    const asked = await postAskCard({
      userId: w.owner,
      sessionId: w.childId,
      card: buildQuestionCard({ question: SECRET, options: [{ label: `no` }] }),
    })
    await expect(
      escalateCard({ userId: w.owner, caller: await row(w.thread), messageId: asked.messageId })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    expect(await asks(w.conversation)).toEqual([])
  })

  it(`another member's conversation is never chosen; the starter's own wins`, async () => {
    const w = await managed()
    const other = await user()
    await db.insert(teamMembers).values({ teamId: w.teamId, userId: other, role: `member` })
    const otherDevice = `dev-${id8()}`
    await db.insert(devices).values({ userId: other, deviceId: otherDevice, label: `theirs`, kind: `server` })
    const [theirs] = await db
      .insert(agents)
      .values({ teamId: w.teamId, name: `Their agent`, deviceId: otherDevice, ownerId: other })
      .returning({ id: agents.id })
    const theirConversation = (await sendSession({ userId: other, input: { agentId: theirs!.id, text: `hi` } })).sessionId
    // watchSessions refuses a private session the watcher's account cannot see.
    expect(await db.transaction((tx) => watchSessions(tx, theirConversation, [w.thread]))).toEqual([])
    expect((await row(theirConversation)).wakeOn ?? []).not.toContain(w.thread)
    // Even a (legacy) watch row of theirs never wins, ordered first by id or not.
    await db
      .update(sessions)
      .set({ wakeOn: sql`coalesce(${sessions.wakeOn}, '[]'::jsonb) || ${JSON.stringify([w.thread])}::jsonb` })
      .where(eq(sessions.id, theirConversation))
    const asked = await postAskCard({
      userId: w.owner,
      sessionId: w.childId,
      card: buildQuestionCard({ question: SECRET, options: [{ label: `no` }] }),
    })
    const [threadMirror] = await asks(w.thread)
    expect(await escalateCard({ userId: w.owner, caller: await row(w.thread), messageId: threadMirror!.id })).toMatchObject({
      sessionId: w.conversation,
      escalated: true,
    })
    expect(await asks(theirConversation)).toEqual([])
    expect((await asks(w.conversation)).map((m) => (m.card as { messageId: string }).messageId)).toContain(asked.messageId)
  })

  it(`two escalates at once insert ONE mirror; an answer first refuses the escalate`, async () => {
    const w = await managed()
    const asked = await postAskCard({
      userId: w.owner,
      sessionId: w.childId,
      card: buildQuestionCard({ question: `Race?`, options: [{ label: `a` }] }),
    })
    const results = await Promise.all([
      escalateCard({ userId: w.owner, caller: await row(w.thread), messageId: asked.messageId }),
      escalateCard({ userId: w.owner, caller: await row(w.thread), messageId: asked.messageId }),
    ])
    expect(results.filter((r) => r.escalated)).toHaveLength(1)
    expect(new Set(results.map((r) => r.messageId)).size).toBe(1)
    expect(await asks(w.conversation)).toHaveLength(1)
    await expect(
      db.insert(messages).values({
        conversationId: sessionConversation(w.conversation),
        authorKind: `agent`,
        body: `dup`,
        card: { kind: `ask`, sessionId: w.childId, messageId: asked.messageId },
      } as never)
    ).rejects.toThrow()
    const second = await postAskCard({
      userId: w.owner,
      sessionId: w.childId,
      card: buildQuestionCard({ question: `Late?`, options: [{ label: `b` }] }),
    })
    await caller(w.owner).answer({ messageId: second.messageId, answers: [`b`] })
    await expect(
      escalateCard({ userId: w.owner, caller: await row(w.thread), messageId: second.messageId })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`an origin that ends or is deleted unanswered closes its mirrors`, async () => {
    const w = await managed()
    const asked = await postAskCard({
      userId: w.owner,
      sessionId: w.childId,
      card: buildQuestionCard({ question: `Still there?`, options: [{ label: `x` }] }),
    })
    await escalateCard({ userId: w.owner, caller: await row(w.thread), messageId: asked.messageId })
    await caller(w.owner).end({ sessionId: w.childId })
    for (const m of [...(await asks(w.thread)), ...(await asks(w.conversation))]) expect(m.answeredAt).not.toBeNull()
    // Delete: the root's own card mirrored into the conversation closes too.
    const own = await postAskCard({
      userId: w.owner,
      sessionId: w.thread,
      card: buildQuestionCard({ question: `Delete me?`, options: [{ label: `x` }] }),
    })
    await caller(w.owner).delete({ sessionId: w.thread })
    const left = (await asks(w.conversation)).find((m) => (m.card as { messageId: string }).messageId === own.messageId)
    expect(left!.answeredAt).not.toBeNull()
  })
})

describe.skipIf(!ready.ok)(`0198 migration: ask mirrors from before the review lose their plaintext`, () => {
  // The scrub = 0198's first statement: every `ask` mirror's body becomes
  // `askHiddenText`, whatever the reader may see (clients draw the origin).
  const scrub = readFileSync(
    new URL(`../../db/out/0198_exp1399_ask_mirror_unique.sql`, import.meta.url),
    `utf8`
  ).split(`--> statement-breakpoint`)[0]!

  it(`a plaintext question and plan mirror become the neutral text; the digest drops the card text`, async () => {
    const w = await managed()
    const question = buildQuestionCard({ question: `Which secret port?`, options: [{ label: `54398` }] })
    const plan = buildPlanCard({ plan: `Rotate the secret key` })
    const asked = await postAskCard({ userId: w.owner, sessionId: w.childId, card: question })
    const planned = await postAskCard({ userId: w.owner, sessionId: w.childId, card: plan })
    // What master's busAsk wrote: the card text, in plaintext.
    const before = await asks(w.thread)
    expect(before.map((m) => m.body).join(`\n`)).toContain(`Which secret port?`)
    expect(before.map((m) => m.body).join(`\n`)).toContain(`Rotate the secret key`)
    await inRollback(async (tx) => {
      await tx.execute(sql.raw(scrub))
      const after = await tx
        .select()
        .from(messages)
        .where(and(inSession(w.thread), sql`${messages.card}->>'kind' = 'ask'`))
      const byOrigin = new Map(after.map((m) => [(m.card as { messageId: string }).messageId, m.body]))
      expect(byOrigin.get(asked.messageId)).toBe(askHiddenText({ id: w.childId }, question))
      expect(byOrigin.get(planned.messageId)).toBe(askHiddenText({ id: w.childId }, plan))
      const [conv] = await tx
        .select({ digest: conversations.digest })
        .from(conversations)
        .where(eq(conversations.id, after[0]!.conversationId!))
      expect(JSON.stringify(conv!.digest)).not.toMatch(/secret/)
      // Idempotent: a second run rewrites nothing.
      const again = await tx.execute(sql.raw(scrub))
      expect((again as unknown as { rowCount: number }).rowCount).toBe(0)
    })
  })
})
