// @vitest-environment node
//
// #EXP-2123 (fixtures/install-trust.json `keyMaker`): only an APPROVED
// install makes a chat key, against the REAL migrated Postgres. An install
// the server lists as waiting (`installs.waiting`'s New install rule) is
// refused every key write (a first key, a rotation it races for) and is
// named nothing due; a chat STRANDED before this rule (its current epoch
// held only by a waiting install, or by none) is found, flagged and rotated
// by an approved reader that never held the old secret.
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, inArray, sql } from "drizzle-orm"
import { randomBytes } from "node:crypto"
import { db } from "@/db/connection"
import { devices, messages, sessionKeys, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { installX25519Of, KEY_MAKER_REFUSAL, signApproval, signRegistration } from "@exp/domain-contract/install-trust"
import { generateTreeKey, sealBody, wrapTreeKey } from "@exp/domain-contract/sealed-content"
import { endpointIdOf } from "@exp/domain-contract/wrap-signature"

process.env.BETTER_AUTH_SECRET ||= `key-maker-test-secret-0123456789abcdef`

vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: false }),
}))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'installs' AND column_name = 'approval_requested_at') AS ok
`)
if (!ready.ok) console.info(`[key-maker.real-db] skipped: ${ready.reason}`)

const { installsRouter } = await import(`@/lib/trpc/installs`)
const { sessionKeysRouter } = await import(`@/lib/trpc/session-keys`)
const { findStrandedKeys, flagStrandedKeysInTx } = await import(`@/lib/sessions/stranded-keys`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }
const ctx = (userId: string) => ({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) }) as never
const api = (userId: string) => installsRouter.createCaller(ctx(userId))
const keys = (userId: string) => sessionKeysRouter.createCaller(ctx(userId))

async function user(): Promise<string> {
  const id = `maker-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@maker.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function team(...members: string[]): Promise<string> {
  const [row] = await db.insert(teams).values({ name: `Maker`, slug: `maker-${id8()}` }).returning({ id: teams.id })
  made.teams.push(row!.id)
  for (const [at, userId] of members.entries()) await db.insert(teamMembers).values({ teamId: row!.id, userId, role: at === 0 ? `owner` : `member` })
  return row!.id
}

type Install = { userId: string; installId: string; identity: Uint8Array; endpointId: string; x25519Public: string }

/** A real registration (`installs.register`): its signature is stored. */
async function install(userId: string, label = `Browser`): Promise<Install> {
  const identity = new Uint8Array(randomBytes(32))
  const signedAt = new Date().toISOString()
  const endpointId = endpointIdOf(identity)
  const x25519Public = installX25519Of(identity)
  const { installId } = await api(userId).register({ kind: `web`, label, endpointId, x25519Public, signedAt, signature: signRegistration(identity, userId, signedAt) })
  return { userId, installId, identity, endpointId, x25519Public }
}

const subjectOf = (who: Install) => ({ userId: who.userId, endpointId: who.endpointId, x25519Public: who.x25519Public })
const claim = (who: Install) => api(who.userId).claimRoot({ installId: who.installId, signature: signApproval(who.identity, `root`, subjectOf(who)) })
const approve = (by: Install, subject: Install) =>
  api(by.userId).approve({ installId: by.installId, subjectInstallId: subject.installId, kind: `approve`, signature: signApproval(by.identity, `approve`, subjectOf(subject)) })

/** A private chat of `starter` (keyless). */
async function chat(teamId: string, starter: string, shared = false): Promise<string> {
  const rootId = crypto.randomUUID()
  const machine = `dev-${id8()}`
  await db.insert(devices).values({ userId: starter, deviceId: machine, label: machine, kind: `desktop` })
  await db.insert(sessions).values({ id: rootId, rootId, teamId, userId: starter, deviceId: machine, shared } as never)
  return rootId
}

/** A key write as a client makes it: the wraps it names, its own first. */
function write(who: Install, rootId: string, epoch: number, to: Install[] = [who]) {
  const tree = generateTreeKey()
  return {
    tree,
    call: keys(who.userId).create({ rootId, installId: who.installId, epoch, publicKey: tree.publicKey, wraps: to.map((reader) => ({ installId: reader.installId, wrapped: wrapTreeKey(reader.x25519Public, tree.secret) })) }),
  }
}

const refused = { code: `FORBIDDEN`, message: KEY_MAKER_REFUSAL }
const rootRow = async (rootId: string) => (await db.select().from(sessions).where(eq(sessions.id, rootId)))[0]!
const wrapRows = async (rootId: string) => db.select().from(sessionKeys).where(eq(sessionKeys.rootId, rootId))

/** The account of the bug: A started the chain, N registered afterwards. */
async function account() {
  const me = await user()
  const teamId = await team(me)
  const A = await install(me, `MacBook`)
  await claim(A)
  const N = await install(me, `Firefox`)
  return { me, teamId, A, N }
}

/** The state the bug left behind (written past the router, as before the
 *  rule): `maker` keyed `rootId` at `epoch` with its own wrap only. */
async function strand(rootId: string, maker: Install | null, epoch = 1) {
  const tree = generateTreeKey()
  await db.update(sessions).set({ sealPublicKey: tree.publicKey, sealEpoch: epoch }).where(eq(sessions.id, rootId))
  if (maker) await db.insert(sessionKeys).values({ rootId, epoch, installId: maker.installId, wrapped: wrapTreeKey(maker.x25519Public, tree.secret), createdByInstallId: maker.installId })
  return tree
}

async function message(rootId: string, teamId: string, author: string, body: string) {
  await db.insert(messages).values({ conversationId: sql`(SELECT id FROM conversations WHERE session_id = ${rootId})`, sessionId: rootId, rootId, teamId, authorKind: `user`, authorId: author, body } as never)
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.delete(teams).where(inArray(teams.id, made.teams))
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`#EXP-2123 a waiting install makes no chat key`, () => {
  it(`the first key of a chat: refused while it waits (asked, or never asked beside its account's chain), nothing written; made once approved`, async () => {
    const { me, teamId, A, N } = await account()
    const rootId = await chat(teamId, me)
    // Never asked: its account has a chain without it (a New install row, any build).
    await expect(write(N, rootId, 1).call).rejects.toMatchObject(refused)
    await api(me).requestApproval({ installId: N.installId })
    await expect(write(N, rootId, 1).call).rejects.toMatchObject(refused)
    // Not even with a wrap for the approved install beside its own.
    await expect(write(N, rootId, 1, [N, A]).call).rejects.toMatchObject(refused)
    expect(await rootRow(rootId)).toMatchObject({ sealEpoch: null, sealPublicKey: null })
    expect(await wrapRows(rootId)).toEqual([])

    await approve(A, N)
    await write(N, rootId, 1, [N, A]).call
    expect(await rootRow(rootId)).toMatchObject({ sealEpoch: 1 })
    expect((await wrapRows(rootId)).map((row) => row.installId).sort()).toEqual([A.installId, N.installId].sort())
  })

  it(`racing a due rotation: the waiting install is named nothing due and its rotation is refused; the approved holder rotates`, async () => {
    const { me, teamId, A, N } = await account()
    await api(me).requestApproval({ installId: N.installId })
    const rootId = await chat(teamId, me)
    // Epoch 1 by the approved install; the waiting one holds it too (an older build wrapped it).
    const first = write(A, rootId, 1, [A, N])
    await first.call
    await db.update(sessions).set({ sealRotateEpoch: 1 }).where(eq(sessions.id, rootId))

    expect(await keys(me).due({ installId: N.installId })).toEqual([])
    await expect(write(N, rootId, 2).call).rejects.toMatchObject(refused)
    expect(await rootRow(rootId)).toMatchObject({ sealEpoch: 1, sealRotateEpoch: 1, sealPublicKey: first.tree.publicKey })

    expect(await keys(me).due({ installId: A.installId })).toEqual([{ rootId, epoch: 1 }])
    await write(A, rootId, 2).call
    expect(await rootRow(rootId)).toMatchObject({ sealEpoch: 2, sealRotateEpoch: null })
  })

  it(`who does not wait: the first install before its root claim, an account nobody runs this release in, an install on the chain`, async () => {
    const me = await user()
    const teamId = await team(me)
    // No chain in the account, nobody asked: older builds, and the first install before its claim.
    const old = await install(me, `Old phone`)
    const other = await install(me, `Old browser`)
    await write(old, await chat(teamId, me), 1, [old, other]).call
    // One of them runs this release beside the others and asks: it waits.
    await api(me).requestApproval({ installId: other.installId })
    await expect(write(other, await chat(teamId, me), 1).call).rejects.toMatchObject(refused)
    // The one that did not ask still keys; once it claims the root, as well.
    await write(old, await chat(teamId, me), 1).call
    await claim(old)
    await write(old, await chat(teamId, me), 1).call
  })
})

describe.skipIf(!ready.ok)(`#EXP-2123 the repair: stranded chats are found, flagged and rotated by an approved reader`, () => {
  it(`a chat keyed by a waiting install: found with its sealed messages; once flagged, the approved install is named it stranded and rotates without the old secret`, async () => {
    const { me, teamId, A, N } = await account()
    await api(me).requestApproval({ installId: N.installId })
    const rootId = await chat(teamId, me)
    const lost = await strand(rootId, N)
    await message(rootId, teamId, me, sealBody(lost.publicKey, 1, `only the waiting install opens this`))
    await message(rootId, teamId, me, `a server line in clear`)
    // A healthy chat beside it is never found.
    const healthy = await chat(teamId, me)
    await write(A, healthy, 1).call

    const mine = async () => (await findStrandedKeys(db)).filter((root) => root.teamId === teamId)
    expect(await mine()).toEqual([{ rootId, teamId, epoch: 1, kind: `waiting`, holders: [N.installId], flagged: false, messages: 1 }])
    // Not flagged yet: nobody is named it.
    expect(await keys(me).due({ installId: A.installId })).toEqual([])

    const found = await mine()
    expect(await db.transaction((tx) => flagStrandedKeysInTx(tx, found))).toBe(1)
    expect(await db.transaction((tx) => flagStrandedKeysInTx(tx, [{ rootId, epoch: 1 }]))).toBe(0)
    expect(await keys(me).due({ installId: N.installId })).toEqual([])
    expect(await keys(me).due({ installId: A.installId })).toEqual([{ rootId, epoch: 1, stranded: true }])

    await write(A, rootId, 2).call
    expect(await rootRow(rootId)).toMatchObject({ sealEpoch: 2, sealRotateEpoch: null })
    expect(await mine()).toEqual([])
    expect(await keys(me).due({ installId: A.installId })).toEqual([])
  })

  it(`approving the waiting install instead loses nothing: the chat is no longer stranded and it wraps for the readers`, async () => {
    const { me, teamId, A, N } = await account()
    await api(me).requestApproval({ installId: N.installId })
    const rootId = await chat(teamId, me)
    await strand(rootId, N)
    const mine = async () => (await findStrandedKeys(db)).filter((root) => root.teamId === teamId)
    expect((await mine()).map((root) => root.kind)).toEqual([`waiting`])
    await approve(A, N)
    expect(await mine()).toEqual([])
    expect((await keys(me).pending({ installId: N.installId })).map((pair) => pair.targetInstallId)).toContain(A.installId)
  })

  it(`a chat whose current key no live install holds (its maker was removed): found as noHolder; a team-mate's install rotates a shared one`, async () => {
    const me = await user()
    const mate = await user()
    const teamId = await team(me, mate)
    const A = await install(me)
    await claim(A)
    const M = await install(mate)
    await claim(M)
    const rootId = await chat(teamId, me, true)
    await strand(rootId, null)
    const found = (await findStrandedKeys(db)).filter((root) => root.teamId === teamId)
    expect(found).toEqual([{ rootId, teamId, epoch: 1, kind: `noHolder`, holders: [], flagged: false, messages: 0 }])
    await db.transaction((tx) => flagStrandedKeysInTx(tx, found))
    expect(await keys(mate).due({ installId: M.installId })).toEqual([{ rootId, epoch: 1, stranded: true }])
    await write(M, rootId, 2).call
    expect(await keys(me).due({ installId: A.installId })).toEqual([])
    // A private chat of another member is never named to a team-mate.
    const secret = await chat(teamId, me)
    await strand(secret, null)
    await db.update(sessions).set({ sealRotateEpoch: 1 }).where(eq(sessions.id, secret))
    expect(await keys(mate).due({ installId: M.installId })).toEqual([])
    expect(await keys(me).due({ installId: A.installId })).toEqual([{ rootId: secret, epoch: 1, stranded: true }])
  })
})
