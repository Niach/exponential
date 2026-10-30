// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, asc, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { attachments, boards, devices, installs, messages, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { inSession } from "@/lib/sessions/messages"
import { generateTreeKey, openCard, sealBody, sealCard, wrapTreeKey } from "@exp/domain-contract/sealed-content"

// EXP-1458: sealed show/ask/plan cards, their pictures (clear attachment
// ids), their ask mirrors (the device's sealed texts), answering them with
// `sealedCard` (tRPC + the bus) and sealed suggestions, against the REAL
// migrated Postgres. Plaintext keeps its old shape.

process.env.BETTER_AUTH_SECRET ||= `sealed-plan-test-secret-0123456789abcdef`

vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
  notifyAssistantDecision: async () => {},
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
    AND EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'session_keys')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
  ) AS ok
`)
if (!ready.ok) console.info(`[plan-sealed.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { postShowCard, postSealedSuggestionCard, buildQuestionCard, buildSealedShowCard, publishCardPicture } = await import(
  `@/lib/sessions/plan`
)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `sealed-plan-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@sealed.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

const caller = (userId: string) =>
  sessionsRouter.createCaller({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) } as never)

async function world() {
  const owner = await user()
  const [team] = await db.insert(teams).values({ name: `Sealed`, slug: `sealed-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `SP` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const installKey = generateTreeKey()
  const [install] = await db
    .insert(installs)
    .values({ userId: owner, kind: `desktop`, endpointId: `ep-${crypto.randomUUID()}`, x25519Public: installKey.publicKey })
    .returning({ id: installs.id })
  const tree = generateTreeKey()
  const seal = {
    installId: install!.id,
    publicKey: tree.publicKey,
    epoch: 1 as const,
    wraps: [{ installId: install!.id, wrapped: wrapTreeKey(installKey.publicKey, tree.secret) }],
  }
  const rootId = (
    await sendSession({ userId: owner, input: { teamId, deviceId, text: sealBody(tree.publicKey, 1, `root`), seal } })
  ).sessionId
  const childId = (await sendSession({ userId: owner, input: { parentId: rootId, text: sealBody(tree.publicKey, 1, `child`) } }))
    .sessionId
  const plainRoot = (await sendSession({ userId: owner, input: { teamId, deviceId, text: `plain root` } })).sessionId
  const plainChild = (await sendSession({ userId: owner, input: { parentId: plainRoot, text: `plain child` } })).sessionId
  const upload = async (uploaderId = owner) =>
    (
      await db
        .insert(attachments)
        .values({ teamId, uploaderId, filename: `a.png`, contentType: `image/png`, sizeBytes: 3, url: `/x`, storageKey: `k-${id8()}` } as never)
        .returning({ id: attachments.id })
    )[0]!.id
  return { owner, teamId, tree, rootId, childId, plainRoot, plainChild, upload }
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

const message = async (id: string) => (await db.select().from(messages).where(eq(messages.id, id)).limit(1))[0]!
const mirrorsIn = (sessionId: string, from: string) =>
  db
    .select()
    .from(messages)
    .where(and(inSession(sessionId), eq(messages.authorSessionId, from), sql`${messages.card}->>'kind' = 'ask'`))
    .orderBy(asc(messages.createdAt))

describe.skipIf(!ready.ok)(`sealed show cards`, () => {
  it(`a sealed question lands whole: clear pictures claimed, the parent's mirror = the device's ask text, no uploads`, async () => {
    const w = await world()
    const file = await w.upload()
    const full = { kind: `question`, question: `Which?`, options: [`A`], pictures: [{ label: `web a`, attachmentId: file, caption: `c` }] }
    const card = buildSealedShowCard(sealCard(w.tree.publicKey, 1, full), `question`)
    expect(card).toMatchObject({ kind: `question`, pictures: [{ label: `web a`, attachmentId: file }] })
    expect(JSON.stringify(card)).not.toContain(`Which?`)
    const askText = sealBody(w.tree.publicKey, 1, `Asks: Which?`)
    const posted = await postShowCard({ userId: w.owner, sessionId: w.childId, card, replace: false, sealed: { askText } })
    expect(posted.uploads).toEqual([])
    const row = await message(posted.messageId)
    expect(row.card).toEqual(card)
    expect(row.attachmentIds).toEqual([file])
    expect((await db.select().from(attachments).where(eq(attachments.id, file)))[0]!.messageId).toBe(posted.messageId)
    expect((await mirrorsIn(w.rootId, w.childId)).map((m) => m.body)).toEqual([askText])

    // Replace: the new envelope + the device's new ask text on the mirror.
    const next = buildSealedShowCard(sealCard(w.tree.publicKey, 1, { ...full, question: `Which now?` }))
    const askText2 = sealBody(w.tree.publicKey, 1, `Asks: Which now?`)
    const replaced = await postShowCard({ userId: w.owner, sessionId: w.childId, card: next, replace: true, sealed: { askText: askText2 } })
    expect(replaced).toMatchObject({ messageId: posted.messageId, replaced: true, uploads: [] })
    expect((await message(posted.messageId)).card).toEqual(next)
    expect((await mirrorsIn(w.rootId, w.childId)).map((m) => m.body)).toEqual([askText2])
  })

  it(`refuses a picture someone else uploaded, a content field in clear, and a keyless tree`, async () => {
    const w = await world()
    const stranger = await user()
    const foreign = await w.upload(stranger)
    const card = buildSealedShowCard(
      sealCard(w.tree.publicKey, 1, { kind: `surface`, pictures: [{ label: `x`, attachmentId: foreign }] })
    )
    await expect(postShowCard({ userId: w.owner, sessionId: w.childId, card, replace: false, sealed: {} })).rejects.toThrow(
      /uploaded to this team/
    )
    expect(() => buildSealedShowCard({ ...sealCard(w.tree.publicKey, 1, { kind: `surface` }), title: `leak` })).toThrow(/clear fields/)
    const keyless = buildSealedShowCard(sealCard(generateTreeKey().publicKey, 1, { kind: `surface` }))
    await expect(postShowCard({ userId: w.owner, sessionId: w.plainChild, card: keyless, replace: false })).rejects.toThrow(/no key/)
  })

  it(`a sealed plan with pictures still to upload: uploads minted by label, the late upload filed on the clear entry only`, async () => {
    const w = await world()
    const full = {
      kind: `plan`,
      plan: `## Plan\n![After](upload:web-after)`,
      pictures: [{ label: `web-after`, caption: `After` }],
    }
    const card = buildSealedShowCard(sealCard(w.tree.publicKey, 1, full), `plan`)
    expect(card).toMatchObject({ kind: `plan`, pictures: [{ label: `web-after` }] })
    const posted = await postShowCard({ userId: w.owner, sessionId: w.childId, card, replace: false, sealed: {} })
    expect(posted.uploads.map((upload) => upload.label)).toEqual([`web-after`])
    expect(posted.uploads[0]!.curl).toContain(`/api/guide-uploads/`)
    expect((await message(posted.messageId)).attachmentIds).toBeNull()

    const attachmentId = crypto.randomUUID()
    const prepared = {
      attachmentId,
      filename: `web-after.png`,
      contentType: `image/png`,
      sizeBytes: 3,
      storageKey: `k-${id8()}`,
      url: `/api/attachments/${attachmentId}`,
      width: 800,
      height: 600,
    }
    const input = { messageId: posted.messageId, sessionId: w.childId, teamId: w.teamId, label: `web-after`, uploaderId: w.owner }
    await publishCardPicture(input, prepared)
    const row = await message(posted.messageId)
    // The sealed part is untouched; only the clear entry names the upload.
    expect(row.card).toEqual({ ...card, pictures: [{ label: `web-after`, attachmentId }] })
    expect(row.attachmentIds).toEqual([attachmentId])
    await expect(publishCardPicture(input, { ...prepared, attachmentId: crypto.randomUUID() })).rejects.toThrow(/already uploaded/)
    // Readers fill the sealed picture (and the plan's embed) from it.
    const opened = openCard(row.card as never, new Map([[1, w.tree.secret]]))
    expect(opened.card).toMatchObject({
      plan: `## Plan\n![After](/api/attachments/${attachmentId})`,
      pictures: [{ label: `web-after`, caption: `After`, attachmentId }],
    })

    // A sealed replace naming the same label still to upload keeps the file.
    const next = buildSealedShowCard(sealCard(w.tree.publicKey, 1, { ...full, plan: `## Plan v2\n![After](upload:web-after)` }))
    const replaced = await postShowCard({ userId: w.owner, sessionId: w.childId, card: next, replace: true, sealed: {} })
    expect(replaced).toMatchObject({ messageId: posted.messageId, replaced: true, uploads: [] })
    const after = await message(posted.messageId)
    expect(after.card).toEqual({ ...next, pictures: [{ label: `web-after`, attachmentId }] })
    expect(after.attachmentIds).toEqual([attachmentId])
  })

  it(`a plaintext question keeps its uploads and its plaintext mirror`, async () => {
    const w = await world()
    const card = buildQuestionCard({ question: `Which?`, options: [{ label: `A` }], pictures: [{ label: `web a` }] })
    const posted = await postShowCard({ userId: w.owner, sessionId: w.plainChild, card, replace: false })
    expect(posted.uploads.map((upload) => upload.label)).toEqual([`web a`])
    expect((await message(posted.messageId)).attachmentIds).toBeNull()
    const [mirror] = await mirrorsIn(w.plainRoot, w.plainChild)
    expect(mirror!.body).toContain(`Which?`)
  })
})

describe.skipIf(!ready.ok)(`answering sealed cards`, () => {
  async function sealedAsk(w: Awaited<ReturnType<typeof world>>, full: Record<string, unknown>) {
    const card = buildSealedShowCard(sealCard(w.tree.publicKey, 1, full))
    return (await postShowCard({ userId: w.owner, sessionId: w.childId, card, replace: false, sealed: {} })).messageId
  }

  it(`sessions.answer stores the re-sealed card, answered + answered_at, first answer wins`, async () => {
    const w = await world()
    const messageId = await sealedAsk(w, { kind: `plan`, plan: `Do it` })
    const host = caller(w.owner)
    await expect(host.answer({ messageId, decision: `approved` })).rejects.toThrow(/sealed/)
    const sealedCard = sealCard(w.tree.publicKey, 1, { kind: `plan`, plan: `Do it`, decision: `approved` }) as {
      kind: string
      sealed: string
    }
    await host.answer({ messageId, sealedCard })
    const row = await message(messageId)
    expect(row.card).toEqual({ kind: `plan`, decision: `approved`, fresh: false, answered: true, sealed: sealedCard.sealed })
    expect(row.answeredAt).not.toBeNull()
    expect((await mirrorsIn(w.rootId, w.childId))[0]!.answeredAt).not.toBeNull()
    await expect(host.answer({ messageId, sealedCard })).rejects.toThrow(/Already answered/)
  })

  it(`the bus answer (sessions.send {answer}) takes sealedCard; plaintext answers are refused`, async () => {
    const w = await world()
    const messageId = await sealedAsk(w, { kind: `question`, question: `Which?`, options: [`A`] })
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: w.childId, answer: { messageId, answer: `A` } } })
    ).rejects.toThrow(/sealed/)
    expect((await message(messageId)).answeredAt).toBeNull()
    const sealedCard = sealCard(w.tree.publicKey, 1, { kind: `question`, question: `Which?`, answer: `A` }) as {
      kind: string
      sealed: string
    }
    await sendSession({ userId: w.owner, input: { sessionId: w.childId, answer: { messageId, sealedCard } } })
    expect((await message(messageId)).card).toEqual({ kind: `question`, answered: true, sealed: sealedCard.sealed })
  })

  it(`a plaintext card refuses sealedCard and keeps its old answer`, async () => {
    const w = await world()
    const card = buildQuestionCard({ question: `Which?`, options: [{ label: `A` }] })
    const { messageId } = await postShowCard({ userId: w.owner, sessionId: w.plainChild, card, replace: false })
    const host = caller(w.owner)
    await expect(host.answer({ messageId, sealedCard: { kind: `question`, sealed: `v1:1:x` } })).rejects.toThrow()
    await host.answer({ messageId, answers: [`A`] })
    expect((await message(messageId)).card).toMatchObject({ kind: `question`, answer: `A`, answers: [`A`] })
  })
})

describe.skipIf(!ready.ok)(`sealed suggestions`, () => {
  it(`a sealed suggestions card lands pre-consumed and the server refuses acting on it`, async () => {
    const w = await world()
    const envelope = sealCard(w.tree.publicKey, 1, { kind: `suggestions`, items: [{ id: `i1`, title: `T`, text: `do` }] })
    const { messageId } = await postSealedSuggestionCard({ userId: w.owner, callerSessionId: w.childId, card: envelope })
    const row = await message(messageId)
    expect(row.card).toEqual(envelope)
    expect(row.consumedAt).not.toBeNull()
    await expect(
      postSealedSuggestionCard({ userId: w.owner, callerSessionId: w.childId, card: { ...envelope, items: [] } })
    ).rejects.toThrow(/clear fields/)
  })
})
