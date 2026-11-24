// @vitest-environment node
//
// #EXP-2123 (fixtures/install-trust.json `keyMaker`): only a JOINED install
// makes a chat key. The REAL web key holder (KeyHolder + InstallTrust +
// sealSendInput / sendSealed) of an install that WAITS for approval, sending
// the first message of a new chat, the first message into a keyless chat and
// racing a due rotation, against an HONEST server double (it refuses a
// waiting install's key writes and names it nothing due) and a MALICIOUS one
// (it accepts every key write and names the waiting install every root as
// due and stranded). Plus the repair: a joined install rotates a STRANDED
// root without the old secret.
import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/install-trust.json" with { type: "json" }
import { KEY_MAKER_REFUSAL, signApproval, type ServedApproval, type ServedInstall } from "@exp/domain-contract/install-trust"
import { parseBodyEnvelope, treeKeyFromSecret, unwrapTreeKey, wrapTreeKey } from "@exp/domain-contract/sealed-content"
import { verifyWrapSignature } from "@exp/domain-contract/wrap-signature"
import { InstallTrust, type TrustApi } from "./install-trust"
import { KeyHolder, type MineRow, type PendingPair, type SessionKeysApi } from "./key-holder"
import { NoKeyError, sealSendInput, sendSealed, type RequestKeyResult, type SealDeps, type SealSessionRow, type TreePublicKey } from "./seal-send"
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
const approval = (kind: `root` | `approve`, subject: Name, approver: Name): ServedApproval => ({
  kind,
  subjectEndpointId: key(subject).endpointId,
  approverEndpointId: key(approver).endpointId,
  signature: signApproval(identity(approver), kind, { userId: key(subject).userId, endpointId: key(subject).endpointId, x25519Public: key(subject).x25519Public }),
})

const ROOT = `01234567-89ab-4def-8123-456789abcdef`
const TREE = treeKeyFromSecret(fromHex(`202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f`))
/** The key a DEVICE made for the keyless root once it was asked. */
const DEVICE_TREE = treeKeyFromSecret(fromHex(`404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f`))

type Create = Parameters<SessionKeysApi[`create`]>[0]
const refusal = () => Object.assign(new Error(KEY_MAKER_REFUSAL), { data: { code: `FORBIDDEN` } })

/**
 * The server double. `honest`: it knows who waits (`waiting`): their key
 * writes are refused with the contract's refusal and `due` answers them
 * nothing. `malicious`: every key write is accepted and `due` names every
 * root it was told (`due`) to anyone, stranded.
 */
function server(kind: `honest` | `malicious`, state: { waiting?: Name[]; due?: Array<{ rootId: string; epoch: number; stranded?: boolean }>; mine?: MineRow[]; pending?: PendingPair[]; chain: ServedApproval[]; installs: ServedInstall[] }) {
  const calls = { create: [] as Create[], refused: [] as Create[], wrap: [] as unknown[], requestKey: [] as string[], sent: [] as unknown[] }
  const waits = (id: string) => (state.waiting ?? []).some((name) => installId(name) === id)
  /** The key of a root a device keyed after `requestKey`. */
  let deviceKey: TreePublicKey | undefined
  const keys: SessionKeysApi = {
    mine: async (input) => (state.mine ?? []).filter((row) => !input.rootIds || input.rootIds.includes(row.rootId)),
    pending: async () => state.pending ?? [],
    wrap: async (input) => {
      calls.wrap.push(...input.wraps)
      return {}
    },
    create: async (input) => {
      if (kind === `honest` && waits(input.installId)) {
        calls.refused.push(input)
        throw refusal()
      }
      calls.create.push(input)
      return {}
    },
    due: async (input) => (kind === `honest` && waits(input.installId) ? [] : (state.due ?? [])),
    resign: async (input) => ({ replaced: input.wraps.length, skipped: 0 }),
  }
  const trust: TrustApi = {
    trust: async () => ({ installs: state.installs, approvals: state.chain }),
    requestApproval: async () => ({}),
    claimRoot: async () => ({}),
    approve: async () => ({}),
    revoke: async () => ({}),
  }
  const requestKey = async (rootId: string): Promise<RequestKeyResult> => {
    calls.requestKey.push(rootId)
    deviceKey = { epoch: 1, publicKey: DEVICE_TREE.publicKey }
    return { state: `requested` }
  }
  /** `sessions.send`: an inline seal is a key write like any other. */
  const send = async (input: { seal?: { installId: string } }) => {
    if (input.seal && kind === `honest` && waits(input.seal.installId)) throw refusal()
    calls.sent.push(input)
    return { sessionId: ROOT }
  }
  return { keys, trust, calls, requestKey, send, publicKey: async () => deviceKey, state, waits }
}
type Server = ReturnType<typeof server>

/** The account: A started the chain, N registered afterwards and asked. */
const account = (extra: Partial<Parameters<typeof server>[1]> = {}) => ({
  installs: [served(`A`), served(`N`, { approvalRequested: true })],
  chain: [approval(`root`, `A`, `A`)],
  ...extra,
})
async function install(name: Name, on: Server) {
  const trust = new InstallTrust(key(name).userId, installId(name), identity(name), { api: on.trust, io: memoryTrustStore() })
  const holder = new KeyHolder(key(name).userId, installId(name), secretOf(name), { api: on.keys, store: memoryTreeKeyStore(), identity: identity(name), trust })
  return { trust, holder }
}

const rootRow = (owner: Name, epoch: number | null, publicKey: string | null): SealSessionRow => ({
  id: ROOT,
  rootId: ROOT,
  parentId: null,
  teamId: `team`,
  userId: key(owner).userId,
  deviceId: `mac-1`,
  shared: false,
  sealEpoch: epoch,
  sealPublicKey: publicKey,
})

function sealDeps(name: Name, holder: KeyHolder, on: Server, root?: SealSessionRow): SealDeps {
  const mine = on.state.installs.filter((row) => row.userId === key(name).userId)
  return {
    holder,
    me: key(name).userId,
    session: (id) => (root && id === root.id ? root : undefined),
    sessions: () => (root ? [root] : []),
    deviceRowIds: () => [],
    myInstalls: async () => mine,
    teamInstalls: async () => [],
    userIdsByEmail: () => [],
    issueIdsByIdentifier: () => [],
    sendTarget: async () => ({ rootId: null, epoch: null, publicKey: null }),
    publicKey: on.publicKey,
    requestKey: on.requestKey,
    waits: { holderMs: 0, keyMs: 50, pollMs: 0 },
    sleep: async () => {},
  }
}

/** A wrap of `tree` for `to` an OLDER build made (unsigned). */
const unsignedWrap = (to: Name, epoch: number, tree = TREE): MineRow => ({ rootId: ROOT, epoch, wrapped: wrapTreeKey(key(to).x25519Public, tree.secret), signature: null, signerEndpointId: null })

for (const kind of [`honest`, `malicious`] as const) {
  describe(`#EXP-2123 a WAITING install against the ${kind} server`, () => {
    it(`the first message of a NEW chat: no key is made, no root, the send fails (never plaintext)`, async () => {
      const on = server(kind, account({ waiting: [`N`] }))
      const { holder, trust } = await install(`N`, on)
      const deps = sealDeps(`N`, holder, on)
      await expect(sendSealed({ teamId: `team`, deviceId: `mac-1`, text: `hello` } as never, deps, on.send as never)).rejects.toBeInstanceOf(NoKeyError)
      expect(trust.status).toBe(`waiting`)
      expect(on.calls.sent).toEqual([])
      expect(on.calls.create).toEqual([])
      expect(on.calls.refused).toEqual([])
    })

    it(`the first message into a KEYLESS chat: nothing is created, its devices are asked, the text is sealed to THEIR key`, async () => {
      const on = server(kind, account({ waiting: [`N`] }))
      const { holder } = await install(`N`, on)
      const root = rootRow(`N`, null, null)
      const sealed = await sealSendInput({ sessionId: ROOT, text: `hello` } as never, sealDeps(`N`, holder, on, root))
      expect(on.calls.create).toEqual([])
      expect(on.calls.refused).toEqual([])
      expect(on.calls.requestKey).toEqual([ROOT])
      expect(sealed.input.seal).toBeUndefined()
      const body = parseBodyEnvelope(sealed.input.text!)
      expect(body.kind).toBe(`sealed`)
      // The device's key opens it; this install holds no secret of the tree.
      expect(holder.keysFor(ROOT).size).toBe(0)
    })

    it(`racing a DUE rotation (it holds the current epoch from before it waited): it never rotates, pass after pass`, async () => {
      const on = server(kind, account({ waiting: [`N`], mine: [unsignedWrap(`N`, 1)], due: [{ rootId: ROOT, epoch: 1 }, { rootId: ROOT, epoch: 1, stranded: true }] }))
      const { holder, trust } = await install(`N`, on)
      await holder.refresh()
      expect(holder.keysFor(ROOT).has(1)).toBe(true)
      await holder.pass()
      await holder.pass()
      expect(trust.status).toBe(`waiting`)
      expect(on.calls.create).toEqual([])
      expect(on.calls.refused).toEqual([])
      expect(await holder.rotateDue()).toBe(0)
      expect([...holder.keysFor(ROOT).keys()]).toEqual([1])
    })

    it(`a store never reconciled (the send beats the first installs.trust): it is reconciled first, nothing is made`, async () => {
      const on = server(kind, account({ waiting: [`N`] }))
      const { holder, trust } = await install(`N`, on)
      expect(trust.status).toBe(`unknown`)
      await expect(sealSendInput({ teamId: `team`, text: `hello` } as never, sealDeps(`N`, holder, on))).rejects.toBeInstanceOf(NoKeyError)
      expect(trust.status).toBe(`waiting`)
      expect(on.calls.create).toEqual([])
    })
  })
}

describe(`#EXP-2123 once approved, and the server's refusal read by an install that takes itself for joined`, () => {
  const approved = () => ({ installs: [served(`A`), served(`N`, { approvalRequested: true })], chain: [approval(`root`, `A`, `A`), approval(`approve`, `N`, `A`)] })

  it(`approved: the same install keys the new chat (its wrap and the approved reader's) and rotates the due root`, async () => {
    const on = server(`honest`, { ...approved(), mine: [unsignedWrap(`N`, 1)], due: [{ rootId: ROOT, epoch: 1 }] })
    const { holder, trust } = await install(`N`, on)
    await trust.sync()
    expect(trust.status).toBe(`trusted`)
    await sendSealed({ teamId: `team`, deviceId: `mac-1`, text: `hello` } as never, sealDeps(`N`, holder, on), on.send as never)
    const sent = on.calls.sent[0] as { seal: { wraps: Array<{ installId: string }> } }
    expect(sent.seal.wraps.map((wrap) => wrap.installId).sort()).toEqual([installId(`A`), installId(`N`)])
    await holder.refresh()
    expect(await holder.rotateDue()).toBe(1)
    expect(on.calls.create.map((c) => [c.rootId, c.epoch])).toEqual([[ROOT, 2]])
  })

  it(`the server still lists it as waiting (its approval is not stored yet): a rotation is refused and NOT remembered; the next pass rotates`, async () => {
    const on = server(`honest`, { ...approved(), waiting: [`N`], mine: [unsignedWrap(`N`, 1)] })
    // The server's `due` lags behind too: it names the root to the install it refuses.
    on.keys.due = async () => [{ rootId: ROOT, epoch: 1 }]
    const { holder, trust } = await install(`N`, on)
    await trust.sync()
    await holder.refresh()
    expect(await holder.rotateDue()).toBe(0)
    expect(on.calls.refused).toHaveLength(1)
    on.state.waiting = []
    expect(await holder.rotateDue()).toBe(1)
    expect(on.calls.create.map((c) => c.epoch)).toEqual([2])
  })

  it(`the same lag on a keyless chat: the refused create asks the devices; on a new chat: the send fails`, async () => {
    const on = server(`honest`, { ...approved(), waiting: [`N`] })
    const { holder, trust } = await install(`N`, on)
    await trust.sync()
    const sealed = await sealSendInput({ sessionId: ROOT, text: `hello` } as never, sealDeps(`N`, holder, on, rootRow(`N`, null, null)))
    expect(on.calls.refused).toHaveLength(1)
    expect(on.calls.requestKey).toEqual([ROOT])
    expect(parseBodyEnvelope(sealed.input.text!).kind).toBe(`sealed`)
    await expect(sendSealed({ teamId: `team`, deviceId: `mac-1`, text: `hello` } as never, sealDeps(`N`, holder, on), on.send as never)).rejects.toBeInstanceOf(NoKeyError)
    expect(on.calls.sent).toEqual([])
  })
})

describe(`#EXP-2123 the repair: a STRANDED root (its current epoch is held by no approved install)`, () => {
  it(`a joined reader rotates it without the old secret: epoch + 1, its own signed wrap, then the readers through pending`, async () => {
    const pair: PendingPair = { rootId: ROOT, epoch: 2, publicKey: ``, targetInstallId: installId(`B`), targetX25519Public: key(`B`).x25519Public, targetEndpointId: key(`B`).endpointId }
    const on = server(`honest`, {
      installs: [served(`A`), served(`B`), served(`N`, { approvalRequested: true })],
      chain: [approval(`root`, `A`, `A`), approval(`approve`, `B`, `A`)],
      waiting: [`N`],
      due: [{ rootId: ROOT, epoch: 1, stranded: true }],
    })
    on.keys.pending = async () => (on.calls.create.length > 0 ? [{ ...pair, publicKey: on.calls.create[0]!.publicKey }] : [])
    const { holder } = await install(`A`, on)
    expect(holder.keysFor(ROOT).size).toBe(0)
    await holder.pass()
    expect(on.calls.create).toHaveLength(1)
    const made = on.calls.create[0]!
    expect([made.rootId, made.epoch, made.wraps.map((wrap) => wrap.installId)]).toEqual([ROOT, 2, [installId(`A`)]])
    const secret = unwrapTreeKey(secretOf(`A`), made.wraps[0]!.wrapped)
    expect(treeKeyFromSecret(secret).publicKey).toBe(made.publicKey)
    expect(verifyWrapSignature({ rootId: ROOT, epoch: 2, treePublicKey: made.publicKey, recipientEndpointId: key(`A`).endpointId, wrapped: made.wraps[0]!.wrapped }, made.wraps[0]!.signature!, key(`A`).endpointId)).toBe(true)
    expect([...holder.keysFor(ROOT).keys()]).toEqual([2])
    // The approved reader got the new epoch; the waiting install is not a target.
    expect((on.calls.wrap as Array<{ targetInstallId: string }>).map((wrap) => wrap.targetInstallId)).toEqual([installId(`B`)])
  })

  it(`a root that is due but NOT stranded and whose secret this install does not hold stays another holder's`, async () => {
    const on = server(`honest`, { installs: [served(`A`)], chain: [approval(`root`, `A`, `A`)], due: [{ rootId: ROOT, epoch: 1, stranded: false }] })
    const { holder } = await install(`A`, on)
    await holder.pass()
    expect(on.calls.create).toEqual([])
  })
})
