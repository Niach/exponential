// @vitest-environment node
//
// #EXP-1950 decision (c) (fixtures/wrap-signature.json `resign`): a holder
// of a root's current epoch signs it again through `sessionKeys.resign`,
// which replaces a reader's UNSIGNED wrap and never a signed one. Through
// the routers against the REAL migrated Postgres.
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, inArray, sql } from "drizzle-orm"
import { randomBytes } from "node:crypto"
import { db } from "@/db/connection"
import { devices, installs, sessionKeys, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { b64urlEncode, generateTreeKey, pushPublicKey, wrapTreeKey } from "@exp/domain-contract/sealed-content"
import { acceptWrap, endpointIdOf, signWrap } from "@exp/domain-contract/wrap-signature"
import { RESIGN_NOT_READER, WRAP_SIGNATURE_REFUSAL } from "@/lib/sessions/wrap-signatures"

process.env.BETTER_AUTH_SECRET ||= `resign-test-secret-0123456789abcdef`

vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: false }),
}))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'session_keys' AND column_name = 'signer_endpoint_id') AS ok
`)
if (!ready.ok) console.info(`[session-keys-resign.real-db] skipped: ${ready.reason}`)

const { sessionKeysRouter } = await import(`@/lib/trpc/session-keys`)
const { installsRouter } = await import(`@/lib/trpc/installs`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }
const ctx = (userId: string) => ({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) }) as never
const keys = (userId: string) => sessionKeysRouter.createCaller(ctx(userId))

type Install = { id: string; identity: Uint8Array; endpointId: string; installSecret: Uint8Array; x25519Public: string }

async function user(): Promise<string> {
  const id = `resign-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@resign.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function install(userId: string): Promise<Install> {
  const identity = new Uint8Array(randomBytes(32))
  const installSecret = new Uint8Array(randomBytes(32))
  const endpointId = endpointIdOf(identity)
  const x25519Public = b64urlEncode(pushPublicKey(installSecret))
  const [row] = await db.insert(installs).values({ userId, kind: `desktop`, endpointId, x25519Public }).returning({ id: installs.id })
  return { id: row!.id, identity, endpointId, installSecret, x25519Public }
}

/** A member with three installs (H holds and re-signs, R and O read) and a
 *  root keyed at epoch 1 by an OLD build: every wrap unsigned. */
async function world() {
  const me = await user()
  const [team] = await db.insert(teams).values({ name: `Resign`, slug: `resign-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: me, role: `owner` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: me, deviceId, label: `box`, kind: `desktop` })
  const H = await install(me)
  const R = await install(me)
  const O = await install(me)
  const rootId = crypto.randomUUID()
  await db.insert(sessions).values({ id: rootId, rootId, teamId, userId: me, deviceId, status: `idle`, title: `Chat` } as never)
  const tree = generateTreeKey()
  await keys(me).create({
    rootId,
    installId: H.id,
    epoch: 1,
    publicKey: tree.publicKey,
    wraps: [H, R, O].map((target) => ({ installId: target.id, wrapped: wrapTreeKey(target.x25519Public, tree.secret) })),
  })
  /** H's fresh signed wrap of `epoch` for `target` (signed by `signer`). */
  const resigned = (target: Install, opts: { epoch?: number; signer?: Install; secret?: Uint8Array; publicKey?: string } = {}) => {
    const epoch = opts.epoch ?? 1
    const wrapped = wrapTreeKey(target.x25519Public, opts.secret ?? tree.secret)
    const statement = { rootId, epoch, treePublicKey: opts.publicKey ?? tree.publicKey, recipientEndpointId: target.endpointId, wrapped }
    return { rootId, epoch, installId: target.id, wrapped, signature: signWrap((opts.signer ?? H).identity, statement) }
  }
  return { me, teamId, rootId, tree, H, R, O, resigned }
}

const rowOf = async (rootId: string, epoch: number, installId: string) =>
  (await db.select().from(sessionKeys).where(and(eq(sessionKeys.rootId, rootId), eq(sessionKeys.epoch, epoch), eq(sessionKeys.installId, installId))))[0]

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.delete(teams).where(inArray(teams.id, made.teams))
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`#EXP-1950 sessionKeys.resign`, () => {
  it(`replaces an unsigned row; the reader verifies the new wrap; a signed row is never touched again`, async () => {
    const w = await world()
    const first = w.resigned(w.R)
    expect(await keys(w.me).resign({ installId: w.H.id, wraps: [first] })).toEqual({ replaced: 1, skipped: 0 })
    expect(await rowOf(w.rootId, 1, w.R.id)).toMatchObject({
      wrapped: first.wrapped,
      signature: first.signature,
      signerEndpointId: w.H.endpointId,
      createdByInstallId: w.H.id,
    })
    const [served] = await keys(w.me).mine({ installId: w.R.id, rootIds: [w.rootId] })
    expect(acceptWrap(served!, { identity: w.R.identity, installSecret: w.R.installSecret }, [w.H.endpointId], true)).toMatchObject({
      status: `verified`,
      treePublicKey: w.tree.publicKey,
    })
    // A second, valid signed wrap by another holder: skipped, the first stays.
    const again = w.resigned(w.R, { signer: w.O })
    expect(await keys(w.me).resign({ installId: w.O.id, wraps: [again] })).toEqual({ replaced: 0, skipped: 1 })
    expect(await rowOf(w.rootId, 1, w.R.id)).toMatchObject({ wrapped: first.wrapped, signature: first.signature, signerEndpointId: w.H.endpointId })
  })

  it(`a wrong epoch is skipped (its signature unchecked); a pair without a row is skipped`, async () => {
    const w = await world()
    const stale = { ...w.resigned(w.R, { epoch: 2 }), signature: `x`.repeat(86) }
    const R2 = await install(w.me) // a reader with no row yet: `wrap`'s job
    expect(await keys(w.me).resign({ installId: w.H.id, wraps: [stale, w.resigned(R2), w.resigned(w.O)] })).toEqual({ replaced: 1, skipped: 2 })
    expect(await rowOf(w.rootId, 1, R2.id)).toBeUndefined()
    expect((await rowOf(w.rootId, 1, w.R.id))!.signature).toBeNull()
    expect((await rowOf(w.rootId, 1, w.O.id))!.signature).not.toBeNull()
  })

  it(`a bad signature refuses the whole call, nothing written`, async () => {
    const w = await world()
    const good = w.resigned(w.R)
    for (const bad of [
      w.resigned(w.O, { signer: w.R }), // not the caller's install
      w.resigned(w.O, { publicKey: generateTreeKey().publicKey }), // not the stored key
      { ...w.resigned(w.O), wrapped: w.resigned(w.O).wrapped }, // the signature is over other bytes
    ]) {
      await expect(keys(w.me).resign({ installId: w.H.id, wraps: [good, bad] })).rejects.toMatchObject({ code: `BAD_REQUEST`, message: WRAP_SIGNATURE_REFUSAL })
    }
    for (const target of [w.R, w.O]) expect((await rowOf(w.rootId, 1, target.id))!.signature).toBeNull()
  })

  it(`a target that is no reader refuses the call; a caller that reads nothing or is revoked is refused`, async () => {
    const w = await world()
    const strangerId = await user()
    const stranger = await install(strangerId)
    await expect(keys(w.me).resign({ installId: w.H.id, wraps: [w.resigned(w.R), w.resigned(stranger)] })).rejects.toMatchObject({
      code: `FORBIDDEN`,
      message: RESIGN_NOT_READER,
    })
    expect((await rowOf(w.rootId, 1, w.R.id))!.signature).toBeNull()
    // Another account's caller reads nothing here.
    await expect(keys(strangerId).resign({ installId: stranger.id, wraps: [w.resigned(w.R, { signer: stranger })] })).rejects.toMatchObject({ code: `FORBIDDEN` })
    // A revoked caller.
    await db.update(installs).set({ revokedAt: new Date() }).where(eq(installs.id, w.H.id))
    await expect(keys(w.me).resign({ installId: w.H.id, wraps: [w.resigned(w.R)] })).rejects.toMatchObject({ code: `FORBIDDEN` })
    expect((await rowOf(w.rootId, 1, w.R.id))!.signature).toBeNull()
  })

  it(`a dropped reader gets nothing: its wrap is gone and a resign for it is refused`, async () => {
    const w = await world()
    await installsRouter.createCaller(ctx(w.me)).revoke({ installId: w.O.id })
    expect(await rowOf(w.rootId, 1, w.O.id)).toBeUndefined()
    await expect(keys(w.me).resign({ installId: w.H.id, wraps: [w.resigned(w.O)] })).rejects.toMatchObject({ code: `FORBIDDEN`, message: RESIGN_NOT_READER })
    expect(await rowOf(w.rootId, 1, w.O.id)).toBeUndefined()
  })

  it(`one pair twice is refused`, async () => {
    const w = await world()
    const wrap = w.resigned(w.R)
    await expect(keys(w.me).resign({ installId: w.H.id, wraps: [wrap, wrap] })).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })
})
