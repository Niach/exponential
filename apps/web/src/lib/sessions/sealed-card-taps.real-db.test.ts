// @vitest-environment node
// #EXP-1958 (fixtures/sealed-content.json `cardStamps.serverCases`) against
// the REAL migrated Postgres: a tap on a SEALED replies / suggestions card is
// the client's sealed `sessions.send` with `cardItem`; the server only stamps
// the item in the card's clear `stamps`. The tree is keyed the way prod keys
// one (started in clear, then `sessionKeys.create` by its device), the card
// is sealed with that tree's key and posted through the gateway's server path
// (`postSealedSuggestionCard`), and no item text ever reaches a row.
import { afterAll, describe, expect, it, vi } from "vitest"
import { asc, eq, sql } from "drizzle-orm"
import fixture from "@exp/domain-contract/fixtures/sealed-content.json"
import { REPLIES_MAX, SUGGESTIONS_MAX } from "@exp/db-schema/domain"
import {
  generateTreeKey,
  openBody,
  openCard,
  sealBody,
  sealCard,
  wrapTreeKey,
  type TreeKey,
} from "@exp/domain-contract/sealed-content"
import { db } from "@/db/connection"
import { boards, devices, installs, issues, messages, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { inSession } from "@/lib/sessions/messages"

vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/integrations/notifications")>()),
  notifySessionAsk: async () => {},
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  getSteerRelayConfig: () => null,
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: false }),
}))
vi.mock(`@/lib/conversion/events`, () => ({ recordConversionEvent: async () => {} }))

const ready = await probeRealDb(`
  SELECT (
    EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'session_keys')
    AND EXISTS (SELECT 1 FROM information_schema.columns
      WHERE table_name = 'sessions' AND column_name = 'clear_tree')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
  ) AS ok
`)
if (!ready.ok) console.info(`[sealed-card-taps.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { postSealedSuggestionCard } = await import(`@/lib/sessions/plan`)
const { SEALED_SUGGESTIONS_MESSAGE } = await import(`@/lib/sessions/suggestions`)
const { createTreeKeyInTx } = await import(`@/lib/sessions/tree-keys`)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)

type Case = (typeof fixture.cardStamps.serverCases)[number]
const CASES = fixture.cardStamps.serverCases as Case[]
const caseNamed = (prefix: string) => CASES.find((c) => c.name.startsWith(prefix))!

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `card-taps-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@taps.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

const caller = (userId: string) =>
  sessionsRouter.createCaller({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) } as never)

// The item texts: none of them may ever land in a row in clear.
const SECRET = {
  reply: `Yes, ship the vault migration`,
  reply2: `Hold the vault migration`,
  title: `Rotate the vault keys`,
  text: `Rotate every vault key and re-wrap the readers`,
  title2: `Audit the vault`,
  text2: `List every reader of the vault tree`,
}

async function world() {
  const owner = await user()
  const outsider = await user()
  const [team] = await db.insert(teams).values({ name: `Taps`, slug: `taps-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  const [board] = await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `ST` }).returning()
  const [issue] = await db.insert(issues).values({ boardId: board!.id, teamId, title: `Vault`, creatorId: owner }).returning({ id: issues.id })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `desktop`, lastSeenAt: new Date(), sharedTeamIds: [teamId] })
  const installKey = generateTreeKey()
  const [install] = await db
    .insert(installs)
    .values({ userId: owner, kind: `desktop`, endpointId: `ep-${crypto.randomUUID()}`, x25519Public: installKey.publicKey })
    .returning({ id: installs.id })
  // Started in clear (an older client), then keyed by its device: prod's shape.
  const { sessionId: rootId } = await sendSession({
    userId: owner,
    input: { teamId, deviceId, issueIds: [issue!.id], text: `look at the vault` },
  })
  const tree = generateTreeKey()
  await db.transaction((tx) =>
    createTreeKeyInTx(tx, owner, {
      rootId,
      installId: install!.id,
      epoch: 1,
      publicKey: tree.publicKey,
      wraps: [{ installId: install!.id, wrapped: wrapTreeKey(installKey.publicKey, tree.secret) }],
    })
  )
  return { owner, outsider, teamId, deviceId, boardId: board!.id, issueId: issue!.id, rootId, tree }
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

const keysOf = (tree: TreeKey) => new Map([[1, tree.secret]])
const storedCard = async (messageId: string) =>
  (await db.select({ card: messages.card }).from(messages).where(eq(messages.id, messageId)).limit(1))[0]!.card as unknown as Record<
    string,
    unknown
  >
const lastBody = async (sessionId: string) =>
  (await db.select({ body: messages.body }).from(messages).where(inSession(sessionId)).orderBy(asc(messages.createdAt), asc(messages.id))).at(-1)!
    .body ?? ``

/** A sealed card on the root, posted the way the gateway's `sessions_suggest {sealed}` does. */
async function postSealed(w: Awaited<ReturnType<typeof world>>, card: Record<string, unknown>) {
  const envelope = sealCard(w.tree.publicKey, 1, card)
  const { messageId } = await postSealedSuggestionCard({ userId: w.owner, callerSessionId: w.rootId, card: envelope })
  return messageId
}
const repliesCard = () => ({
  kind: `replies`,
  items: [
    { id: crypto.randomUUID(), text: SECRET.reply },
    { id: crypto.randomUUID(), text: SECRET.reply2 },
  ],
})
const suggestionsCard = () => ({
  kind: `suggestions`,
  items: [
    { id: crypto.randomUUID(), title: SECRET.title, text: SECRET.text },
    { id: crypto.randomUUID(), title: SECRET.title2, text: SECRET.text2 },
  ],
})
const ids = (card: { items: Array<{ id: string }> }) => card.items.map((item) => item.id)

/** No row of the team holds an item's text in clear (messages, sessions). */
async function assertNoPlaintext(teamId: string) {
  const rows = await db.execute(sql`
    SELECT row_to_json(m)::text AS j FROM messages m
      JOIN sessions s ON s.id = m.session_id WHERE s.team_id = ${teamId}
    UNION ALL SELECT row_to_json(s)::text FROM sessions s WHERE s.team_id = ${teamId}
  `)
  const dump = (rows as unknown as { rows?: Array<{ j: string }> }).rows ?? (rows as unknown as Array<{ j: string }>)
  const text = dump.map((row) => row.j).join(`\n`)
  expect(text.length).toBeGreaterThan(0)
  for (const secret of Object.values(SECRET)) expect(text).not.toContain(secret)
}

describe.skipIf(!ready.ok)(`#EXP-1958 taps on a sealed card in a keyed tree`, () => {
  it(`the fixture's server cases`, () => {
    expect(CASES.length).toBeGreaterThan(10)
  })

  it(`${caseNamed(`a sealed reply lands`).name}; ${caseNamed(`a second tap`).name} = CONFLICT`, async () => {
    const w = await world()
    expect((await db.select().from(sessions).where(eq(sessions.id, w.rootId)))[0]!.sealEpoch).toBe(1)
    const card = repliesCard()
    const messageId = await postSealed(w, card)
    const [first] = ids(card)
    const text = sealBody(w.tree.publicKey, 1, SECRET.reply)
    const sent = await caller(w.owner).send({ sessionId: w.rootId, text, cardItem: { messageId, itemId: first! } })
    expect(sent.sessionId).toBe(w.rootId)
    const body = await lastBody(w.rootId)
    expect(body).toBe(text)
    expect(openBody(body, keysOf(w.tree))).toMatchObject({ status: `opened`, text: SECRET.reply })
    const stored = await storedCard(messageId)
    expect(stored.stamps).toEqual({ [first!]: caseNamed(`a sealed reply lands`).stamp })
    // The reader: the stamp lands on its item, `stamps` never reaches it.
    const opened = openCard(stored, keysOf(w.tree))
    expect(opened.card.items).toEqual([
      { id: first, text: SECRET.reply, sent: true },
      { id: card.items[1]!.id, text: SECRET.reply2 },
    ])
    // A second tap on the same reply: nothing posted.
    await expect(
      caller(w.owner).send({ sessionId: w.rootId, text: sealBody(w.tree.publicKey, 1, SECRET.reply), cardItem: { messageId, itemId: first! } })
    ).rejects.toMatchObject({ code: caseNamed(`a second tap`).code })
    expect(await lastBody(w.rootId)).toBe(text)
    await assertNoPlaintext(w.teamId)
  })

  it(`${caseNamed(`Do it here`).name}; ${caseNamed(`Start lands`).name}`, async () => {
    const w = await world()
    const card = suggestionsCard()
    const messageId = await postSealed(w, card)
    const [here, start] = ids(card) as [string, string]
    const host = caller(w.owner)
    const hereText = sealBody(w.tree.publicKey, 1, `${SECRET.title}\n\n${SECRET.text}`)
    await host.send({ sessionId: w.rootId, text: hereText, cardItem: { messageId, itemId: here } })
    expect(await lastBody(w.rootId)).toBe(hereText)

    const startText = sealBody(w.tree.publicKey, 1, `${SECRET.title2}\n\n${SECRET.text2}`)
    const child = await host.send({
      parentId: w.rootId,
      text: startText,
      sealedName: sealBody(w.tree.publicKey, 1, SECRET.title2),
      issueIds: [w.issueId],
      cardItem: { messageId, itemId: start },
    })
    const [row] = await db.select().from(sessions).where(eq(sessions.id, child.sessionId))
    expect(row).toMatchObject({ parentId: w.rootId, rootId: w.rootId })
    expect(openBody(row!.sealedTitle!, keysOf(w.tree))).toMatchObject({ text: SECRET.title2 })
    expect(await lastBody(child.sessionId)).toBe(startText)
    const stored = await storedCard(messageId)
    expect(stored.stamps).toEqual({ [here]: { here: true }, [start]: { sessionId: child.sessionId } })
    const opened = openCard(stored, keysOf(w.tree)).card.items as Array<Record<string, unknown>>
    expect(opened.map((item) => [item.here ?? null, item.sessionId ?? null])).toEqual([
      [true, null],
      [null, child.sessionId],
    ])
    // Start on a handled item rolls the child back.
    const before = (await db.select().from(sessions).where(eq(sessions.parentId, w.rootId))).length
    await expect(
      host.send({ parentId: w.rootId, text: startText, cardItem: { messageId, itemId: start } })
    ).rejects.toMatchObject({ code: `CONFLICT` })
    expect((await db.select().from(sessions).where(eq(sessions.parentId, w.rootId))).length).toBe(before)
    await assertNoPlaintext(w.teamId)
  })

  it(`the refusals: ${[`a plaintext text`, `cardItem naming an unsealed`, `cardItem into another`, `Start from a replies`, `someone outside`].map((p) => caseNamed(p).code).join(`, `)}`, async () => {
    const w = await world()
    const card = repliesCard()
    const messageId = await postSealed(w, card)
    const [first] = ids(card) as [string]
    const host = caller(w.owner)
    const sealed = sealBody(w.tree.publicKey, 1, SECRET.reply)
    const cardItem = { messageId, itemId: first }
    await expect(host.send({ sessionId: w.rootId, text: SECRET.reply, cardItem })).rejects.toMatchObject({
      code: caseNamed(`a plaintext text`).code,
    })
    // An unsealed replies card (an older row) keeps the server path.
    const { sessionId: plainRoot } = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `plain` } })
    const plainCard = await db.transaction(async (tx) => {
      const { insertMessage } = await import(`@/lib/sessions/messages`)
      return insertMessage(tx, plainRoot, { authorKind: `agent`, authorSessionId: plainRoot, body: ``, card: { kind: `replies`, items: [{ id: `a`, text: `x` }] } as never })
    })
    await expect(
      host.send({ sessionId: plainRoot, text: sealed, cardItem: { messageId: plainCard, itemId: `a` } })
    ).rejects.toMatchObject({ code: caseNamed(`cardItem naming an unsealed`).code })
    await expect(host.send({ sessionId: plainRoot, text: sealed, cardItem })).rejects.toMatchObject({
      code: caseNamed(`cardItem into another`).code,
    })
    await expect(host.send({ parentId: w.rootId, text: sealed, cardItem })).rejects.toMatchObject({
      code: caseNamed(`Start from a replies`).code,
    })
    await expect(caller(w.outsider).send({ sessionId: w.rootId, text: sealed, cardItem })).rejects.toMatchObject({
      code: caseNamed(`someone outside`).code,
    })
    // The server's own actions stay refused on a sealed card.
    await expect(host.sendReply(cardItem)).rejects.toThrow(SEALED_SUGGESTIONS_MESSAGE)
    const tasks = await postSealed(w, suggestionsCard())
    await expect(host.startSuggestion({ messageId: tasks, itemId: `x`, where: `child` })).rejects.toThrow(SEALED_SUGGESTIONS_MESSAGE)
    expect((await storedCard(messageId)).stamps).toBeUndefined()
    await assertNoPlaintext(w.teamId)
  })

  it(`${caseNamed(`an item id that is no uuid`).name}, ${caseNamed(`__proto__ as`).name}: nothing posted, nothing stamped`, async () => {
    const w = await world()
    const host = caller(w.owner)
    const replies = repliesCard()
    const repliesId = await postSealed(w, replies)
    const tasksId = await postSealed(w, suggestionsCard())
    const before = await lastBody(w.rootId)
    const sealed = () => sealBody(w.tree.publicKey, 1, SECRET.reply)
    for (const c of [caseNamed(`an item id that is no uuid`), caseNamed(`__proto__ as`)] as Array<Case & { itemId: string }>) {
      // Twice: an unstamped success would let the same tap repeat forever.
      for (let i = 0; i < 2; i++) {
        await expect(
          host.send({ sessionId: w.rootId, text: sealed(), cardItem: { messageId: repliesId, itemId: c.itemId } })
        ).rejects.toMatchObject({ code: c.code })
      }
    }
    const stamp = caseNamed(`a stamp action with an item id`) as Case & { itemId: string }
    await expect(host.dismissSuggestion({ messageId: tasksId, itemId: stamp.itemId })).rejects.toMatchObject({ code: stamp.code })
    await expect(host.dismissReplies({ messageId: repliesId, itemIds: [ids(replies)[0]!, stamp.itemId] })).rejects.toMatchObject({
      code: stamp.code,
    })
    await expect(
      host.fileSuggestion({ messageId: tasksId, itemId: stamp.itemId, title: `Proto task`, text: `x` })
    ).rejects.toMatchObject({ code: stamp.code })
    expect((await db.select().from(issues).where(eq(issues.title, `Proto task`))).length).toBe(0)
    expect(await lastBody(w.rootId)).toBe(before)
    expect((await storedCard(repliesId)).stamps).toBeUndefined()
    expect((await storedCard(tasksId)).stamps).toBeUndefined()
  })

  it(`${caseNamed(`a new id once stamps holds`).name}`, async () => {
    const w = await world()
    const host = caller(w.owner)
    const code = caseNamed(`a new id once stamps holds`).code
    const fresh = () => crypto.randomUUID()
    const sealed = () => sealBody(w.tree.publicKey, 1, SECRET.reply)
    const count = async (messageId: string) => Object.keys(((await storedCard(messageId)).stamps ?? {}) as object).length

    // Suggestions: a steerer inventing ids fills the map to SUGGESTIONS_MAX, no further.
    const tasksId = await postSealed(w, suggestionsCard())
    for (let i = 0; i < SUGGESTIONS_MAX; i++) await host.dismissSuggestion({ messageId: tasksId, itemId: fresh() })
    expect(await count(tasksId)).toBe(SUGGESTIONS_MAX)
    await expect(host.dismissSuggestion({ messageId: tasksId, itemId: fresh() })).rejects.toMatchObject({ code })
    const children = (await db.select().from(sessions).where(eq(sessions.parentId, w.rootId))).length
    await expect(
      host.send({ parentId: w.rootId, text: sealed(), cardItem: { messageId: tasksId, itemId: fresh() } })
    ).rejects.toMatchObject({ code })
    expect((await db.select().from(sessions).where(eq(sessions.parentId, w.rootId))).length).toBe(children)
    await expect(host.fileSuggestion({ messageId: tasksId, itemId: fresh(), title: `Overflow task`, text: `x` })).rejects.toMatchObject({ code })
    expect((await db.select().from(issues).where(eq(issues.title, `Overflow task`))).length).toBe(0)
    expect(await count(tasksId)).toBe(SUGGESTIONS_MAX)

    // Replies: REPLIES_MAX, by the send and by dismissReplies (all or nothing).
    const replies = repliesCard()
    const repliesId = await postSealed(w, replies)
    const [first] = ids(replies) as [string]
    await host.send({ sessionId: w.rootId, text: sealed(), cardItem: { messageId: repliesId, itemId: first } })
    await expect(
      host.dismissReplies({ messageId: repliesId, itemIds: Array.from({ length: REPLIES_MAX }, fresh) })
    ).rejects.toMatchObject({ code })
    expect(await count(repliesId)).toBe(1)
    await host.dismissReplies({ messageId: repliesId, itemIds: [first, ...Array.from({ length: REPLIES_MAX - 1 }, fresh)] })
    expect(await count(repliesId)).toBe(REPLIES_MAX)
    const before = await lastBody(w.rootId)
    await expect(
      host.send({ sessionId: w.rootId, text: sealed(), cardItem: { messageId: repliesId, itemId: fresh() } })
    ).rejects.toMatchObject({ code })
    expect(await lastBody(w.rootId)).toBe(before)
    // A known id is still stamped again by dismissReplies.
    await host.dismissReplies({ messageId: repliesId, itemIds: [first] })
    expect(await count(repliesId)).toBe(REPLIES_MAX)
    await assertNoPlaintext(w.teamId)
  })

  it(`${caseNamed(`dismissing a sealed suggestion`).name}, ${caseNamed(`dismissing sealed replies by`).name}, ${caseNamed(`filing a sealed suggestion with`).name}`, async () => {
    const w = await world()
    const host = caller(w.owner)
    const replies = repliesCard()
    const repliesId = await postSealed(w, replies)
    await expect(host.dismissReplies({ messageId: repliesId })).rejects.toMatchObject({
      code: caseNamed(`dismissing sealed replies without`).code,
    })
    await host.dismissReplies({ messageId: repliesId, itemIds: ids(replies) })
    expect((await storedCard(repliesId)).stamps).toEqual(Object.fromEntries(ids(replies).map((id) => [id, { sent: true }])))

    const tasks = suggestionsCard()
    const tasksId = await postSealed(w, tasks)
    const [dismiss, file] = ids(tasks) as [string, string]
    await host.dismissSuggestion({ messageId: tasksId, itemId: dismiss })
    await expect(host.dismissSuggestion({ messageId: tasksId, itemId: dismiss })).rejects.toMatchObject({ code: `CONFLICT` })
    await expect(host.fileSuggestion({ messageId: tasksId, itemId: file })).rejects.toMatchObject({
      code: caseNamed(`filing a sealed suggestion without`).code,
    })
    // An issue is clear by design: the person files the item on the board.
    const filed = await host.fileSuggestion({ messageId: tasksId, itemId: file, title: `Filed task`, text: `Its description` })
    const [issue] = await db.select().from(issues).where(eq(issues.id, filed.issueId))
    expect(issue).toMatchObject({ boardId: w.boardId, title: `Filed task` })
    expect((await storedCard(tasksId)).stamps).toEqual({ [dismiss]: { dismissed: true }, [file]: { issueId: filed.issueId } })
    await expect(
      host.fileSuggestion({ messageId: tasksId, itemId: file, title: `Again`, text: `Again` })
    ).rejects.toMatchObject({ code: `CONFLICT` })
    expect((await db.select().from(issues).where(eq(issues.title, `Again`))).length).toBe(0)
    await assertNoPlaintext(w.teamId)
  })
})
