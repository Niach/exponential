// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { attachments, boards, devices, installs, issues, messages, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { generateTreeKey, sealBody, wrapTreeKey } from "@exp/domain-contract/sealed-content"

// #EXP-1682: a file a sender can read (a person's pasted screenshot on the
// message an agent answers) forwards by id through `sessions.send
// {attachmentIds}` into an existing chat, a new chat or a child: a NEW
// row + blob copy on the new message, no re-upload; a sealed body is never
// touched; another member's private file is refused.

vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  notifySessionAsk: async () => {},
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
const storage = vi.hoisted(() => ({ copies: [] as [string, string][], deleted: [] as string[] }))
vi.mock(`@/lib/storage`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/storage")>()),
  copyObject: async (from: string, to: string) => {
    storage.copies.push([from, to])
  },
  deleteObject: async (key: string) => {
    storage.deleted.push(key)
  },
}))

const ready = await probeRealDb(`
  SELECT (
    EXISTS (SELECT 1 FROM information_schema.columns
      WHERE table_name = 'messages' AND column_name = 'attachment_ids')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
  ) AS ok
`)
if (!ready.ok) console.info(`[forward-attachments.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { answeredMessageFiles, forwardMessageFiles, retargetBody } = await import(`@/lib/sessions/forward-attachments`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `fwd-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@fwd.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function world() {
  const owner = await user()
  const [team] = await db.insert(teams).values({ name: `Fwd`, slug: `fwd-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  const [board] = await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `FW` }).returning({ id: boards.id })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const upload = async (uploaderId = owner, filename = `shot.png`) =>
    (
      await db
        .insert(attachments)
        .values({ teamId, uploaderId, filename, contentType: `image/png`, sizeBytes: 3, storageKey: `k-${id8()}`, url: `x` })
        .returning({ id: attachments.id })
    )[0]!.id
  /** A chat whose person message carries one pasted image. */
  const pasted = async (userId = owner, extra: Record<string, unknown> = {}) => {
    const file = await upload(userId)
    const sent = await sendSession({ userId, input: { teamId, deviceId, text: `look [Image #1]`, attachmentIds: [file], ...extra } })
    return { rootId: sent.sessionId, file }
  }
  return { owner, teamId, deviceId, boardId: board!.id, upload, pasted }
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

const message = async (id: string) => (await db.select().from(messages).where(eq(messages.id, id)).limit(1))[0]!
const file = async (id: string) => (await db.select().from(attachments).where(eq(attachments.id, id)).limit(1))[0]!

describe(`retargetBody`, () => {
  it(`points a forwarded file's url at its copy`, () => {
    expect(retargetBody(`see ![a](/api/attachments/a1) and /api/attachments/b2`, new Map([[`a1`, `c3`]]))).toBe(
      `see ![a](/api/attachments/c3) and /api/attachments/b2`
    )
  })
})

describe.skipIf(!ready.ok)(`forwarding a message's files (sessions.send attachmentIds)`, () => {
  it(`an agent forwards its person's pasted image into a new chat, a child and an existing chat, each a fresh copy`, async () => {
    const w = await world()
    const { rootId, file: source } = await w.pasted()
    expect(await answeredMessageFiles(db, rootId)).toEqual([source])
    const other = (await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `other` } })).sessionId
    const asAgent = { userId: w.owner, callerSessionId: rootId, viaMcp: true }
    const targets = [
      { teamId: w.teamId, deviceId: w.deviceId, text: `new chat` },
      { parentId: rootId, text: `child` },
      { sessionId: other, text: `existing ![shot](/api/attachments/${source})` },
    ]
    const copies: string[] = []
    for (const target of targets) {
      storage.copies.length = 0
      const sent = await sendSession({ ...asAgent, input: { ...target, attachmentIds: [source] } })
      const m = await message(sent.messageId!)
      const [copy] = m.attachmentIds!
      expect(copy).not.toBe(source)
      copies.push(copy!)
      expect(await file(copy!)).toMatchObject({ messageId: m.id, teamId: w.teamId, uploaderId: w.owner, filename: `shot.png`, url: `/api/attachments/${copy}` })
      expect(storage.copies).toHaveLength(1)
      expect(storage.copies[0]![1]).toContain(copy!)
      // The body names the copy (appended, or retargeted in place), never the source.
      expect(m.body).toContain(`/api/attachments/${copy}`)
      expect(m.body).not.toContain(source)
    }
    expect(new Set(copies).size).toBe(3)
    // The source stays where it was.
    expect((await file(source)).messageId).not.toBeNull()
  })

  it(`a sealed message's file forwards into a sealed tree; the sealed body is never touched`, async () => {
    const w = await world()
    const installKey = generateTreeKey()
    const [install] = await db
      .insert(installs)
      .values({ userId: w.owner, kind: `desktop`, endpointId: `ep-${crypto.randomUUID()}`, x25519Public: installKey.publicKey })
      .returning({ id: installs.id })
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
    const a = seal()
    const source = await w.upload()
    const rootId = (
      await sendSession({
        userId: w.owner,
        input: { teamId: w.teamId, deviceId: w.deviceId, text: sealBody(a.tree.publicKey, 1, `look`), attachmentIds: [source], seal: a.seal },
      })
    ).sessionId
    expect(await answeredMessageFiles(db, rootId)).toEqual([source])
    const b = seal()
    const text = sealBody(b.tree.publicKey, 1, `fix this`)
    const sent = await sendSession({
      userId: w.owner,
      callerSessionId: rootId,
      viaMcp: true,
      input: { teamId: w.teamId, deviceId: w.deviceId, text, attachmentIds: [source], seal: b.seal },
    })
    const m = await message(sent.messageId!)
    expect(m.body).toBe(text)
    expect(m.attachmentIds).toHaveLength(1)
    expect(m.attachmentIds![0]).not.toBe(source)
  })

  it(`refuses another member's private file and an unknown id`, async () => {
    const w = await world()
    const mate = await user()
    await db.insert(teamMembers).values({ teamId: w.teamId, userId: mate, role: `member` })
    const mateDevice = `dev-${id8()}`
    await db.insert(devices).values({ userId: mate, deviceId: mateDevice, label: `theirs`, kind: `server` })
    const { file: theirs } = await w.pasted(mate, { deviceId: mateDevice })
    const mine = (await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `mine` } })).sessionId
    for (const id of [theirs, crypto.randomUUID()]) {
      await expect(sendSession({ userId: w.owner, input: { sessionId: mine, text: `x`, attachmentIds: [id] } })).rejects.toThrow(
        /files of messages you can read/
      )
    }
  })

  it(`a pasted image forwards onto an issue, as the comment author's own upload (exponential_comments create)`, async () => {
    const w = await world()
    const { file: source } = await w.pasted()
    const [issue] = await db
      .insert(issues)
      .values({ boardId: w.boardId, teamId: w.teamId, title: `Bug`, creatorId: w.owner })
      .returning({ id: issues.id })
    const forwarded = await db.transaction((tx) =>
      forwardMessageFiles(tx, {
        ids: [source, crypto.randomUUID()],
        teamId: w.teamId,
        readerIds: [w.owner],
        uploaderId: w.owner,
        owner: { kind: `issue`, issueId: issue!.id },
      })
    )
    expect([...forwarded.keys()]).toEqual([source])
    // The issue row: the board mirror is derived, the comment rule (same
    // issue, uploaded by the author, no comment yet) holds.
    expect(await file(forwarded.get(source)!)).toMatchObject({
      issueId: issue!.id,
      boardId: w.boardId,
      messageId: null,
      commentId: null,
      uploaderId: w.owner,
    })
  })
})
