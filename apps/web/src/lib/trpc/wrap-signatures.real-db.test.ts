// @vitest-environment node
//
// #EXP-1950 (PR 2, the server half of signed key wraps;
// fixtures/wrap-signature.json): every key write takes an OPTIONAL signature
// per wrap, verified against the caller's own install over exactly what is
// stored, and served byte for byte; an unsigned write (every released
// client) still succeeds. Through the routers against the REAL migrated
// Postgres: `sessionKeys.create`, `wrap`, `mine`, `pending`, `publicKeys`,
// the inline `seal` of `sessions.send`, `voice.callConversation` and
// `sessions.sendTarget`.
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, inArray, sql } from "drizzle-orm"
import { randomBytes } from "node:crypto"
import { db } from "@/db/connection"
import { agents, boards, devices, installs, sessionKeys, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { b64urlEncode, generateTreeKey, pushPublicKey, sealBody, wrapTreeKey } from "@exp/domain-contract/sealed-content"
import { acceptVouch, acceptWrap, endpointIdOf, signWrap } from "@exp/domain-contract/wrap-signature"
import { WRAP_SIGNATURE_REFUSAL } from "@/lib/sessions/wrap-signatures"

process.env.BETTER_AUTH_SECRET ||= `wrap-signatures-test-secret-0123456789abcdef`

vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: false }),
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
  notifySessionAsk: async () => {},
}))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'session_keys' AND column_name = 'signer_endpoint_id') AS ok
`)
if (!ready.ok) console.info(`[wrap-signatures.real-db] skipped: ${ready.reason}`)

const { sessionKeysRouter } = await import(`@/lib/trpc/session-keys`)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
const { voiceRouter } = await import(`@/lib/trpc/voice`)
const { mintCallId } = await import(`@/lib/voice-call-id`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }
const ctx = (userId: string) => ({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) }) as never
const keys = (userId: string) => sessionKeysRouter.createCaller(ctx(userId))
const sessionsOf = (userId: string) => sessionsRouter.createCaller(ctx(userId))
const voice = (userId: string) => voiceRouter.createCaller(ctx(userId))

type Install = { id: string; identity: Uint8Array; endpointId: string; installSecret: Uint8Array; x25519Public: string }

async function install(userId: string, opts: { revoked?: boolean } = {}): Promise<Install> {
  const identity = new Uint8Array(randomBytes(32))
  const installSecret = new Uint8Array(randomBytes(32))
  const endpointId = endpointIdOf(identity)
  const x25519Public = b64urlEncode(pushPublicKey(installSecret))
  const [row] = await db
    .insert(installs)
    .values({ userId, kind: `desktop`, endpointId, x25519Public, revokedAt: opts.revoked ? new Date() : null })
    .returning({ id: installs.id })
  return { id: row!.id, identity, endpointId, installSecret, x25519Public }
}

/** A member with a device, a keyless root, an agent there and three installs:
 *  S signs, R reads, O is another install of the member (a wrong signer). */
async function world() {
  const me = `wrap-sig-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${me}, 'Tester', ${`${me}@wrap.test`}, true, false, now(), now())
  `)
  made.users.push(me)
  const [team] = await db.insert(teams).values({ name: `Wrap`, slug: `wrap-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: me, role: `owner` })
  await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `WS` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: me, deviceId, label: `box`, kind: `desktop` })
  const [agent] = await db.insert(agents).values({ teamId, name: `Helper`, deviceId, ownerId: me }).returning({ id: agents.id })
  const root = async () => {
    const id = crypto.randomUUID()
    await db.insert(sessions).values({ id, rootId: id, teamId, userId: me, deviceId, status: `idle`, title: `Chat` } as never)
    return id
  }
  return { me, teamId, deviceId, agentId: agent!.id, S: await install(me), R: await install(me), O: await install(me), root }
}

/** A key of `rootId` at `epoch`, wrapped for `to`; `sign` = how each wrap is signed. */
function treeKey(
  rootId: string,
  epoch: number,
  maker: Install,
  to: Install[],
  sign?: (wrap: { rootId: string; epoch: number; treePublicKey: string; recipientEndpointId: string; wrapped: string }) => string
) {
  const tree = generateTreeKey()
  return {
    tree,
    input: {
      rootId,
      installId: maker.id,
      epoch,
      publicKey: tree.publicKey,
      wraps: to.map((target) => {
        const wrapped = wrapTreeKey(target.x25519Public, tree.secret)
        const statement = { rootId, epoch, treePublicKey: tree.publicKey, recipientEndpointId: target.endpointId, wrapped }
        return { installId: target.id, wrapped, ...(sign ? { signature: sign(statement) } : {}) }
      }),
    },
  }
}

const rowsOf = async (rootId: string) =>
  (await db.select().from(sessionKeys).where(eq(sessionKeys.rootId, rootId))).sort((a, b) => (a.installId < b.installId ? -1 : 1))
const refused = { code: `BAD_REQUEST`, message: WRAP_SIGNATURE_REFUSAL }

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.delete(teams).where(inArray(teams.id, made.teams))
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`#EXP-1950 signed wraps on sessionKeys.create`, () => {
  it(`stores a valid signature and serves it byte for byte; the recipient's acceptWrap verifies it`, async () => {
    const w = await world()
    const rootId = await w.root()
    const { tree, input } = treeKey(rootId, 1, w.S, [w.S, w.R], (statement) => signWrap(w.S.identity, statement))
    await keys(w.me).create(input)
    const stored = await rowsOf(rootId)
    expect(stored).toHaveLength(2)
    for (const row of stored) {
      const sent = input.wraps.find((wrap) => wrap.installId === row.installId)!
      expect(row).toMatchObject({ wrapped: sent.wrapped, signature: sent.signature, signerEndpointId: w.S.endpointId, createdByInstallId: w.S.id })
    }
    const [served] = await keys(w.me).mine({ installId: w.R.id, rootIds: [rootId] })
    const sent = input.wraps.find((wrap) => wrap.installId === w.R.id)!
    expect(served).toEqual({ rootId, epoch: 1, wrapped: sent.wrapped, signature: sent.signature, signerEndpointId: w.S.endpointId })
    const verdict = acceptWrap(served!, { identity: w.R.identity, installSecret: w.R.installSecret }, [w.S.endpointId], true)
    expect(verdict).toMatchObject({ status: `verified`, treePublicKey: tree.publicKey, signerEndpointId: w.S.endpointId })
  })

  it(`an unsigned write from an old client still succeeds, stored and served as null`, async () => {
    const w = await world()
    const rootId = await w.root()
    const { input } = treeKey(rootId, 1, w.S, [w.S, w.R])
    await keys(w.me).create(input)
    expect((await rowsOf(rootId)).map((row) => [row.signature, row.signerEndpointId])).toEqual([
      [null, null],
      [null, null],
    ])
    expect(await keys(w.me).mine({ installId: w.R.id, rootIds: [rootId] })).toEqual([
      { rootId, epoch: 1, wrapped: input.wraps[1]!.wrapped, signature: null, signerEndpointId: null },
    ])
  })

  it(`a mix: an explicit null is unsigned, the signed wrap beside it is checked`, async () => {
    const w = await world()
    const rootId = await w.root()
    const { input } = treeKey(rootId, 1, w.S, [w.S, w.R], (statement) => signWrap(w.S.identity, statement))
    input.wraps[0] = { ...input.wraps[0]!, signature: null as never }
    await keys(w.me).create(input)
    const stored = await rowsOf(rootId)
    expect(stored.find((row) => row.installId === w.S.id)).toMatchObject({ signature: null, signerEndpointId: null })
    expect(stored.find((row) => row.installId === w.R.id)).toMatchObject({ signerEndpointId: w.S.endpointId })
  })

  const attacks: Array<[string, (w: Awaited<ReturnType<typeof world>>, rootId: string) => ReturnType<typeof treeKey>]> = [
    [
      `a tampered signature`,
      (w, rootId) =>
        treeKey(rootId, 1, w.S, [w.S, w.R], (statement) => {
          const good = signWrap(w.S.identity, statement)
          return `${good.slice(0, 10)}${good[10] === `A` ? `B` : `A`}${good.slice(11)}`
        }),
    ],
    [`a wrong signer (another install's identity)`, (w, rootId) => treeKey(rootId, 1, w.S, [w.S, w.R], (statement) => signWrap(w.O.identity, statement))],
    [
      `a signature for another root`,
      (w, rootId) => treeKey(rootId, 1, w.S, [w.S, w.R], (statement) => signWrap(w.S.identity, { ...statement, rootId: crypto.randomUUID() })),
    ],
    [`a signature for another epoch`, (w, rootId) => treeKey(rootId, 1, w.S, [w.S, w.R], (statement) => signWrap(w.S.identity, { ...statement, epoch: 2 }))],
    [
      `a signature for another recipient`,
      (w, rootId) => treeKey(rootId, 1, w.S, [w.S, w.R], (statement) => signWrap(w.S.identity, { ...statement, recipientEndpointId: w.O.endpointId })),
    ],
    [
      `a signature over another tree public key`,
      (w, rootId) =>
        treeKey(rootId, 1, w.S, [w.S, w.R], (statement) => signWrap(w.S.identity, { ...statement, treePublicKey: generateTreeKey().publicKey })),
    ],
    [`a malformed signature`, (w, rootId) => treeKey(rootId, 1, w.S, [w.S, w.R], () => `not-a-signature`)],
  ]
  for (const [name, make] of attacks) {
    it(`refuses ${name}, and writes nothing`, async () => {
      const w = await world()
      const rootId = await w.root()
      await expect(keys(w.me).create(make(w, rootId).input)).rejects.toMatchObject(refused)
      expect(await rowsOf(rootId)).toEqual([])
      expect((await db.select().from(sessions).where(eq(sessions.id, rootId)))[0]).toMatchObject({ sealEpoch: null, sealPublicKey: null })
    })
  }

  it(`refuses a revoked signer`, async () => {
    const w = await world()
    const rootId = await w.root()
    const revoked = await install(w.me, { revoked: true })
    const { input } = treeKey(rootId, 1, revoked, [revoked, w.R], (statement) => signWrap(revoked.identity, statement))
    await expect(keys(w.me).create(input)).rejects.toMatchObject({ code: `FORBIDDEN` })
    expect(await rowsOf(rootId)).toEqual([])
  })

  it(`a rotation is signed over the new epoch and key; one signed for the old epoch is refused`, async () => {
    const w = await world()
    const rootId = await w.root()
    await keys(w.me).create(treeKey(rootId, 1, w.S, [w.S, w.R], (statement) => signWrap(w.S.identity, statement)).input)
    await expect(
      keys(w.me).create(treeKey(rootId, 2, w.S, [w.S, w.R], (statement) => signWrap(w.S.identity, { ...statement, epoch: 1 })).input)
    ).rejects.toMatchObject(refused)
    const next = treeKey(rootId, 2, w.S, [w.S, w.R], (statement) => signWrap(w.S.identity, statement))
    await keys(w.me).create(next.input)
    const served = await keys(w.me).mine({ installId: w.R.id, rootIds: [rootId] })
    const epoch2 = served.find((row) => row.epoch === 2)!
    expect(acceptWrap(epoch2, { identity: w.R.identity, installSecret: w.R.installSecret }, [w.S.endpointId], true)).toMatchObject({
      status: `verified`,
      treePublicKey: next.tree.publicKey,
    })
  })
})

describe.skipIf(!ready.ok)(`#EXP-1950 pending, wrap and the vouch`, () => {
  it(`pending names targetEndpointId; a holder's signed wrap is stored, a bad one refuses the batch`, async () => {
    const w = await world()
    const rootId = await w.root()
    const { tree } = await (async () => {
      const made = treeKey(rootId, 1, w.S, [w.S], (statement) => signWrap(w.S.identity, statement))
      await keys(w.me).create(made.input)
      return made
    })()
    const pairs = await keys(w.me).pending({ installId: w.S.id, rootIds: [rootId] })
    const forR = pairs.find((pair) => pair.targetInstallId === w.R.id)!
    expect(forR).toMatchObject({ rootId, epoch: 1, publicKey: tree.publicKey, targetX25519Public: w.R.x25519Public, targetEndpointId: w.R.endpointId })
    const forO = pairs.find((pair) => pair.targetInstallId === w.O.id)!
    expect(forO.targetEndpointId).toBe(w.O.endpointId)

    const wrapFor = (pair: typeof forR) => {
      const wrapped = wrapTreeKey(pair.targetX25519Public, tree.secret)
      return { wrapped, statement: { rootId, epoch: 1, treePublicKey: pair.publicKey, recipientEndpointId: pair.targetEndpointId, wrapped } }
    }
    const r = wrapFor(forR)
    const o = wrapFor(forO)
    // One bad signature (made for R's endpoint, on O's wrap) refuses the whole batch.
    await expect(
      keys(w.me).wrap({
        installId: w.S.id,
        wraps: [
          { rootId, epoch: 1, targetInstallId: w.R.id, wrapped: r.wrapped, signature: signWrap(w.S.identity, r.statement) },
          { rootId, epoch: 1, targetInstallId: w.O.id, wrapped: o.wrapped, signature: signWrap(w.S.identity, { ...o.statement, recipientEndpointId: w.R.endpointId }) },
        ],
      })
    ).rejects.toMatchObject(refused)
    expect((await rowsOf(rootId)).map((row) => row.installId)).toEqual([w.S.id])
    // A wrong signer on its own is refused too.
    await expect(
      keys(w.me).wrap({ installId: w.S.id, wraps: [{ rootId, epoch: 1, targetInstallId: w.R.id, wrapped: r.wrapped, signature: signWrap(w.O.identity, r.statement) }] })
    ).rejects.toMatchObject(refused)

    // Signed for R, unsigned for O (an old holder's write): both land.
    expect(
      await keys(w.me).wrap({
        installId: w.S.id,
        wraps: [
          { rootId, epoch: 1, targetInstallId: w.R.id, wrapped: r.wrapped, signature: signWrap(w.S.identity, r.statement) },
          { rootId, epoch: 1, targetInstallId: w.O.id, wrapped: o.wrapped },
        ],
      })
    ).toEqual({ written: 2, skipped: 0 })
    const [served] = await keys(w.me).mine({ installId: w.R.id, rootIds: [rootId] })
    expect(served).toMatchObject({ signature: signWrap(w.S.identity, r.statement), signerEndpointId: w.S.endpointId })
    expect(acceptWrap(served!, { identity: w.R.identity, installSecret: w.R.installSecret }, [w.S.endpointId], true).status).toBe(`verified`)
    expect((await keys(w.me).mine({ installId: w.O.id, rootIds: [rootId] }))[0]).toMatchObject({ signature: null, signerEndpointId: null })
  })

  it(`a revoked holder's signed wrap is refused`, async () => {
    const w = await world()
    const rootId = await w.root()
    const made = treeKey(rootId, 1, w.S, [w.S], (statement) => signWrap(w.S.identity, statement))
    await keys(w.me).create(made.input)
    const [pair] = (await keys(w.me).pending({ installId: w.S.id, rootIds: [rootId] })).filter((p) => p.targetInstallId === w.R.id)
    await db.update(installs).set({ revokedAt: new Date() }).where(eq(installs.id, w.S.id))
    const wrapped = wrapTreeKey(pair!.targetX25519Public, made.tree.secret)
    const signature = signWrap(w.S.identity, { rootId, epoch: 1, treePublicKey: pair!.publicKey, recipientEndpointId: w.R.endpointId, wrapped })
    await expect(
      keys(w.me).wrap({ installId: w.S.id, wraps: [{ rootId, epoch: 1, targetInstallId: w.R.id, wrapped, signature }] })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
  })

  it(`publicKeys answers a vouch a holder-less install verifies; none for an unsigned key`, async () => {
    const w = await world()
    const signedRoot = await w.root()
    const unsignedRoot = await w.root()
    const made = treeKey(signedRoot, 1, w.S, [w.S], (statement) => signWrap(w.S.identity, statement))
    await keys(w.me).create(made.input)
    await keys(w.me).create(treeKey(unsignedRoot, 1, w.S, [w.S]).input)
    const answer = await keys(w.me).publicKeys({ rootIds: [signedRoot, unsignedRoot] })
    const signed = answer.find((key) => key.rootId === signedRoot)!
    expect(signed.vouch).toEqual({
      recipientEndpointId: w.S.endpointId,
      wrapped: made.input.wraps[0]!.wrapped,
      signature: made.input.wraps[0]!.signature,
      signerEndpointId: w.S.endpointId,
    })
    const statement = { rootId: signedRoot, epoch: signed.epoch, treePublicKey: signed.publicKey, recipientEndpointId: signed.vouch!.recipientEndpointId, wrapped: signed.vouch!.wrapped }
    expect(acceptVouch(statement, signed.vouch!.signature, signed.vouch!.signerEndpointId, [w.S.endpointId])).toBe(`verified`)
    expect(answer.find((key) => key.rootId === unsignedRoot)!.vouch).toBeNull()
    // A signer revoked since: its wraps vouch for nothing.
    await db.update(installs).set({ revokedAt: new Date() }).where(eq(installs.id, w.S.id))
    expect((await keys(w.me).publicKeys({ rootIds: [signedRoot] }))[0]!.vouch).toBeNull()
  })
})

describe.skipIf(!ready.ok)(`#EXP-1950 the inline seals`, () => {
  it(`sessions.send {seal}: a signed wrap is stored, a bad one refuses the send; sendTarget answers the vouch`, async () => {
    const w = await world()
    const sign = (statement: Parameters<typeof signWrap>[1]) => signWrap(w.S.identity, statement)
    // The root id is the send's to mint: a signed seal names it, so a first
    // send to an agent can only sign once it knows the root. An unknown
    // root's signature is refused like any other.
    const bad = treeKey(crypto.randomUUID(), 1, w.S, [w.S], sign)
    await expect(
      sessionsOf(w.me).send({ agentId: w.agentId, text: sealBody(bad.tree.publicKey, 1, `hello`), seal: { ...bad.input, epoch: 1 } } as never)
    ).rejects.toMatchObject(refused)

    // An unsigned first send keys the conversation (every released client).
    const first = treeKey(`00000000-0000-4000-8000-000000000000`, 1, w.S, [w.S])
    const sent = await sessionsOf(w.me).send({ agentId: w.agentId, text: sealBody(first.tree.publicKey, 1, `hello`), seal: { ...first.input, epoch: 1 } } as never)
    expect((await rowsOf(sent.sessionId)).map((row) => row.signature)).toEqual([null])
    expect((await sessionsOf(w.me).sendTarget({ agentId: w.agentId })).vouch).toBeNull()

    // A keyless child-less root of the member's: an inline seal signed for it.
    const rootId = await w.root()
    const signed = treeKey(rootId, 1, w.S, [w.S, w.R], sign)
    await sessionsOf(w.me).send({ sessionId: rootId, text: sealBody(signed.tree.publicKey, 1, `hi`), seal: { ...signed.input, epoch: 1 } } as never)
    expect((await rowsOf(rootId)).every((row) => row.signerEndpointId === w.S.endpointId)).toBe(true)
    const tampered = treeKey(await w.root(), 1, w.S, [w.S], (statement) => sign({ ...statement, epoch: 2 }))
    await expect(
      sessionsOf(w.me).send({ sessionId: tampered.input.rootId, text: sealBody(tampered.tree.publicKey, 1, `hi`), seal: { ...tampered.input, epoch: 1 } } as never)
    ).rejects.toMatchObject(refused)
  })

  it(`voice.callConversation: a signed seal is stored and sendTarget vouches with it; a bad one is refused`, async () => {
    const w = await world()
    const callId = () => mintCallId({ userId: w.me, deviceId: w.deviceId, agentId: w.agentId, sealed: true })!
    // A FIRST call's conversation is minted by the call itself, so its seal
    // cannot name the root: a signature over any other root is refused and
    // nothing is created. A conversation that exists keyless is signed for.
    const probe = treeKey(crypto.randomUUID(), 1, w.S, [w.S], (statement) => signWrap(w.S.identity, statement))
    await expect(
      voice(w.me).callConversation({ agentId: w.agentId, callId: callId(), deviceId: w.deviceId, seal: { ...probe.input, epoch: 1 } })
    ).rejects.toMatchObject(refused)
    expect(await db.select().from(sessions).where(eq(sessions.agentId, w.agentId))).toEqual([])
    const rootId = crypto.randomUUID()
    await db.insert(sessions).values({ id: rootId, rootId, teamId: w.teamId, agentId: w.agentId, userId: w.me, deviceId: w.deviceId, status: `idle`, title: `Helper` } as never)
    const made = treeKey(rootId, 1, w.S, [w.S], (statement) => signWrap(w.S.identity, statement))
    const { rootId: _rootId, ...seal } = made.input
    const out = await voice(w.me).callConversation({ agentId: w.agentId, callId: callId(), deviceId: w.deviceId, seal: { ...seal, epoch: 1 } })
    expect(out).toMatchObject({ rootId, epoch: 1, publicKey: made.tree.publicKey })
    expect((await rowsOf(rootId))[0]).toMatchObject({ signature: made.input.wraps[0]!.signature, signerEndpointId: w.S.endpointId })
    const target = await sessionsOf(w.me).sendTarget({ agentId: w.agentId })
    expect(target.vouch).toMatchObject({ signature: made.input.wraps[0]!.signature, signerEndpointId: w.S.endpointId })
    const statement = { rootId, epoch: 1, treePublicKey: target.publicKey!, recipientEndpointId: target.vouch!.recipientEndpointId, wrapped: target.vouch!.wrapped }
    expect(acceptVouch(statement, target.vouch!.signature, target.vouch!.signerEndpointId, [w.S.endpointId])).toBe(`verified`)
  })
})
