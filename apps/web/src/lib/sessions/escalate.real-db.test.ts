// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, asc, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, boards, devices, messages, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

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
    .select()
    .from(messages)
    .where(and(eq(messages.sessionId, sessionId), sql`${messages.card}->>'kind' = 'ask'`))
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
