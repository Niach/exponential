// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, asc, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { attachments, boards, devices, installs, messages, sessionKeys, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { inSession } from "@/lib/sessions/messages"
import {
  generateTreeKey,
  sealBody,
  sealCard,
  wrapTreeKey,
} from "@exp/domain-contract/sealed-content"

// EXP-1458: the server's sealed paths against the REAL migrated Postgres:
// `sessions.send` (inline `seal`, sealed text, clear attachments/mentions/
// refs, sealed name, writer-composed prompts), the device's `reply` (byte
// copies within a tree, per-listener copies across trees, the listener
// list, mentions), sealed asks and reports. Plaintext keeps its old shape.

vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
const notify = vi.hoisted(() => vi.fn((..._args: unknown[]) => {}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  notifySessionAsk: async () => {},
  fireAndForgetSessionMessageNotify: notify,
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
      WHERE table_name = 'messages' AND column_name = 'mention_user_ids')
    AND EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'session_keys')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
  ) AS ok
`)
if (!ready.ok) console.info(`[sealed-send-bus.real-db] skipped: ${ready.reason}`)

const { sendSession, SEALED_NO_KEY_MESSAGE } = await import(`@/lib/sessions/send`)
const { endSession } = await import(`@/lib/sessions/lifecycle`)
const { busAsk, REPORT_FALLBACK } = await import(`@/lib/sessions/bus`)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)

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

const caller = (userId: string) =>
  sessionsRouter.createCaller({
    session: { user: { id: userId } },
    db,
    request: new Request(`http://x.test`),
  } as never)

async function world() {
  const owner = await user()
  const [team] = await db.insert(teams).values({ name: `Sealed`, slug: `sealed-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `SE` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const installKey = generateTreeKey()
  const [install] = await db
    .insert(installs)
    .values({ userId: owner, kind: `desktop`, endpointId: `ep-${crypto.randomUUID()}`, x25519Public: installKey.publicKey })
    .returning({ id: installs.id })
  /** A fresh tree key and the inline `seal` for it. */
  const seal = () => {
    const tree = generateTreeKey()
    return {
      tree,
      seal: {
        installId: install!.id,
        publicKey: tree.publicKey,
        epoch: 1 as const,
        wraps: [{ installId: install!.id, wrapped: wrapTreeKey(installKey.publicKey, tree.secret) }],
      },
    }
  }
  const root = async (text = `root`) => (await sendSession({ userId: owner, input: { teamId, deviceId, text } })).sessionId
  return { owner, teamId, deviceId, installId: install!.id, seal, root, host: caller(owner) }
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

const row = async (id: string) => (await db.select().from(sessions).where(eq(sessions.id, id)).limit(1))[0]!
const messagesOf = (sessionId: string) =>
  db.select().from(messages).where(inSession(sessionId)).orderBy(asc(messages.createdAt), asc(messages.id))
/** The bus rows `from` posted into `sessionId` (its claim line aside). */
const fromChild = async (sessionId: string, from: string) =>
  (
    await db
      .select()
      .from(messages)
      .where(and(inSession(sessionId), eq(messages.authorSessionId, from)))
      .orderBy(asc(messages.createdAt), asc(messages.id))
  ).filter((m) => (m.card as { event?: string } | null)?.event !== `session_claimed`)

describe.skipIf(!ready.ok)(`sessions.send with sealed content`, () => {
  it(`a new thread takes its key and its sealed first message in ONE send`, async () => {
    const w = await world()
    const { tree, seal } = w.seal()
    const text = sealBody(tree.publicKey, 1, `secret plan`)
    const sealedName = sealBody(tree.publicKey, 1, `Secret title`)
    const sent = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text, sealedName, seal } })
    expect(await row(sent.sessionId)).toMatchObject({ sealPublicKey: tree.publicKey, sealEpoch: 1, sealedTitle: sealedName, title: null })
    const keys = await db.select().from(sessionKeys).where(eq(sessionKeys.rootId, sent.sessionId))
    expect(keys).toMatchObject([{ epoch: 1, installId: w.installId, createdByInstallId: w.installId }])
    expect((await messagesOf(sent.sessionId)).map((m) => m.body)).toEqual([text])
  })

  it(`refuses sealed text to a keyless tree and a seal without the caller's own wrap (nothing written)`, async () => {
    const w = await world()
    const { tree, seal } = w.seal()
    const text = sealBody(tree.publicKey, 1, `x`)
    await expect(sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text } })).rejects.toThrow(
      SEALED_NO_KEY_MESSAGE
    )
    const other = await db
      .insert(installs)
      .values({ userId: w.owner, kind: `ios`, endpointId: `ep-${crypto.randomUUID()}`, x25519Public: generateTreeKey().publicKey })
      .returning({ id: installs.id })
    await expect(
      sendSession({
        userId: w.owner,
        input: { teamId: w.teamId, deviceId: w.deviceId, text, seal: { ...seal, installId: other[0]!.id } },
      })
    ).rejects.toThrow(/own install/)
    // A malformed envelope never lands.
    await expect(
      sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `exp-sealed:v1:1:@@`, seal } })
    ).rejects.toThrow(/Invalid sealed text/)
  })

  it(`a sealed message's attachments, mentions and refs ride in clear; the body is never appended to`, async () => {
    const w = await world()
    const { tree, seal } = w.seal()
    const rootId = (
      await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: sealBody(tree.publicKey, 1, `hi`), seal } })
    ).sessionId
    const upload = async () =>
      (
        await db
          .insert(attachments)
          .values({ teamId: w.teamId, uploaderId: w.owner, filename: `a.png`, contentType: `image/png`, sizeBytes: 3, storageKey: `k-${id8()}`, url: `x` })
          .returning({ id: attachments.id })
      )[0]!.id
    const sealedFile = await upload()
    const text = sealBody(tree.publicKey, 1, `look ![a](/api/attachments/${sealedFile})`)
    const sent = await sendSession({
      userId: w.owner,
      input: { sessionId: rootId, text, attachmentIds: [sealedFile], mentionUserIds: [w.owner] },
    })
    const [m] = await db.select().from(messages).where(eq(messages.id, sent.messageId!))
    expect(m).toMatchObject({ body: text, attachmentIds: [sealedFile], mentionUserIds: [w.owner] })

    // Plaintext: the embed line is still appended, and the ids ride too.
    const plainRoot = await w.root()
    const plainFile = await upload()
    const plain = await sendSession({ userId: w.owner, input: { sessionId: plainRoot, text: `see`, attachmentIds: [plainFile] } })
    const [p] = await db.select().from(messages).where(eq(messages.id, plain.messageId!))
    expect(p!.body).toBe(`see\n\n![a.png](/api/attachments/${plainFile})`)
    expect(p!.attachmentIds).toEqual([plainFile])
  })

  it(`sealed text never parses /model and is never composed with a prompt or a brief`, async () => {
    const w = await world()
    const { tree, seal } = w.seal()
    const text = sealBody(tree.publicKey, 1, `/model opus`)
    const rootId = (await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text, seal } })).sessionId
    expect((await messagesOf(rootId)).map((m) => m.body)).toEqual([text])
    // The clear `model` param is the pick.
    await sendSession({ userId: w.owner, input: { sessionId: rootId, model: `opus` } })
    expect((await row(rootId)).model).toBe(`opus`)
  })
})

describe.skipIf(!ready.ok)(`the device's side of sealed trees`, () => {
  it(`a send names the executing device's install (deliverTo); installs list their machine id`, async () => {
    const w = await world()
    const rootId = await w.root()
    // No install bound to the machine yet.
    expect((await sendSession({ userId: w.owner, input: { sessionId: rootId, text: `hi` } })).deliverTo).toBeNull()
    const [machine] = await db.select({ id: devices.id }).from(devices).where(eq(devices.deviceId, w.deviceId))
    await db.update(installs).set({ deviceId: machine!.id, lastSeenAt: new Date() }).where(eq(installs.id, w.installId))
    const [install] = await db.select().from(installs).where(eq(installs.id, w.installId))
    const sent = await sendSession({ userId: w.owner, input: { sessionId: rootId, text: `again` } })
    expect(sent.deliverTo).toEqual({ installId: w.installId, endpointId: install!.endpointId })
    const { installsRouter } = await import(`@/lib/trpc/installs`)
    const installsApi = installsRouter.createCaller({ session: { user: { id: w.owner } }, db } as never)
    expect((await installsApi.mine()).find((row) => row.installId === w.installId)?.deviceId).toBe(w.deviceId)
    expect((await installsApi.list({ teamId: w.teamId })).find((row) => row.installId === w.installId)?.deviceId).toBe(w.deviceId)
  })

  it(`report fills a blank sealed title (never over a set title), and refuses a plaintext one there`, async () => {
    const w = await world()
    const { tree, seal } = w.seal()
    const rootId = (
      await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: sealBody(tree.publicKey, 1, `hi`), seal } })
    ).sessionId
    await w.host.claim({ sessionId: rootId, deviceId: w.deviceId })
    const sealedTitle = sealBody(tree.publicKey, 1, `Auto name`)
    await w.host.report({ sessionId: rootId, sealedTitle })
    expect(await row(rootId)).toMatchObject({ sealedTitle, title: null })
    await w.host.report({ sessionId: rootId, sealedTitle: sealBody(tree.publicKey, 1, `Later`) })
    expect((await row(rootId)).sealedTitle).toBe(sealedTitle)
    await expect(w.host.report({ sessionId: rootId, sealedTitle: `plain` })).rejects.toThrow()
  })
})

describe.skipIf(!ready.ok)(`the bus with sealed content`, () => {
  async function sealedTree(w: Awaited<ReturnType<typeof world>>) {
    const { tree, seal } = w.seal()
    const rootId = (
      await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: sealBody(tree.publicKey, 1, `root`), seal } })
    ).sessionId
    const childId = (
      await sendSession({ userId: w.owner, input: { parentId: rootId, text: sealBody(tree.publicKey, 1, `child`) } })
    ).sessionId
    await w.host.claim({ sessionId: childId, deviceId: w.deviceId })
    return { tree, rootId, childId }
  }

  it(`a sealed reply: a byte copy in its own tree, the device's copy in a watcher's tree, the listener list back`, async () => {
    const w = await world()
    const { tree, rootId, childId } = await sealedTree(w)
    // A watcher in ANOTHER tree (plaintext), and one with no copy.
    const watcher = await w.root(`watcher`)
    const silent = await w.root(`silent`)
    await db.update(sessions).set({ wakeOn: [childId] }).where(sql`${sessions.id} IN (${watcher}, ${silent})`)
    const body = sealBody(tree.publicKey, 1, `found it`)
    const copy = `exp-sealed:v1:1:${body.split(`:`)[3]}`
    const reply = await w.host.reply({ sessionId: childId, body, listenerCopies: [{ rootId: watcher, body: copy }] })
    // One entry per listening TREE (both keyless here), sorted.
    expect(reply.listeners).toEqual(
      [watcher, silent].sort().map((rootId) => ({ rootId, epoch: null, publicKey: null }))
    )
    expect(await w.host.listeners({ sessionId: childId })).toEqual(reply.listeners)
    expect((await fromChild(rootId, childId)).map((m) => m.body)).toEqual([body])
    expect((await fromChild(watcher, childId)).map((m) => m.body)).toEqual([copy])
    const [neutral] = await fromChild(silent, childId)
    expect(neutral!.body).not.toContain(`exp-sealed:`)
    expect(neutral!.body).toContain(childId)

    // A copy must itself be sealed.
    await expect(
      w.host.reply({ sessionId: childId, body, listenerCopies: [{ rootId: watcher, body: `plain leak` }] })
    ).rejects.toThrow(/listener copy/)
  })

  it(`a plaintext reply keeps its old relay (trimmed copy, no listener list)`, async () => {
    const w = await world()
    const rootId = await w.root()
    const childId = (await sendSession({ userId: w.owner, input: { parentId: rootId, text: `child` } })).sessionId
    await w.host.claim({ sessionId: childId, deviceId: w.deviceId })
    const reply = await w.host.reply({ sessionId: childId, body: `  done  ` })
    expect(reply).not.toHaveProperty(`listeners`)
    expect((await fromChild(rootId, childId)).map((m) => m.body)).toEqual([`done`])
  })

  it(`a sealed card asks with the device's sealed ask text in its tree, neutrally elsewhere`, async () => {
    const w = await world()
    const { tree, rootId, childId } = await sealedTree(w)
    const card = sealCard(tree.publicKey, 1, { kind: `question`, question: `Which DB?`, options: [`pg`] }) as {
      kind: string
      [key: string]: unknown
    }
    const askText = sealBody(tree.publicKey, 1, `Asks: Which DB?`)
    const asked = await w.host.reply({ sessionId: childId, body: ``, card, askText })
    const [ask] = await fromChild(rootId, childId)
    expect(ask).toMatchObject({ body: askText, card: { kind: `ask`, sessionId: childId, messageId: asked.messageId } })
    // Without one: the neutral line, never the card's text.
    const child2 = (await sendSession({ userId: w.owner, input: { parentId: rootId, text: sealBody(tree.publicKey, 1, `c2`) } })).sessionId
    await db.transaction(async (tx) => {
      await busAsk(tx, (await row(child2)) as never, crypto.randomUUID(), card as never)
    })
    const [neutral] = await fromChild(rootId, child2)
    expect(neutral!.body).not.toContain(`exp-sealed:`)
    // An invalid sealed card (content beside `sealed`) is refused.
    await expect(
      w.host.reply({ sessionId: childId, body: ``, card: { ...card, question: `leak` } })
    ).rejects.toThrow(/sealed card/)
  })

  it(`a sealed report card is copied whole (per tree); a sealed summary or last reply is never read into a report`, async () => {
    const w = await world()
    const { tree, rootId, childId } = await sealedTree(w)
    const watcher = await w.root(`watcher`)
    await db.update(sessions).set({ wakeOn: [childId] }).where(eq(sessions.id, watcher))
    const report = { kind: `report`, sessionId: childId, status: `done`, final: true, summary: `x`.repeat(5000) }
    const sealedReport = sealCard(tree.publicKey, 1, report)
    const other = generateTreeKey()
    const copy = sealCard(other.publicKey, 1, report)
    await endSession({ userId: w.owner, sessionId: childId, sealedReport, reportCopies: [{ rootId: watcher, card: copy }] })
    const isReport = (m: { card: unknown }) => (m.card as { kind?: string } | null)?.kind === `report`
    expect((await fromChild(rootId, childId)).filter(isReport)).toMatchObject([{ body: ``, card: sealedReport }])
    expect((await fromChild(watcher, childId)).filter(isReport)).toMatchObject([{ body: ``, card: copy }])
    // A report card carrying content beside `sealed` is refused.
    const child3 = (await sendSession({ userId: w.owner, input: { parentId: rootId, text: sealBody(tree.publicKey, 1, `c3`) } })).sessionId
    await expect(
      endSession({ userId: w.owner, sessionId: child3, sealedReport: { ...sealedReport, summary: `leak` } })
    ).rejects.toThrow(/sealed report/)
    // A sealed summary string never becomes a clear summary.
    await endSession({ userId: w.owner, sessionId: child3, summary: sealBody(tree.publicKey, 1, `words`) })
    expect((await fromChild(rootId, child3)).filter(isReport)).toMatchObject([{ body: REPORT_FALLBACK }])

    const child2 = (await sendSession({ userId: w.owner, input: { parentId: rootId, text: sealBody(tree.publicKey, 1, `c2`) } })).sessionId
    await w.host.claim({ sessionId: child2, deviceId: w.deviceId })
    await w.host.reply({ sessionId: child2, body: sealBody(tree.publicKey, 1, `last words`) })
    await endSession({ userId: w.owner, sessionId: child2 })
    const [fallback] = (await fromChild(rootId, child2)).filter((m) => (m.card as { kind?: string } | null)?.kind === `report`)
    expect(fallback).toMatchObject({ body: REPORT_FALLBACK, card: { summary: REPORT_FALLBACK } })
  })

  it(`a reply's mentionUserIds notify; an edit re-notifies only for an added id`, async () => {
    const w = await world()
    const { tree, childId } = await sealedTree(w)
    notify.mockClear()
    await w.host.reply({ sessionId: childId, body: sealBody(tree.publicKey, 1, `hey @x`), mentionUserIds: [w.owner] })
    expect(notify).toHaveBeenCalledTimes(1)
    notify.mockClear()
    await w.host.reply({ sessionId: childId, body: sealBody(tree.publicKey, 1, `no mention`) })
    expect(notify).not.toHaveBeenCalled()
  })
})
