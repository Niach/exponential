// @vitest-environment node
// #EXP-1957 (fixtures/sealed-content.json `serverComposed`) against the REAL
// migrated Postgres: text the SERVER composes from data it holds in clear (an
// unsealed card's reply / suggestion, an app's hand-off brief, the `Link #A`
// wake line) lands as a server line in a KEYED tree (keyed the way prod keys
// one: started in clear, then `sessionKeys.create` by its device), while the
// same plaintext from a client over tRPC or an MCP caller is still refused.
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, asc, eq, sql } from "drizzle-orm"
import fixture from "@exp/domain-contract/fixtures/sealed-content.json"
import { generateTreeKey, wrapTreeKey } from "@exp/domain-contract/sealed-content"
import { db } from "@/db/connection"
import { agents, apps, boards, devices, installs, issues, messages, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { inSession, insertMessage } from "@/lib/sessions/messages"

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
if (!ready.ok) console.info(`[server-composed.real-db] skipped: ${ready.reason}`)

const { sendSession, PLAIN_IN_SEALED_TREE_MESSAGE } = await import(`@/lib/sessions/send`)
const { sendReply, startSuggestion, SEALED_SUGGESTIONS_MESSAGE } = await import(`@/lib/sessions/suggestions`)
const { createTreeKeyInTx } = await import(`@/lib/sessions/tree-keys`)
const { manualLinkText } = await import(`@/lib/sessions/issues`)
const { handOff, resetHandoffSlots } = await import(`@/lib/apps/agent-handoff`)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)

type Case = { name: string; via: string; tree: string; card?: string; landed: boolean }
const CASES = fixture.serverComposed.cases as Case[]
const caseOf = (via: string, card?: string) => CASES.find((c) => c.via === via && (card === undefined || c.card === card))!

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `composed-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@composed.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

const caller = (userId: string) =>
  sessionsRouter.createCaller({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) } as never)

async function world() {
  const owner = await user()
  const [team] = await db.insert(teams).values({ name: `Composed`, slug: `composed-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  const [board] = await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `SC` }).returning()
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `desktop`, lastSeenAt: new Date(), sharedTeamIds: [teamId] })
  const installKey = generateTreeKey()
  const [install] = await db
    .insert(installs)
    .values({ userId: owner, kind: `desktop`, endpointId: `ep-${crypto.randomUUID()}`, x25519Public: installKey.publicKey })
    .returning({ id: installs.id })
  /** Keys an existing root the way the device does at its claim (prod's shape). */
  const key = async (rootId: string) => {
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
  }
  const keyedRoot = async () => {
    const { sessionId } = await sendSession({ userId: owner, input: { teamId, deviceId, text: `fix the flaky test` } })
    await key(sessionId)
    return sessionId
  }
  return { owner, teamId, boardId: board!.id, deviceId, key, keyedRoot }
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

const sessionRow = async (id: string) => (await db.select().from(sessions).where(eq(sessions.id, id)).limit(1))[0]!
const bodies = async (sessionId: string) =>
  (await db.select({ body: messages.body }).from(messages).where(inSession(sessionId)).orderBy(asc(messages.createdAt), asc(messages.id))).map(
    (m) => m.body ?? ``
  )
const cardOf = async (messageId: string) =>
  (await db.select({ card: messages.card }).from(messages).where(eq(messages.id, messageId)).limit(1))[0]!.card as unknown as {
    items: Array<Record<string, unknown>>
  }
const postCard = (sessionId: string, card: Record<string, unknown>) =>
  db.transaction((tx) => insertMessage(tx, sessionId, { authorKind: `agent`, authorSessionId: sessionId, body: ``, card: card as never }))

describe.skipIf(!ready.ok)(`#EXP-1957 server-composed lines in a keyed tree`, () => {
  it(`the fixture names every path`, () => {
    expect(CASES.map((c) => c.via).sort()).toEqual(
      [`appHandoff`, `linkIssues`, `mcpSend`, `sendReply`, `sendReply`, `startSuggestionChild`, `startSuggestionHere`, `trpcSend`].sort()
    )
    expect(CASES.every((c) => c.tree === `keyed`)).toBe(true)
  })

  it(caseOf(`sendReply`, `plain`).name, async () => {
    const w = await world()
    const rootId = await w.keyedRoot()
    expect((await sessionRow(rootId)).sealEpoch).toBe(1)
    const cardId = await postCard(rootId, { kind: `replies`, items: [{ id: `a`, text: `Yes, ship it` }, { id: `b`, text: `Not yet` }] })
    const sent = await sendReply(w.owner, { messageId: cardId, itemId: `a` })
    expect(sent.sessionId).toBe(rootId)
    expect((await bodies(rootId)).at(-1)).toBe(`Yes, ship it`)
    expect((await cardOf(cardId)).items).toMatchObject([{ id: `a`, sent: true }, { id: `b` }])
    expect((await cardOf(cardId)).items[1]!.sent ?? null).toBeNull()
  })

  it(caseOf(`startSuggestionHere`).name, async () => {
    const w = await world()
    const rootId = await w.keyedRoot()
    const cardId = await postCard(rootId, { kind: `suggestions`, items: [{ id: `s1`, title: `Add a test`, text: `Cover the keyed tree.` }] })
    const sent = await startSuggestion(w.owner, { messageId: cardId, itemId: `s1`, where: `here` })
    expect(sent.sessionId).toBe(rootId)
    expect((await bodies(rootId)).at(-1)).toBe(`Add a test\n\nCover the keyed tree.`)
    expect((await cardOf(cardId)).items).toMatchObject([{ id: `s1`, here: true }])
  })

  it(caseOf(`startSuggestionChild`).name, async () => {
    const w = await world()
    const rootId = await w.keyedRoot()
    const cardId = await postCard(rootId, { kind: `suggestions`, items: [{ id: `s1`, title: `Split it`, text: `Do the second half.` }] })
    const sent = await startSuggestion(w.owner, { messageId: cardId, itemId: `s1`, where: `child` })
    const child = await sessionRow(sent.sessionId)
    expect(child).toMatchObject({ parentId: rootId, rootId, title: `Split it` })
    expect(await bodies(child.id)).toContain(`Split it\n\nDo the second half.`)
    expect((await cardOf(cardId)).items).toMatchObject([{ id: `s1`, sessionId: child.id }])
  })

  it(caseOf(`sendReply`, `sealed`).name, async () => {
    const w = await world()
    const rootId = await w.keyedRoot()
    const cardId = await postCard(rootId, { kind: `replies`, sealed: `v1:1:AAAA` })
    await expect(sendReply(w.owner, { messageId: cardId, itemId: `a` })).rejects.toMatchObject({ message: SEALED_SUGGESTIONS_MESSAGE })
  })

  it(caseOf(`appHandoff`).name, async () => {
    resetHandoffSlots()
    const w = await world()
    const [agent] = await db.insert(agents).values({ teamId: w.teamId, name: `Triage Bot`, deviceId: w.deviceId, ownerId: w.owner }).returning({ id: agents.id })
    const [app] = await db
      .insert(apps)
      .values({ teamId: w.teamId, slug: `requests-${id8()}`, name: `Requests`, package: {} as never, headVersion: 1, publishedVersion: 1, publishedBy: w.owner, createdBy: w.owner })
      .returning({ id: apps.id, slug: apps.slug, name: apps.name, teamId: apps.teamId })
    const run = { agent: `triage-bot`, prompt: `Route the request.` }
    const first = await handOff({ app: app!, userId: w.owner, run, input: { title: `one` }, source: `automation route` })
    if (!(`sessionId` in first)) throw new Error(first.error)
    // The device's first claim keys the member's conversation (pickup.rs).
    await w.key(first.sessionId)
    expect((await sessionRow(first.sessionId)).sealEpoch).toBe(1)
    const second = await handOff({ app: app!, userId: w.owner, run, input: { title: `two` }, source: `automation route` })
    expect(second).toEqual({ sessionId: first.sessionId })
    const last = (await bodies(first.sessionId)).at(-1)!
    expect(last).toContain(`Route the request.`)
    expect(last).toContain(`two`)
    void agent
  })

  it(caseOf(`linkIssues`).name, async () => {
    const w = await world()
    const rootId = await w.keyedRoot()
    const [issue] = await db
      .insert(issues)
      .values({ boardId: w.boardId, teamId: w.teamId, title: `Flaky test`, creatorId: w.owner })
      .returning({ id: issues.id, identifier: issues.identifier })
    await caller(w.owner).linkIssues({ sessionId: rootId, issueIds: [issue!.id] })
    expect((await bodies(rootId)).at(-1)).toBe(manualLinkText([issue!.identifier]))
  })

  it(`${caseOf(`trpcSend`).name}, ${caseOf(`mcpSend`).name}: still refused, nothing written`, async () => {
    const w = await world()
    const rootId = await w.keyedRoot()
    const before = await bodies(rootId)
    // A client cannot smuggle the server-only flag through the wire input.
    await expect(
      caller(w.owner).send({ sessionId: rootId, text: `Yes, ship it`, extras: { serverComposed: true }, serverComposed: true } as never)
    ).rejects.toThrow(PLAIN_IN_SEALED_TREE_MESSAGE)
    await expect(sendSession({ userId: w.owner, input: { sessionId: rootId, text: `Yes, ship it` }, viaMcp: true })).rejects.toThrow(
      PLAIN_IN_SEALED_TREE_MESSAGE
    )
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: rootId, text: `Yes, ship it` }, viaMcp: true, callerSessionId: rootId })
    ).rejects.toThrow(PLAIN_IN_SEALED_TREE_MESSAGE)
    expect(await bodies(rootId)).toEqual(before)
    expect(
      await db.select({ id: messages.id }).from(messages).where(and(inSession(rootId), eq(messages.body, `Yes, ship it`)))
    ).toEqual([])
  })
})
