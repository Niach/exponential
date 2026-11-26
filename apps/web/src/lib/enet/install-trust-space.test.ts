// #EXP-2161 (install-trust.json `space`, member-space.json): the REAL web
// InstallTrust + KeyHolder over a member space's install: it asks for spaces,
// Approve signs `space`, the pin covers the own Assistant conversation alone.
import type { RootMark, RootSight } from "@exp/domain-contract/member-space"
import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/install-trust.json" with { type: "json" }
import { MEMBER_SPACE, pinCovers } from "@exp/domain-contract/member-space"
import { emptyTrustStore, signApproval, type ApprovalKind, type PinVia, type ServedApproval, type ServedInstall, type TrustMode, type TrustStore } from "@exp/domain-contract/install-trust"
import { b64urlEncode, pushPublicKey } from "@exp/domain-contract/sealed-content"
import { InstallTrust, type TrustApi } from "./install-trust"
import { KeyHolder, type PendingPair, type SessionKeysApi } from "./key-holder"
import { sealSendInput, type SealDeps } from "./seal-send"
import { isOwnAssistantRoot } from "./own-assistant"
import { memoryTreeKeyStore } from "./tree-key-store"
import { memoryTrustStore } from "./trust-store"

vi.mock(`@/lib/collections`, () => ({ agentCollection: new Map(), sessionCollection: new Map() }))

const space = fixture.space
type Key = { identity: string; userId: string; endpointId: string; installSecret: string; x25519Public: string; signedAt: string; registrationSignature: string; label: string }
const keys = { ...fixture.keys, ...space.keys } as Record<string, Key>
const nameOf = (endpointId: string) => Object.keys(keys).find((name) => keys[name]!.endpointId === endpointId)!
const bytes = (text: string) => new Uint8Array((text.match(/../g) ?? []).map((b) => parseInt(b, 16)))
const ROOT = `01234567-89ab-4def-8123-456789abcdef`

function served(spec: { key: string; approvalRequested?: boolean; registration?: string }): ServedInstall {
  const key = keys[spec.key]!
  const none = spec.registration === `none`
  return {
    installId: `install-${spec.key}`,
    userId: key.userId,
    endpointId: key.endpointId,
    x25519Public: key.x25519Public,
    registrationSignature: none ? null : key.registrationSignature,
    registrationSignedAt: none ? null : key.signedAt,
    approvalRequested: !!spec.approvalRequested,
    label: key.label,
  }
}

type StoreSpec = { viewer: string; space?: boolean; joined: boolean; existing?: boolean; pins: Record<string, unknown>; members?: string[]; knownSpaces?: string[] }
function build(spec: StoreSpec): TrustStore {
  const viewer = keys[spec.viewer]!
  const store = emptyTrustStore(viewer.endpointId, viewer.userId, spec.existing ?? false)
  store.joined = spec.joined
  store.members = spec.members ?? []
  if (spec.knownSpaces) store.knownSpaces = spec.knownSpaces.map((name) => keys[name]!.endpointId)
  for (const [name, pin] of Object.entries(spec.pins)) {
    const [via, by, kind] = pin as [PinVia, string | null, string | undefined]
    const { userId, x25519Public, endpointId } = keys[name]!
    store.pins[endpointId] = { userId, x25519Public, via, by: by ? keys[by]!.endpointId : null, ...(kind === `space` ? { class: `space` as const } : {}) }
  }
  return store
}

async function installTrust(
  answer: { installs: ServedInstall[]; approvals: ServedApproval[] },
  opts: { store?: TrustStore; mode?: TrustMode; ownAssistant?: (rootId: string) => boolean; see?: (rootId: string) => RootSight } = {}
) {
  const calls = { trust: [] as unknown[], approve: [] as { kind: string; subjectInstallId: string; signature: string }[], revoke: [] as unknown[] }
  const api: TrustApi = {
    trust: async (input) => (calls.trust.push(input), answer),
    requestApproval: async () => ({}),
    claimRoot: async () => ({}),
    approve: async (input) => (calls.approve.push(input), {}),
    revoke: async (input) => (calls.revoke.push(input), {}),
  }
  const io = memoryTrustStore()
  if (opts.store) await io.put(keys.A!.userId, opts.store)
  const trust = new InstallTrust(keys.A!.userId, `install-A`, bytes(keys.A!.identity), { api, io, mode: opts.mode, see: opts.see ?? ((id) => (opts.ownAssistant ? (opts.ownAssistant(id) ? `assistant` : `other`) : `unknown`)) })
  await trust.load()
  return { trust, calls, io }
}

const approval = (kind: ApprovalKind, subject: string, approver: string): ServedApproval => ({
  kind,
  subjectEndpointId: keys[subject]!.endpointId,
  approverEndpointId: keys[approver]!.endpointId,
  signature: signApproval(bytes(keys[approver]!.identity), kind, served({ key: subject })),
})
const ROOT_A = approval(`root`, `A`, `A`)

describe(`install-trust.json space on the web`, () => {
  it(`trustCases of this install: the same pins and rows`, async () => {
    for (const c of space.trustCases) {
      const before = c.before as unknown as StoreSpec
      // The space's own reconcile is its daemon's.
      if (before.space || before.viewer !== `A`) continue
      const { trust, calls } = await installTrust(
        {
          installs: (c.installs as { key: string }[]).map(served),
          approvals: c.approvals.map((a) => ({
            kind: a.kind as ApprovalKind,
            subjectEndpointId: keys[a.subject]!.endpointId,
            approverEndpointId: keys[a.approver]!.endpointId,
            signature: a.signature,
          })),
        },
        { store: build(before) }
      )
      await trust.sync()
      expect(calls.trust[0], c.name).toEqual({ installId: `install-A`, spaces: true })
      expect(trust.status, c.name).toBe(c.status)
      expect(
        Object.fromEntries(Object.entries(trust.store.pins).map(([id, pin]) => [nameOf(id), [pin.via, pin.by ? nameOf(pin.by) : null, ...(pin.class ? [pin.class] : [])]])),
        c.name
      ).toEqual(c.after.pins)
      expect(trust.rows.map((row) => ({ kind: row.kind, key: nameOf(row.endpointId) })), c.name).toEqual(c.rows)
      // Never by itself, and never an `approve` about a space.
      expect(calls.approve.some((sent) => sent.subjectInstallId === `install-S`), c.name).toBe(false)
    }
  })

  it(`Approve signs the fixture's \`space\` approval; Remove revokes`, async () => {
    const { trust, calls } = await installTrust({ installs: [served({ key: `A` }), served({ key: `S` })], approvals: [ROOT_A] }, { store: build(space.stores.withoutSpace as unknown as StoreSpec) })
    await trust.sync()
    const row = trust.rows.find((candidate) => candidate.installId === `install-S`)!
    expect(row).toMatchObject({ kind: `newInstall`, label: space.keys.S.label, code: space.keys.S.code })
    await trust.approve(row)
    expect(calls.approve).toEqual([{ installId: `install-A`, subjectInstallId: `install-S`, kind: MEMBER_SPACE.approvalKind, signature: space.approval.signature }])
    await trust.remove(row)
    expect(calls.revoke).toEqual([{ installId: `install-S` }])
  })

  it(`rootMarkCases through the pin store, kept over a reload`, async () => {
    for (const c of MEMBER_SPACE.rootMarkCases) {
      let sight: RootSight = `unknown`
      const spec = space.stores.withSpace as unknown as StoreSpec
      const store = build(spec)
      if (c.mark) store.rootMarks = { r: c.mark as RootMark }
      if ((c as { keyed?: boolean }).keyed) store.roots = { r: { verifiedEpoch: 1, epochs: { 1: `verified` } } }
      let { trust, io } = await installTrust({ installs: [], approvals: [] }, { store, see: () => sight })
      let epoch = 1
      for (const step of c.steps as Array<{ see: RootSight; record?: boolean; counts: boolean; mark: RootMark | null }>) {
        sight = step.see
        // A record judges first, as `keep` does.
        const counts = trust.ownAssistant(`r`)
        if (step.record) trust.record(`r`, ++epoch, `verified`)
        expect(counts, c.name).toBe(step.counts)
        expect(trust.store.rootMarks?.r ?? null, c.name).toBe(step.mark)
        await new Promise((resolve) => setTimeout(resolve, 0))
        const again = new InstallTrust(keys.A!.userId, `install-A`, bytes(keys.A!.identity), { api: {} as TrustApi, io, see: () => sight })
        await again.load()
        trust = again
      }
    }
  })

  it(`a root that never counts: not the space's wrap, vouch or target, whatever the rows say now, nor by addressing the Assistant`, async () => {
    const c = space.sealCases.find((entry) => entry.rootIsOwnAssistant && entry.store === `withSpace`)!
    let sight: RootSight = `other`
    const { trust } = await installTrust({ installs: [], approvals: [] }, { store: build(space.stores.withSpace as unknown as StoreSpec), mode: `record`, see: () => sight })
    trust.ownAssistant(c.rootId)
    sight = `assistant`
    expect(trust.seal({ ...c, held: null })).toMatchObject({ seal: false })
    expect(trust.seal({ ...c, held: null }, true)).toMatchObject({ seal: false })
    const wrap = space.keepCases.find((entry) => entry.rootIsOwnAssistant)!.wrap
    trust.ownAssistant(wrap.rootId)
    expect(trust.keep(wrap, bytes(keys.A!.installSecret))).toMatchObject({ keep: false })
  })

  it(`a new root this install sealed itself: recorded \`verified\` at adoption, so it counts once its row arrives, over a reload too`, async () => {
    let sight: RootSight = `unknown`
    const { trust, io } = await installTrust({ installs: [], approvals: [] }, { store: build(space.stores.withSpace as unknown as StoreSpec), see: () => sight })
    const store = memoryTreeKeyStore()
    const api = { mine: async () => [], pending: async () => [], wrap: async () => ({ written: 0, skipped: 0 }), create: async () => ({}) } as unknown as SessionKeysApi
    const secret = bytes(keys.A!.installSecret)
    const holder = new KeyHolder(keys.A!.userId, `install-A`, secret, { api, store, trust })
    const created = holder.newTreeKey([{ installId: `install-A`, userId: keys.A!.userId, x25519Public: keys.A!.x25519Public, endpointId: keys.A!.endpointId }], true)
    await holder.adoptNewRoot(`root-new`, created)
    expect(trust.recordOf(`root-new`, created.epoch)).toBe(`verified`)
    expect(trust.store.rootMarks?.[`root-new`]).toBe(`unseen`)
    // The next pass, and a reload, never take it for a key held before.
    await holder.loadCache()
    await new Promise((resolve) => setTimeout(resolve, 0))
    const again = new InstallTrust(keys.A!.userId, `install-A`, bytes(keys.A!.identity), { api: {} as TrustApi, io, see: () => sight })
    await again.load()
    await new KeyHolder(keys.A!.userId, `install-A`, secret, { api, store, trust: again }).loadCache()
    expect(again.recordOf(`root-new`, created.epoch)).toBe(`verified`)
    sight = `assistant`
    expect(again.ownAssistant(`root-new`)).toBe(true)
  })

  it(`keepCases, sealCases, targetCases through the own-Assistant lookup`, async () => {
    for (const mode of [`record`, `mandatory`] as const) {
      const at = (c: { now: unknown; mandatory: unknown }) => (mode === `record` ? c.now : c.mandatory) as Record<string, unknown>
      for (const c of space.keepCases) {
        const spec = space.stores[c.store as keyof typeof space.stores] as unknown as StoreSpec
        if (spec.viewer !== `A`) continue
        const { trust } = await installTrust({ installs: [], approvals: [] }, { store: build(spec), mode, ownAssistant: (id) => id === c.wrap.rootId && c.rootIsOwnAssistant })
        expect(trust.keep(c.wrap, bytes(keys.A!.installSecret)), `${c.name} (${mode})`).toMatchObject(at(c))
      }
      for (const c of space.sealCases) {
        const { trust } = await installTrust({ installs: [], approvals: [] }, { store: build(space.stores[c.store as keyof typeof space.stores] as unknown as StoreSpec), mode, ownAssistant: () => c.rootIsOwnAssistant })
        expect(trust.seal({ ...c, held: null }), `${c.name} (${mode})`).toMatchObject(at(c))
      }
      for (const c of space.targetCases) {
        const spec = space.stores[c.store as keyof typeof space.stores] as unknown as StoreSpec
        if (spec.viewer !== `A`) continue
        const target = served(c.target)
        const { trust } = await installTrust({ installs: [served({ key: `A` }), target], approvals: [ROOT_A] }, { store: build(spec), mode })
        await trust.sync()
        expect(trust.target(target, c.rootIsOwnAssistant), `${c.name} (${mode})`).toMatchObject(at(c))
      }
    }
  })

  it(`the holder wraps for an approved space at the own Assistant root only`, async () => {
    const answer = { installs: [served({ key: `A` }), served({ key: `S` })], approvals: [ROOT_A, approval(`space`, `S`, `A`)] }
    for (const own of [true, false]) {
      const { trust } = await installTrust(answer, { ownAssistant: () => own })
      await trust.sync()
      expect(trust.store.pins[keys.S!.endpointId]?.class).toBe(`space`)
      const wrapped: string[] = []
      const pair: PendingPair = { rootId: ROOT, epoch: 1, publicKey: ``, targetInstallId: `install-S`, targetX25519Public: keys.S!.x25519Public, targetEndpointId: keys.S!.endpointId }
      const api: SessionKeysApi = { mine: async () => [], pending: async () => [pair], wrap: async (input) => wrapped.push(...input.wraps.map((w) => w.targetInstallId)), create: async () => ({}), due: async () => [] }
      const holder = new KeyHolder(keys.A!.userId, `install-A`, bytes(keys.A!.installSecret), { api, store: memoryTreeKeyStore(), identity: bytes(keys.A!.identity), trust })
      await holder.keep(ROOT, 1, bytes(`11`.repeat(32)))
      await holder.answerPending()
      expect(wrapped).toEqual(own ? [`install-S`] : [])
    }
  })

  it(`a new conversation's inline key goes to the space; a new chat's does not`, async () => {
    const { trust } = await installTrust({ installs: [served({ key: `A` }), served({ key: `S` })], approvals: [ROOT_A, approval(`space`, `S`, `A`)] })
    await trust.sync()
    const api: SessionKeysApi = { mine: async () => [], pending: async () => [], wrap: async () => ({}), create: async () => ({}), due: async () => [] }
    const holder = new KeyHolder(keys.A!.userId, `install-A`, bytes(keys.A!.installSecret), { api, store: memoryTreeKeyStore(), identity: bytes(keys.A!.identity), trust })
    const deps: SealDeps = {
      holder,
      me: keys.A!.userId,
      session: () => undefined,
      sessions: () => [],
      deviceRowIds: () => [],
      myInstalls: async () => [`A`, `S`].map((name) => ({ installId: `install-${name}`, x25519Public: b64urlEncode(pushPublicKey(bytes(keys[name]!.installSecret))), endpointId: keys[name]!.endpointId, userId: keys[name]!.userId })),
      teamInstalls: async () => [],
      userIdsByEmail: () => [],
      issueIdsByIdentifier: () => [],
      sendTarget: async () => ({ rootId: null, epoch: null, publicKey: null }),
      publicKey: async () => undefined,
      ownAssistantAgent: (id) => id === `assistant`,
    }
    const readers = async (input: Parameters<typeof sealSendInput>[0]) => (await sealSendInput(input, deps)).input.seal?.wraps.map((wrap) => wrap.installId).sort()
    expect(await readers({ agentId: `assistant`, text: `hi` })).toEqual([`install-A`, `install-S`])
    expect(await readers({ agentId: `triage`, text: `hi` })).toEqual([`install-A`])
    expect(await readers({ teamId: `t1`, deviceId: `mac`, text: `hi` })).toEqual([`install-A`])
  })
})

describe(`member-space.json pinCases`, () => {
  it(`the synced rows name the own Assistant conversation`, () => {
    const agent = { role: `assistant`, ownerId: `me` }
    const root = { parentId: null, userId: `me`, agentId: `a` }
    expect(isOwnAssistantRoot(root, agent, `me`)).toBe(true)
    // Detached by a pick, a teammate's, a child, a team agent's.
    expect(isOwnAssistantRoot({ ...root, agentId: null }, undefined, `me`)).toBe(false)
    expect(isOwnAssistantRoot({ ...root, userId: `them` }, agent, `me`)).toBe(false)
    expect(isOwnAssistantRoot(root, { ...agent, ownerId: `them` }, `me`)).toBe(false)
    expect(isOwnAssistantRoot({ ...root, parentId: `p` }, agent, `me`)).toBe(false)
    expect(isOwnAssistantRoot(root, { role: `agent`, ownerId: `me` }, `me`)).toBe(false)
    for (const c of MEMBER_SPACE.pinCases) {
      const own = isOwnAssistantRoot(root, c.rootIsOwnAssistant ? agent : { role: `agent`, ownerId: `me` }, `me`)
      expect(pinCovers(c.pin, own), c.name).toBe(c.expected)
    }
  })
})
