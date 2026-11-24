// #EXP-1950 (fixtures/install-trust.json `maliciousServer`): the REAL web
// key holder (KeyHolder + InstallTrust + sealSendInput + the call's key)
// against ONE malicious server double that lies in `sessionKeys.mine`,
// `pending`, `publicKeys`, `sessions.sendTarget` and `installs.list`, one
// block per attack of the threat model, in release 1's RECORD mode (what a
// build does now) and in MANDATORY mode (what it does once unsigned is
// refused). Plus the negative: an honest holder never wraps for an install
// that is not pinned once it follows the rule, and never approves by itself.
import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/install-trust.json" with { type: "json" }
import { signApproval, type ServedApproval, type ServedInstall, type TrustMode } from "@exp/domain-contract/install-trust"
import { b64urlDecode, parseBodyEnvelope, wrapTreeKey } from "@exp/domain-contract/sealed-content"
import { signWrap } from "@exp/domain-contract/wrap-signature"
import { InstallTrust, type TrustApi } from "./install-trust"
import { KeyHolder, type MineRow, type PendingPair, type SessionKeysApi } from "./key-holder"
import { NoKeyError, sealSendInput, type InstallRow, type SealDeps, type SealSessionRow, type ServerTarget, type TreePublicKey } from "./seal-send"
import { memoryTreeKeyStore } from "./tree-key-store"
import { memoryTrustStore } from "./trust-store"

const fromHex = (text: string) => new Uint8Array((text.match(/../g) ?? []).map((b) => parseInt(b, 16)))
type Name = keyof typeof fixture.keys
const key = (name: Name) => fixture.keys[name]
const identity = (name: Name) => fromHex(key(name).identity)
const secretOf = (name: Name) => fromHex(key(name).installSecret)
const installId = (name: Name) => `install-${name}`
const served = (name: Name, patch: Partial<ServedInstall> = {}): ServedInstall => ({
  installId: installId(name),
  userId: key(name).userId,
  endpointId: key(name).endpointId,
  x25519Public: key(name).x25519Public,
  registrationSignature: key(name).registrationSignature,
  registrationSignedAt: key(name).signedAt,
  approvalRequested: false,
  label: key(name).label,
  ...patch,
})
const approval = (kind: ServedApproval[`kind`], subject: Name, approver: Name): ServedApproval => ({
  kind,
  subjectEndpointId: key(subject).endpointId,
  approverEndpointId: key(approver).endpointId,
  signature: signApproval(identity(approver), kind, served(subject)),
})

const ROOT1 = `01234567-89ab-4def-8123-456789abcdef`
const ROOT3 = `31234567-89ab-4def-8123-456789abcdef`
const REAL = { secret: fromHex(`202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f`), publicKey: `NYBy1jZYgNGu6jKa35EhODhR7SGijjt16WXQ0s0WYlQ` }
const FAKE = { secret: fromHex(`606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f`), publicKey: `Z13VdO13iTELPS52gfN5C0ZsdzsVIf7PNld5WDcepS8` }

/** A wrap of `secret` for `to`, signed by `by` (or with the signature of `sigBy`). */
function wrapOf(rootId: string, epoch: number, tree: { secret: Uint8Array; publicKey: string }, to: Name, by: Name | null, opts: { claim?: Name; sigOver?: string } = {}): MineRow {
  const wrapped = wrapTreeKey(key(to).x25519Public, tree.secret)
  if (!by) return { rootId, epoch, wrapped, signature: null, signerEndpointId: null }
  const signature = signWrap(identity(by), {
    rootId,
    epoch,
    treePublicKey: tree.publicKey,
    recipientEndpointId: key(to).endpointId,
    wrapped: opts.sigOver ?? wrapped,
  })
  return { rootId, epoch, wrapped, signature, signerEndpointId: key(opts.claim ?? by).endpointId }
}

/** THE malicious server double: one object, every answer a lie it is told to tell. */
function maliciousServer(lies: {
  mine?: MineRow[]
  pending?: PendingPair[]
  publicKeys?: TreePublicKey
  sendTarget?: ServerTarget
  installsList?: InstallRow[]
  trust?: { installs: ServedInstall[]; approvals: ServedApproval[] }
}) {
  const calls = { wrap: [] as Array<{ targetInstallId: string; signature?: string }>, create: [] as unknown[], approve: [] as unknown[], revoke: [] as unknown[], resign: [] as unknown[] }
  // The victim's account as it really is: A the root, B approved by A, team-mate T1.
  const honestTrust = {
    installs: [served(`A`), served(`B`), served(`T1`)],
    approvals: [approval(`root`, `A`, `A`), approval(`approve`, `B`, `A`), approval(`root`, `T1`, `T1`)],
  }
  const keys: SessionKeysApi = {
    mine: async (input) => (lies.mine ?? []).filter((row) => !input.rootIds || input.rootIds.includes(row.rootId)),
    pending: async () => lies.pending ?? [],
    wrap: async (input) => {
      calls.wrap.push(...input.wraps)
      return {}
    },
    create: async (input) => {
      calls.create.push(input)
      return {}
    },
    due: async () => [],
    resign: async (input) => {
      calls.resign.push(input)
      return { replaced: input.wraps.length, skipped: 0 }
    },
  }
  const trust: TrustApi = {
    trust: async () => lies.trust ?? honestTrust,
    requestApproval: async () => ({}),
    claimRoot: async () => ({}),
    approve: async (input) => {
      calls.approve.push(input)
      return {}
    },
    revoke: async (input) => {
      calls.revoke.push(input)
      return {}
    },
  }
  return { keys, trust, calls, lies }
}

type Server = ReturnType<typeof maliciousServer>

/** The victim: install A of the web, its real KeyHolder and InstallTrust. */
async function victim(server: Server, mode: TrustMode = `record`) {
  const trust = new InstallTrust(key(`A`).userId, installId(`A`), identity(`A`), { api: server.trust, io: memoryTrustStore(), mode })
  const holder = new KeyHolder(key(`A`).userId, installId(`A`), secretOf(`A`), { api: server.keys, store: memoryTreeKeyStore(), identity: identity(`A`), trust })
  await trust.sync()
  expect(trust.status).toBe(`trusted`)
  return { holder, trust }
}

/** Seal deps over the same double: a send to an existing root `root`. */
function sealDeps(holder: KeyHolder, server: Server, root: SealSessionRow): SealDeps {
  return {
    holder,
    me: key(`A`).userId,
    session: (id) => (id === root.id ? root : undefined),
    sessions: () => [root],
    deviceRowIds: () => [],
    myInstalls: async () => server.lies.installsList ?? [],
    teamInstalls: async () => server.lies.installsList ?? [],
    userIdsByEmail: () => [],
    issueIdsByIdentifier: () => [],
    sendTarget: async () => server.lies.sendTarget ?? { rootId: null, epoch: null, publicKey: null },
    publicKey: async () => server.lies.publicKeys,
    waits: { holderMs: 0, keyMs: 0, pollMs: 0 },
    sleep: async () => {},
  }
}
const rootRow = (id: string, epoch: number | null, publicKey: string | null): SealSessionRow => ({
  id,
  rootId: id,
  parentId: null,
  teamId: `team`,
  userId: key(`A`).userId,
  deviceId: null,
  shared: false,
  sealEpoch: epoch,
  sealPublicKey: publicKey,
})
const sealedTo = (text: string) => parseBodyEnvelope(text)

describe(`#EXP-1950 attack 1: a false epoch (sessionKeys.mine plants a key the server made)`, () => {
  const honest = wrapOf(ROOT1, 1, REAL, `A`, `B`)
  const planted = {
    forged: wrapOf(ROOT1, 2, FAKE, `A`, `Z`, { claim: `B` }),
    lifted: { ...wrapOf(ROOT1, 2, FAKE, `A`, null), signature: honest.signature, signerEndpointId: honest.signerEndpointId },
    unsigned: wrapOf(ROOT1, 2, FAKE, `A`, null),
    serverKey: wrapOf(ROOT1, 2, FAKE, `A`, `Z`),
  }

  for (const mode of [`record`, `mandatory`] as const) {
    it(`a signature naming a pinned signer it never made, or lifted from another wrap: never kept (${mode})`, async () => {
      for (const lie of [planted.forged, planted.lifted]) {
        const server = maliciousServer({ mine: [honest, lie] })
        const { holder, trust } = await victim(server, mode)
        await holder.refresh()
        expect(holder.keysFor(ROOT1).has(1)).toBe(true)
        expect(holder.keysFor(ROOT1).has(2)).toBe(false)
        expect(trust.store.roots[ROOT1]).toEqual({ verifiedEpoch: 1, epochs: { "1": `verified` } })
      }
    })
  }

  it(`unsigned or signed by the server's own install: kept and RECORDED now, refused once mandatory`, async () => {
    for (const [lie, record] of [
      [planted.unsigned, `unsigned`],
      [planted.serverKey, `refused-would-be`],
    ] as const) {
      const now = await victim(maliciousServer({ mine: [honest, lie] }), `record`)
      await now.holder.refresh()
      expect(now.holder.keysFor(ROOT1).has(2)).toBe(true) // the record-only gap
      expect(now.trust.store.roots[ROOT1]!.epochs[`2`]).toBe(record)
      expect(now.trust.store.roots[ROOT1]!.verifiedEpoch).toBe(1)
      const later = await victim(maliciousServer({ mine: [honest, lie] }), `mandatory`)
      await later.holder.refresh()
      expect(later.holder.keysFor(ROOT1).has(2)).toBe(false)
    }
  })

  it(`R2: a send never seals below the verified epoch the server rolls back to`, async () => {
    const v2 = wrapOf(ROOT1, 2, FAKE, `A`, `B`) // B really rotated to epoch 2
    const server = maliciousServer({ mine: [honest, v2], publicKeys: { epoch: 1, publicKey: REAL.publicKey } })
    const { holder } = await victim(server)
    await holder.refresh()
    // The synced row and publicKeys both say epoch 1.
    const deps = sealDeps(holder, server, rootRow(ROOT1, 1, REAL.publicKey))
    await expect(sealSendInput({ sessionId: ROOT1, text: `hi` } as never, deps, { fresh: true })).rejects.toBeInstanceOf(NoKeyError)
  })

  it(`the planted key is sealed to only while its record says what it is (now), never once mandatory`, async () => {
    const server = maliciousServer({ mine: [honest, planted.unsigned], publicKeys: { epoch: 2, publicKey: FAKE.publicKey } })
    const now = await victim(server, `record`)
    await now.holder.refresh()
    const out = await sealSendInput({ sessionId: ROOT1, text: `hi` } as never, sealDeps(now.holder, server, rootRow(ROOT1, 2, FAKE.publicKey)))
    expect(sealedTo(out.input.text!)).toMatchObject({ epoch: 2 })
    const later = await victim(server, `mandatory`)
    await later.holder.refresh()
    await expect(sealSendInput({ sessionId: ROOT1, text: `hi` } as never, sealDeps(later.holder, server, rootRow(ROOT1, 2, FAKE.publicKey)))).rejects.toBeInstanceOf(NoKeyError)
  })
})

describe(`#EXP-1950 attack 2: a false public key before any wrap (sessionKeys.publicKeys / sendTarget)`, () => {
  const vouchOf = (signer: Name, tree: { secret: Uint8Array; publicKey: string }, recipient: Name, claim?: Name) => {
    const wrap = wrapOf(ROOT3, 1, tree, recipient, signer, { claim })
    return { recipientEndpointId: key(recipient).endpointId, wrapped: wrap.wrapped, signature: wrap.signature!, signerEndpointId: wrap.signerEndpointId! }
  }
  const send = (holder: KeyHolder, server: Server) =>
    sealSendInput({ sessionId: ROOT3, text: `hi` } as never, sealDeps(holder, server, rootRow(ROOT3, 1, server.lies.publicKeys!.publicKey)), { fresh: true })

  it(`a vouch naming a pinned signer it never made, or lifted from the real key: nothing is sealed, in every mode`, async () => {
    for (const mode of [`record`, `mandatory`] as const) {
      for (const vouch of [vouchOf(`Z`, FAKE, `Z`, `B`), vouchOf(`B`, REAL, `T1`)]) {
        const server = maliciousServer({ publicKeys: { epoch: 1, publicKey: FAKE.publicKey, vouch } })
        const { holder } = await victim(server, mode)
        await expect(send(holder, server)).rejects.toBeInstanceOf(NoKeyError)
      }
    }
  })

  it(`no vouch or the server's own: sealed and recorded now (the gap), refused once mandatory; a pinned vouch seals`, async () => {
    for (const vouch of [null, vouchOf(`Z`, FAKE, `Z`)]) {
      const server = maliciousServer({ publicKeys: { epoch: 1, publicKey: FAKE.publicKey, vouch } })
      const now = await victim(server, `record`)
      await expect(send(now.holder, server)).resolves.toBeDefined()
      expect(now.trust.decisions.at(-1)).toMatchObject({ rule: `R3`, verdict: vouch ? `refused-would-be` : `unsigned` })
      const later = await victim(server, `mandatory`)
      await expect(send(later.holder, server)).rejects.toBeInstanceOf(NoKeyError)
    }
    const honest = maliciousServer({ publicKeys: { epoch: 1, publicKey: REAL.publicKey, vouch: vouchOf(`B`, REAL, `T1`) } })
    const { holder } = await victim(honest, `mandatory`)
    await expect(send(holder, honest)).resolves.toBeDefined()
  })
})

describe(`#EXP-1950 attack 3: a false reader (pending / installs.list name the server's install Z)`, () => {
  const pair = (name: Name, patch: Partial<PendingPair> = {}): PendingPair => ({
    rootId: ROOT1,
    epoch: 1,
    publicKey: REAL.publicKey,
    targetInstallId: installId(name),
    targetX25519Public: key(name).x25519Public,
    targetEndpointId: key(name).endpointId,
    ...patch,
  })
  const trustWith = (...extra: ServedInstall[]) => ({
    installs: [served(`A`), served(`B`), served(`T1`), ...extra],
    approvals: [approval(`root`, `A`, `A`), approval(`approve`, `B`, `A`), approval(`root`, `T1`, `T1`)],
  })

  it(`Z asked to be approved (or claims a root): no wrap for it, in every mode; the honest pairs beside it are signed`, async () => {
    for (const mode of [`record`, `mandatory`] as const) {
      for (const [z, approvals] of [
        [served(`Z`, { approvalRequested: true }), []],
        [served(`Z`), [approval(`root`, `Z`, `Z`)]],
      ] as const) {
        const lies = trustWith(z)
        const server = maliciousServer({ mine: [wrapOf(ROOT1, 1, REAL, `A`, `B`)], pending: [pair(`Z`), pair(`B`)], trust: { ...lies, approvals: [...lies.approvals, ...approvals] } })
        const { holder } = await victim(server, mode)
        await holder.refresh()
        await holder.answerPending()
        expect(server.calls.wrap.map((wrap) => wrap.targetInstallId)).toEqual([installId(`B`)])
        expect(server.calls.wrap[0]!.signature).toMatch(/^[A-Za-z0-9_-]{86}$/)
      }
    }
  })

  it(`a pinned endpoint served with Z's X25519 key: no wrap, in every mode`, async () => {
    for (const mode of [`record`, `mandatory`] as const) {
      const server = maliciousServer({ mine: [wrapOf(ROOT1, 1, REAL, `A`, `B`)], pending: [pair(`B`, { targetX25519Public: key(`Z`).x25519Public })] })
      const { holder } = await victim(server, mode)
      await holder.refresh()
      await holder.answerPending()
      expect(server.calls.wrap).toEqual([])
    }
  })

  it(`a silent Z looks like an older build: wrapped and recorded legacy now (the gap), refused once mandatory`, async () => {
    const now = maliciousServer({ mine: [wrapOf(ROOT1, 1, REAL, `A`, `B`)], pending: [pair(`Z`)], trust: trustWith(served(`Z`)) })
    const a = await victim(now, `record`)
    await a.holder.refresh()
    await a.holder.answerPending()
    expect(now.calls.wrap.map((wrap) => wrap.targetInstallId)).toEqual([installId(`Z`)])
    expect(a.trust.decisions.find((d) => d.rule === `R4`)).toMatchObject({ subject: installId(`Z`), verdict: `legacy` })
    const later = maliciousServer({ mine: [wrapOf(ROOT1, 1, REAL, `A`, `B`)], pending: [pair(`Z`)], trust: trustWith(served(`Z`)) })
    const b = await victim(later, `mandatory`)
    await b.holder.refresh()
    await b.holder.answerPending()
    expect(later.calls.wrap).toEqual([])
  })

  it(`installs.list names Z for a new root's seal: Z gets no wrap; the seal is signed right after the send`, async () => {
    const server = maliciousServer({
      trust: trustWith(served(`Z`, { approvalRequested: true })),
      installsList: [
        { installId: installId(`A`), x25519Public: key(`A`).x25519Public, endpointId: key(`A`).endpointId, userId: key(`A`).userId },
        { installId: installId(`B`), x25519Public: key(`B`).x25519Public, endpointId: key(`B`).endpointId, userId: key(`B`).userId },
        { installId: installId(`Z`), x25519Public: key(`Z`).x25519Public, endpointId: key(`Z`).endpointId, userId: key(`Z`).userId },
      ] as InstallRow[],
    })
    const { holder, trust } = await victim(server)
    const out = await sealSendInput({ teamId: `team`, text: `hi` } as never, sealDeps(holder, server, rootRow(`unused`, null, null)))
    expect(out.input.seal!.wraps.map((wrap) => wrap.installId).sort()).toEqual([installId(`A`), installId(`B`)])
    // newRoot: unsigned in the send (the root id is unknown), resigned once it is.
    expect(out.input.seal!.wraps.every((wrap) => !(`signature` in wrap) || !wrap.signature)).toBe(true)
    const newRoot = `41234567-89ab-4def-8123-456789abcdef`
    await out.after({ sessionId: newRoot })
    const [resign] = server.calls.resign as Array<{ wraps: Array<{ rootId: string; installId: string; signature: string }> }>
    expect(resign!.wraps.map((wrap) => [wrap.rootId, wrap.installId]).sort()).toEqual([
      [newRoot, installId(`A`)],
      [newRoot, installId(`B`)],
    ])
    expect(trust.store.roots[newRoot]).toEqual({ verifiedEpoch: 1, epochs: { "1": `verified` } })
  })
})

describe(`#EXP-1950 attack 4: a false call key (sendTarget names a planted root and epoch)`, () => {
  it(`the call reads its key from the verified keyring only: nothing held = no_key; a planted key only while record-only`, async () => {
    const honest = wrapOf(ROOT1, 1, REAL, `A`, `B`)
    for (const [lie, now] of [
      [wrapOf(ROOT1, 2, FAKE, `A`, `Z`, { claim: `B` }), false], // refused: no key at all
      [wrapOf(ROOT1, 2, FAKE, `A`, null), true], // unsigned: the record-only gap
      [wrapOf(ROOT1, 2, FAKE, `A`, `Z`), true], // the server's own install
    ] as const) {
      const a = await victim(maliciousServer({ mine: [honest, lie], sendTarget: { rootId: ROOT1, epoch: 2, publicKey: FAKE.publicKey } }), `record`)
      expect((await a.holder.callSecret(ROOT1, 2)) !== null).toBe(now)
      const b = await victim(maliciousServer({ mine: [honest, lie], sendTarget: { rootId: ROOT1, epoch: 2, publicKey: FAKE.publicKey } }), `mandatory`)
      expect(await b.holder.callSecret(ROOT1, 2)).toBeNull()
    }
    // The honest key.
    const c = await victim(maliciousServer({ mine: [honest] }), `mandatory`)
    expect(await c.holder.callSecret(ROOT1, 1)).toEqual(REAL.secret)
  })
})

describe(`#EXP-1950 the negative: an honest holder`, () => {
  it(`never wraps for an install that is not pinned once it follows the rule (an own install that asked, a team-mate's unvouched install)`, async () => {
    const t2 = served(`T2`, { approvalRequested: true })
    const n = served(`N`, { approvalRequested: true })
    const server = maliciousServer({
      mine: [wrapOf(ROOT1, 1, REAL, `A`, `B`)],
      trust: {
        installs: [served(`A`), served(`B`), served(`T1`), t2, n],
        approvals: [approval(`root`, `A`, `A`), approval(`approve`, `B`, `A`), approval(`root`, `T1`, `T1`)],
      },
      pending: [`T2`, `N`, `T1`].map((name) => ({
        rootId: ROOT1,
        epoch: 1,
        publicKey: REAL.publicKey,
        targetInstallId: installId(name as Name),
        targetX25519Public: key(name as Name).x25519Public,
        targetEndpointId: key(name as Name).endpointId,
      })),
    })
    const { holder, trust } = await victim(server)
    await holder.refresh()
    await holder.pass()
    await holder.pass()
    expect(server.calls.wrap.map((wrap) => wrap.targetInstallId)).toEqual([installId(`T1`), installId(`T1`)])
    // The asking install is a row a PERSON approves; the holder never does by itself.
    expect(trust.rows.map((row) => [row.kind, row.installId])).toEqual([[`newInstall`, installId(`N`)]])
    expect(server.calls.approve).toEqual([])
    expect(server.calls.revoke).toEqual([])
    // Once approved (by a person, here B's signature), N is wrapped for.
    const approved = maliciousServer({
      mine: [wrapOf(ROOT1, 1, REAL, `A`, `B`)],
      trust: { installs: [served(`A`), served(`B`), n], approvals: [approval(`root`, `A`, `A`), approval(`approve`, `B`, `A`), approval(`approve`, `N`, `B`)] },
      pending: [{ rootId: ROOT1, epoch: 1, publicKey: REAL.publicKey, targetInstallId: installId(`N`), targetX25519Public: key(`N`).x25519Public, targetEndpointId: key(`N`).endpointId }],
    })
    const again = await victim(approved)
    await again.holder.refresh()
    await again.holder.pass()
    expect(approved.calls.wrap.map((wrap) => wrap.targetInstallId)).toEqual([installId(`N`)])
    expect(approved.calls.approve).toEqual([])
  })

  it(`a waiting install wraps for nobody, shows its code, and asks once`, async () => {
    const requests: unknown[] = []
    const server = maliciousServer({
      trust: { installs: [served(`A`), served(`N`)], approvals: [approval(`root`, `A`, `A`)] },
      pending: [{ rootId: ROOT1, epoch: 1, publicKey: REAL.publicKey, targetInstallId: installId(`A`), targetX25519Public: key(`A`).x25519Public, targetEndpointId: key(`A`).endpointId }],
      mine: [wrapOf(ROOT1, 1, REAL, `N`, null)],
    })
    server.trust.requestApproval = async (input) => {
      requests.push(input)
      return {}
    }
    const trust = new InstallTrust(key(`N`).userId, installId(`N`), identity(`N`), { api: server.trust, io: memoryTrustStore() })
    const holder = new KeyHolder(key(`N`).userId, installId(`N`), secretOf(`N`), { api: server.keys, store: memoryTreeKeyStore(), identity: identity(`N`), trust })
    await holder.refresh()
    await holder.pass()
    await holder.pass()
    expect(trust.status).toBe(`waiting`)
    expect(trust.rows).toEqual([])
    expect(server.calls.wrap).toEqual([])
    expect(requests).toEqual([{ installId: installId(`N`) }])
    // What an unapproved install can still read: wraps OLD holders made for it (unsigned), recorded.
    expect(holder.keysFor(ROOT1).has(1)).toBe(true)
    expect(trust.store.roots[ROOT1]!.epochs[`1`]).toBe(`unsigned`)
  })

  it(`the pending wraps, a rotation and a first key are signed by this install over exactly what is stored`, async () => {
    const created: Array<{ wraps: Array<{ installId: string; wrapped: string; signature?: string }>; publicKey: string; rootId: string; epoch: number }> = []
    const server = maliciousServer({})
    server.keys.create = async (input) => {
      created.push(input as never)
      return {}
    }
    const { holder } = await victim(server)
    await holder.createTree(ROOT3, [
      { installId: installId(`A`), x25519Public: key(`A`).x25519Public, endpointId: key(`A`).endpointId },
      { installId: installId(`B`), x25519Public: key(`B`).x25519Public, endpointId: key(`B`).endpointId },
    ])
    const { acceptWrap } = await import(`@exp/domain-contract/wrap-signature`)
    for (const wrap of created[0]!.wraps) {
      const who: Name = wrap.installId === installId(`A`) ? `A` : `B`
      const verdict = acceptWrap({ rootId: ROOT3, epoch: 1, wrapped: wrap.wrapped, signature: wrap.signature, signerEndpointId: key(`A`).endpointId }, { identity: identity(who), installSecret: secretOf(who) }, [key(`A`).endpointId], true)
      expect(verdict.status).toBe(`verified`)
    }
    expect(b64urlDecode(created[0]!.wraps[0]!.signature!)?.length).toBe(64)
  })
})
