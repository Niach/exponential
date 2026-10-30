// @vitest-environment node
import { readFileSync } from "node:fs"
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, asc, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, boards, devices, messages, sessions, teamMembers, teams } from "@/db/schema"
import { inRollback, probeRealDb } from "@/test/real-db"
import { inSession, insertMessage, messageWithSession } from "@/lib/sessions/messages"

// EXP-1458 (fixtures/sealed-content.json): the server-side READERS of
// message text never read a sealed row: the display title skips sealed
// inputs, the brief hands a sealed first message back as `sealedText`,
// escalate writes the neutral ask line, the private-lines pass neutralises
// sealed lines by their clear kind, suggestions refuse a sealed card.
// Plaintext rows keep their old behaviour. Real Postgres, own teams, deleted
// after (cascade).

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
      WHERE table_name = 'messages' AND column_name = 'attachment_ids')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
  ) AS ok
`)
if (!ready.ok) console.info(`[sealed-readers.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { deriveSessionTitle } = await import(`@/lib/sessions/title`)
const { composeSessionBrief } = await import(`@/lib/sessions/brief`)
const { escalateCard, escalateTarget } = await import(`@/lib/sessions/escalate`)
const { hidePrivateLinesInTx } = await import(`@/lib/sessions/private-lines`)
const { dismissSuggestion, dismissReplies } = await import(`@/lib/sessions/suggestions`)
const { askHiddenText, privateText } = await import(`@/lib/sessions/bus`)
const { buildQuestionCard, postAskCard } = await import(`@/lib/sessions/plan`)
const { PRIVATE_PR_LEAD, PRIVATE_REPLY_LEAD, PRIVATE_REPORT_LEAD } = await import(`@exp/domain-contract/chats`)

const fixture = JSON.parse(
  readFileSync(new URL(`../../../../../packages/domain-contract/fixtures/sealed-content.json`, import.meta.url), `utf8`)
) as { body: { envelope: string }; card: { splits: { card: { kind: string }; envelope: Record<string, unknown> }[] } }
const ENVELOPE = fixture.body.envelope
const SEALED = (fixture.card.splits[0]!.envelope.sealed as string)!

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `sealed-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@sealed.test`}, true, false, now(), now())
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

async function world() {
  const owner = await user()
  const mate = await user()
  const [team] = await db
    .insert(teams)
    .values({ name: `Sealed`, slug: `sealed-${id8()}` })
    .returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner, role: `owner` },
    { teamId, userId: mate, role: `member` },
  ])
  await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `SE` })
  const deviceId = `dev-${id8()}`
  const mateDev = `dev-${id8()}`
  await db.insert(devices).values([
    { userId: owner, deviceId, label: `box`, kind: `server` },
    { userId: mate, deviceId: mateDev, label: `mate box`, kind: `server` },
  ])
  const root = async (text: string, userId = owner, dev = deviceId) =>
    (await sendSession({ userId, input: { teamId, deviceId: dev, text } })).sessionId
  return { owner, mate, teamId, deviceId, mateDev, root }
}

const row = async (id: string) => (await db.select().from(sessions).where(eq(sessions.id, id)).limit(1))[0]!

/** The session's first person message turned into the body envelope. */
async function sealFirst(sessionId: string) {
  const [first] = await db
    .select({ id: messages.id })
    .from(messages)
    .where(and(inSession(sessionId), eq(messages.authorKind, `user`)))
    .orderBy(asc(messages.createdAt))
    .limit(1)
  await db.update(messages).set({ body: ENVELOPE }).where(eq(messages.id, first!.id))
}

describe.skipIf(!ready.ok)(`EXP-1458: title + brief`, () => {
  it(`a sealed input never becomes the title; the next plaintext one does; plaintext unchanged`, async () => {
    const w = await world()
    const plain = await w.root(`Fix the login`)
    expect(await deriveSessionTitle({ ...(await row(plain)), title: null })).toBe(`Fix the login`)

    const sealed = await w.root(`placeholder`)
    await sealFirst(sealed)
    const alone = await deriveSessionTitle({ ...(await row(sealed)), title: null })
    expect(alone).not.toContain(`exp-sealed:`)
    await db.transaction((tx) =>
      insertMessage(tx, sealed, { authorKind: `user`, authorId: w.owner, body: `Second, in clear` })
    )
    expect(await deriveSessionTitle({ ...(await row(sealed)), title: null })).toBe(`Second, in clear`)
  })

  it(`a sealed first message: the brief has no text and the envelope rides as sealedText`, async () => {
    const w = await world()
    const plain = await w.root(`Rotate the key`)
    const open = await composeSessionBrief(plain)
    expect(open.sealedText).toBeNull()
    expect(open.brief).toContain(`Rotate the key`)

    const sealed = await w.root(`placeholder`)
    await sealFirst(sealed)
    const brief = await composeSessionBrief(sealed)
    expect(brief.sealedText).toBe(ENVELOPE)
    expect(brief.brief).not.toContain(`exp-sealed:`)
    expect(brief.brief).not.toContain(`placeholder`)
  })
})

/** An agent's conversation managing a thread, and a child of the thread. */
async function managed() {
  const w = await world()
  const [agent] = await db
    .insert(agents)
    .values({ teamId: w.teamId, name: `Watcher`, deviceId: w.deviceId, ownerId: w.owner })
    .returning({ id: agents.id })
  const conversation = (await sendSession({ userId: w.owner, input: { agentId: agent!.id, text: `watch` } })).sessionId
  const thread = (
    await sendSession({
      userId: w.owner,
      callerSessionId: conversation,
      viaMcp: true,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `work` },
    })
  ).sessionId
  const child = (await sendSession({ userId: w.owner, input: { parentId: thread, text: `child` } })).sessionId
  const sealedQuestion = () =>
    db.transaction((tx) =>
      insertMessage(tx, child, {
        authorKind: `agent`,
        authorSessionId: child,
        body: ``,
        card: { kind: `question`, sealed: SEALED } as never,
      })
    )
  const keyConversation = () =>
    db.update(sessions).set({ sealEpoch: 1, sealPublicKey: `A`.repeat(43) }).where(eq(sessions.id, conversation))
  return { ...w, conversation, thread, child, sealedQuestion, keyConversation }
}

const mirrorOf = async (id: string) =>
  (await db.select(messageWithSession()).from(messages).where(eq(messages.id, id)))[0]!

describe.skipIf(!ready.ok)(`EXP-1458: escalate`, () => {
  it(`escalateTarget names the managing conversation's root, sealed or plaintext; refuses what escalate refuses`, async () => {
    const m = await managed()
    const sealed = await m.sealedQuestion()
    expect(await escalateTarget(m.owner, sealed)).toEqual({ rootId: m.conversation })
    const plain = await postAskCard({
      userId: m.owner,
      sessionId: m.child,
      card: buildQuestionCard({ question: `Port?`, options: [{ label: `1` }] }),
    })
    expect(await escalateTarget(m.owner, plain.messageId)).toEqual({ rootId: m.conversation })
    // Its ask mirror in the thread names the same target.
    const [threadMirror] = await db
      .select({ id: messages.id })
      .from(messages)
      .where(and(inSession(m.thread), sql`${messages.card}->>'messageId' = ${plain.messageId}`))
    expect(await escalateTarget(m.owner, threadMirror!.id)).toEqual({ rootId: m.conversation })
    // A plain message, a stranger, a thread nobody manages.
    const [first] = await db.select({ id: messages.id }).from(messages).where(inSession(m.child)).orderBy(asc(messages.createdAt)).limit(1)
    await expect(escalateTarget(m.owner, first!.id)).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(escalateTarget(m.mate, sealed)).rejects.toBeTruthy()
    const lone = await m.root(`alone`)
    const loneCard = await postAskCard({
      userId: m.owner,
      sessionId: lone,
      card: buildQuestionCard({ question: `Q?`, options: [{ label: `a` }] }),
    })
    await expect(escalateTarget(m.owner, loneCard.messageId)).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`a sealed question without sealedCard: the neutral ask line, never its text`, async () => {
    const m = await managed()
    const messageId = await m.sealedQuestion()
    const escalated = await escalateCard({ userId: m.owner, caller: await row(m.child), messageId })
    expect(escalated).toMatchObject({ sessionId: m.conversation, escalated: true })
    const mirror = await mirrorOf(escalated.messageId)
    expect(mirror.body).toBe(askHiddenText({ id: m.child }, { kind: `question` } as never))
    expect(mirror.card).toEqual({ kind: `ask`, sessionId: m.child, messageId })
  })

  it(`a sealed question with the device's copy for the target tree: the mirror carries it`, async () => {
    const m = await managed()
    const messageId = await m.sealedQuestion()
    const copy = { kind: `question`, sealed: SEALED }
    const caller = await row(m.child)
    // The target has no key yet: refused.
    await expect(escalateCard({ userId: m.owner, caller, messageId, sealedCard: copy })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
    })
    await m.keyConversation()
    // Wrong kind, extra plaintext beside `sealed`, malformed.
    for (const wrong of [
      { kind: `plan`, sealed: SEALED },
      { kind: `question`, sealed: SEALED, question: `leak` },
      { kind: `question`, sealed: `v1:1:x` },
    ]) {
      await expect(escalateCard({ userId: m.owner, caller, messageId, sealedCard: wrong })).rejects.toMatchObject({
        code: `BAD_REQUEST`,
      })
    }
    const escalated = await escalateCard({ userId: m.owner, caller, messageId, sealedCard: copy })
    const mirror = await mirrorOf(escalated.messageId)
    expect(mirror.body).toBe(askHiddenText({ id: m.child }, { kind: `question` } as never))
    expect(mirror.card).toEqual({ kind: `ask`, sessionId: m.child, messageId, sealedCard: copy })
    // Idempotent: the existing mirror.
    expect(await escalateCard({ userId: m.owner, caller, messageId, sealedCard: copy })).toMatchObject({
      messageId: escalated.messageId,
      escalated: false,
    })
  })

  it(`a plaintext question: askText as before; a sealedCard beside it is refused`, async () => {
    const m = await managed()
    await m.keyConversation()
    const plain = await postAskCard({
      userId: m.owner,
      sessionId: m.child,
      card: buildQuestionCard({ question: `Which port?`, options: [{ label: `54398` }] }),
    })
    const caller = await row(m.child)
    await expect(
      escalateCard({ userId: m.owner, caller, messageId: plain.messageId, sealedCard: { kind: `question`, sealed: SEALED } })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    const escalated = await escalateCard({ userId: m.owner, caller, messageId: plain.messageId })
    const mirror = await mirrorOf(escalated.messageId)
    expect(mirror.body).toContain(`Which port?`)
    expect(mirror.card).toEqual({ kind: `ask`, sessionId: m.child, messageId: plain.messageId })
  })
})

describe.skipIf(!ready.ok)(`EXP-1458: private lines`, () => {
  it(`sealed lines go neutral by their clear kind without being read; plaintext unchanged`, async () => {
    const w = await world()
    const origin = await w.root(`private work`)
    const listener = await w.root(`watch`, w.mate, w.mateDev)
    await inRollback(async (tx) => {
      const put = (body: string, card: Record<string, unknown> | null) =>
        insertMessage(tx, listener, { authorKind: `agent`, authorSessionId: origin, body, card: card as never })
      const reply = await put(ENVELOPE, null)
      const report = await put(ENVELOPE, { kind: `report`, sessionId: origin, final: true, sealed: SEALED })
      const pr = await put(ENVELOPE, { kind: `event`, event: `pr_opened`, pullRequestId: crypto.randomUUID(), sealed: SEALED })
      const other = await put(`clear words`, { kind: `subagent`, sealed: SEALED })
      const plain = await put(`a plain non-relay line`, null)
      await hidePrivateLinesInTx(tx, [listener])
      const after = new Map(
        (await tx.select().from(messages).where(inSession(listener))).map((m) => [m.id, m] as const)
      )
      expect(after.get(reply)).toMatchObject({ body: privateText({ id: origin }, PRIVATE_REPLY_LEAD), card: null })
      expect(after.get(report)).toMatchObject({
        body: privateText({ id: origin }, PRIVATE_REPORT_LEAD),
        card: { kind: `report`, sessionId: origin, final: true, summary: PRIVATE_REPORT_LEAD },
      })
      expect(after.get(report)!.card).not.toHaveProperty(`sealed`)
      expect(after.get(pr)).toMatchObject({
        body: privateText({ id: origin }, PRIVATE_PR_LEAD),
        card: { kind: `event`, event: `pr_opened` },
      })
      expect(after.get(pr)!.card).not.toHaveProperty(`pullRequestId`)
      expect(after.get(pr)!.card).not.toHaveProperty(`sealed`)
      expect(after.get(other)).toMatchObject({ body: privateText({ id: origin }, PRIVATE_REPLY_LEAD), card: { kind: `subagent` } })
      expect(after.get(other)!.card).not.toHaveProperty(`sealed`)
      // Not a relay, not sealed: the old rules leave it.
      expect(after.get(plain)!.body).toBe(`a plain non-relay line`)
      expect(JSON.stringify([...after.values()])).not.toContain(`exp-sealed:`)
      // Idempotent.
      expect(await hidePrivateLinesInTx(tx, [listener])).toBe(0)
    })
  })
})

describe.skipIf(!ready.ok)(`EXP-1458: suggestions`, () => {
  it(`a sealed suggestions / replies card is refused, never read`, async () => {
    const w = await world()
    const session = await w.root(`work`)
    const put = (kind: string) =>
      db.transaction((tx) =>
        insertMessage(tx, session, { authorKind: `agent`, body: ``, card: { kind, sealed: SEALED } as never })
      )
    const suggestions = await put(`suggestions`)
    await expect(dismissSuggestion(w.owner, { messageId: suggestions, itemId: `a` })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: expect.stringContaining(`sealed`),
    })
    const replies = await put(`replies`)
    await expect(dismissReplies(w.owner, replies)).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: expect.stringContaining(`sealed`),
    })
  })
})
