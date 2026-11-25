// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import installFixture from "../fixtures/install-register.json" with { type: "json" }
import fixture from "../fixtures/install-trust.json" with { type: "json" }
import {
  APPROVAL_KINDS,
  APPROVAL_LABEL,
  APPROVAL_MESSAGE_BYTES,
  approvalMessage,
  callKey,
  emptyTrustStore,
  INSTALL_CODE_LABEL,
  INSTALL_TRUST_COPY,
  installCode,
  installX25519Of,
  isKeyMakerRefusal,
  KEY_MAKER_REFUSAL,
  keepWrap,
  keyMakerStep,
  keyMakerWaits,
  reconcileTrust,
  registrationValid,
  sealTo,
  signApproval,
  signRegistration,
  strandedRotationEpoch,
  TRUST_MODE,
  verifyApproval,
  wrapTarget,
  type ApprovalKind,
  type KeyRecord,
  type KeyWrite,
  type PinVia,
  type ServedInstall,
  type TrustMode,
  type TrustStore,
} from "./install-trust"
import { endpointIdOf } from "./wrap-signature"

const fromHex = (text: string) => new Uint8Array((text.match(/../g) ?? []).map((b) => parseInt(b, 16)))
const toHex = (bytes: Uint8Array) => [...bytes].map((b) => b.toString(16).padStart(2, `0`)).join(``)

type Key = (typeof fixture.keys)[keyof typeof fixture.keys]
const keys = fixture.keys as Record<string, Key>
const nameOf = (endpointId: string) => Object.keys(keys).find((name) => keys[name]!.endpointId === endpointId)!

type InstallSpec = { key: string; approvalRequested?: boolean; registration?: string; x25519Public?: string }
function served(spec: InstallSpec): ServedInstall {
  const key = keys[spec.key]!
  return {
    installId: `install-${spec.key}`,
    userId: key.userId,
    endpointId: key.endpointId,
    x25519Public: spec.x25519Public ?? key.x25519Public,
    registrationSignature:
      spec.registration === `none`
        ? null
        : spec.registration === `forged`
          ? signRegistration(fromHex(keys.Z!.identity), key.userId, key.signedAt)
          : key.registrationSignature,
    registrationSignedAt: spec.registration === `none` ? null : key.signedAt,
    approvalRequested: !!spec.approvalRequested,
    label: key.label,
  }
}

type StoreSpec = {
  viewer: string
  joined: boolean
  existing?: boolean
  pins: Record<string, unknown>
  members?: string[]
  declined?: string[]
  roots?: Record<string, { verifiedEpoch: number; epochs: Record<string, string> }>
}
function buildStore(spec: StoreSpec): TrustStore {
  const viewer = keys[spec.viewer]!
  const store = emptyTrustStore(viewer.endpointId, viewer.userId, spec.existing ?? false)
  store.joined = spec.joined
  for (const [name, pin] of Object.entries(spec.pins)) {
    const [via, by] = pin as [PinVia, string | null]
    store.pins[keys[name]!.endpointId] = { userId: keys[name]!.userId, x25519Public: keys[name]!.x25519Public, via, by: by ? keys[by]!.endpointId : null }
  }
  store.members = spec.members ?? []
  store.declined = (spec.declined ?? []).map((name) => keys[name]!.endpointId)
  store.roots = (spec.roots ?? {}) as TrustStore[`roots`]
  return store
}
const stores = fixture.stores as Record<string, StoreSpec>
const own = (store: string) => {
  const key = keys[stores[store]!.viewer]!
  return { identity: fromHex(key.identity), installSecret: fromHex(key.installSecret) }
}
const modes: TrustMode[] = [`record`, `mandatory`]
const at = (c: { now: unknown; mandatory: unknown }, mode: TrustMode) => (mode === `record` ? c.now : c.mandatory)

describe(`install-trust.json`, () => {
  test(`the constants, the keys and their registrations`, () => {
    expect(fixture.mode.release1).toBe(TRUST_MODE)
    expect(INSTALL_TRUST_COPY).toEqual({ newInstall: `New install`, reset: `Reset`, approve: `Approve`, remove: `Remove`, waiting: `Waiting for approval` })
    expect(fixture.code.label).toBe(INSTALL_CODE_LABEL)
    expect(fixture.approval.label).toBe(APPROVAL_LABEL)
    expect(fixture.approval.messageBytes).toBe(APPROVAL_MESSAGE_BYTES)
    expect(Object.values(fixture.approval.layout).reduce((sum, n) => sum + n, 0)).toBe(APPROVAL_MESSAGE_BYTES)
    expect(fixture.approval.layout.label).toBe(APPROVAL_LABEL.length + 1)
    expect(fixture.approval.kinds).toEqual(APPROVAL_KINDS)
    for (const key of Object.values(keys)) {
      const identity = fromHex(key.identity)
      expect(endpointIdOf(identity)).toBe(key.endpointId)
      expect(installX25519Of(identity)).toBe(key.x25519Public)
      expect(signRegistration(identity, key.userId, key.signedAt)).toBe(key.registrationSignature)
      expect(installCode(key.endpointId)).toBe(key.code)
      expect(registrationValid(served({ key: nameOf(key.endpointId) }))).toBe(true)
    }
    // The registration message is install-register.json's.
    const vector = installFixture
    expect(signRegistration(fromHex(vector.identity), vector.userId, vector.signedAt)).toBe(vector.signature)
  })

  test(`the code`, () => {
    for (const vector of fixture.code.vectors) {
      expect(installCode(vector.endpointId)).toBe(vector.code)
      expect(vector.code).toMatch(/^[0-9A-HJKMNP-TV-Z]{5} [0-9A-HJKMNP-TV-Z]{5}$/)
    }
    expect(installCode(`not hex`)).toBeNull()
  })

  test(`the approval vector, byte-exact`, () => {
    const v = fixture.approval.vector
    expect(toHex(approvalMessage(v.kind as ApprovalKind, v.subject)!)).toBe(v.message)
    expect(signApproval(fromHex(keys.A!.identity), v.kind as ApprovalKind, v.subject)).toBe(v.signature)
    expect(verifyApproval(v.kind as ApprovalKind, v.subject, v.signature, v.approverEndpointId)).toBe(true)
    // Every field is covered: another kind, user, endpoint or key fails.
    expect(verifyApproval(`reset`, v.subject, v.signature, v.approverEndpointId)).toBe(false)
    expect(verifyApproval(v.kind as ApprovalKind, { ...v.subject, userId: `user-other` }, v.signature, v.approverEndpointId)).toBe(false)
    expect(verifyApproval(v.kind as ApprovalKind, { ...v.subject, endpointId: keys.B!.endpointId }, v.signature, v.approverEndpointId)).toBe(false)
    expect(verifyApproval(v.kind as ApprovalKind, { ...v.subject, x25519Public: keys.B!.x25519Public }, v.signature, v.approverEndpointId)).toBe(false)
    expect(verifyApproval(v.kind as ApprovalKind, v.subject, v.signature, keys.B!.endpointId)).toBe(false)
  })

  test(`trustCases: the pin store's states and transitions`, () => {
    for (const c of fixture.trustCases) {
      const view = reconcileTrust(buildStore(c.before as StoreSpec), {
        installs: (c.installs as InstallSpec[]).map(served),
        approvals: c.approvals.map((a) => ({
          kind: a.kind as ApprovalKind,
          subjectEndpointId: keys[a.subject]!.endpointId,
          approverEndpointId: keys[a.approver]!.endpointId,
          signature: a.signature,
        })),
      })
      expect(view.status, c.name).toBe(c.status as typeof view.status)
      expect(view.store.joined, c.name).toBe(c.after.joined)
      const pins = Object.fromEntries(
        Object.entries(view.store.pins)
          .map(([endpointId, pin]) => [nameOf(endpointId), [pin.via, pin.by ? nameOf(pin.by) : null]] as const)
          .sort(([a], [b]) => (a < b ? -1 : 1))
      )
      expect(pins, c.name).toEqual(c.after.pins as typeof pins)
      expect(view.store.members, c.name).toEqual(c.after.members)
      expect(view.store.declined.map(nameOf), c.name).toEqual(c.after.declined)
      expect(
        view.rows.map((row) => ({ kind: row.kind, key: nameOf(row.endpointId), installId: row.installId, code: row.code, label: row.label })),
        c.name
      ).toEqual(c.rows)
      expect(view.approveExisting.map(nameOf), c.name).toEqual((c as { approveExisting?: string[] }).approveExisting ?? [])
      const after = c.after as { existing?: boolean }
      if (after.existing !== undefined) expect(view.store.existing, c.name).toBe(after.existing)
    }
  })

  test(`decision (e): on day one no row and no waiting state for an account whose installs all existed`, () => {
    const dayOne = fixture.trustCases.filter((c) => (c as { dayOne?: boolean }).dayOne)
    expect(dayOne.length).toBeGreaterThanOrEqual(4)
    for (const c of dayOne) {
      expect(c.rows, c.name).toEqual([])
      expect(c.status, c.name).not.toBe(`waiting`)
      expect(c.after.joined, c.name).toBe(true)
    }
  })

  test(`decision (e): a planted install after the first reconcile is never pinned`, () => {
    const attacks = fixture.trustCases.filter((c) => (c as { attack?: string }).attack === `existing`)
    expect(attacks.length).toBeGreaterThanOrEqual(3)
    for (const c of attacks) {
      if ((c.before as StoreSpec).existing) continue // the accepted moment: the first reconcile
      expect(Object.keys(c.after.pins), c.name).not.toContain(`Z`)
      expect(Object.keys(c.after.pins), c.name).toEqual(Object.keys((c.before as StoreSpec).pins).sort())
    }
    // Its row: a New install (with a registration that binds its key).
    expect(fixture.trustCases.some((c) => (c as { attack?: string }).attack === `existing` && c.rows.some((row) => row.key === `Z` && row.kind === `newInstall`))).toBe(true)
  })

  test(`R1 keepWrap, now and once mandatory`, () => {
    for (const c of fixture.rules.keepCases) {
      for (const mode of modes) {
        const got = keepWrap(c.wrap, own(c.store), buildStore(stores[c.store]!), mode)
        expect(got.keep ? { keep: true, record: got.record } : { keep: false, reason: got.reason }, `${c.name} (${mode})`).toEqual(at(c, mode) as never)
      }
    }
  })

  test(`R2 + R3 sealTo, now and once mandatory`, () => {
    for (const c of fixture.rules.sealCases) {
      const input = { ...c, held: c.held ? { secret: fromHex(c.held.secret), record: c.held.record as KeyRecord } : null }
      for (const mode of modes) {
        const got = sealTo(input, buildStore(stores[c.store]!), mode)
        expect(got.seal ? { seal: true, record: got.record } : { seal: false, reason: got.reason }, `${c.name} (${mode})`).toEqual(at(c, mode) as never)
      }
    }
  })

  test(`R4 wrapTarget, now and once mandatory`, () => {
    for (const c of fixture.rules.targetCases) {
      const roots = new Set(c.roots.map((name) => keys[name]!.endpointId))
      for (const mode of modes) {
        const got = wrapTarget(served(c.target as InstallSpec), buildStore(stores[c.store]!), mode, roots)
        expect(got.wrap ? { wrap: true, record: got.record } : { wrap: false, reason: got.reason }, `${c.name} (${mode})`).toEqual(at(c, mode) as never)
      }
    }
  })

  test(`a call's key, now and once mandatory`, () => {
    for (const c of fixture.rules.callCases) {
      const held = c.held ? { secret: fromHex(c.held.secret), record: c.held.record as KeyRecord } : null
      for (const mode of modes) {
        const got = callKey(c.target, held, buildStore(stores[c.store]!), mode)
        expect(`key` in got ? { key: true, record: got.record } : { key: false, noKey: got.noKey }, `${c.name} (${mode})`).toEqual(at(c, mode) as never)
      }
    }
  })

  test(`maliciousServer: every attack, its lying answers and the verdicts its rule replays`, () => {
    expect(fixture.maliciousServer.map((entry) => entry.attack)).toEqual([1, 2, 3, 4])
    for (const entry of fixture.maliciousServer) {
      expect(entry.cases.length, `attack ${entry.attack}`).toBeGreaterThan(0)
      for (const c of entry.cases) {
        const rule = (fixture.rules as Record<string, Array<{ name: string; now: unknown; mandatory: unknown; attack?: number }>>)[c.replay]!
        const source = rule.find((r) => r.name === c.name)!
        expect(source.attack, c.name).toBe(entry.attack)
        expect({ now: c.now, mandatory: c.mandatory }, c.name).toEqual({ now: source.now, mandatory: source.mandatory })
        expect(Object.keys(c.answers).length, c.name).toBeGreaterThan(0)
      }
      // Once mandatory, no attack case keeps, seals, wraps or keys anything.
      for (const c of entry.cases) {
        const m = c.mandatory as Record<string, unknown>
        expect(m.keep ?? m.seal ?? m.wrap ?? m.key, c.name).toBe(false)
      }
    }
  })

  test(`the new root rule`, () => {
    expect(fixture.newRoot.makerRecord).toBe(`verified`)
    expect(fixture.newRoot.readerRecordBeforeResign).toEqual({ now: `unsigned`, mandatory: `refused` })
  })
  test(`keyMaker (#EXP-2123): only a joined install makes a chat key`, () => {
    const maker = fixture.keyMaker
    expect(KEY_MAKER_REFUSAL).toBe(maker.refusal)
    expect(isKeyMakerRefusal(maker.refusal)).toBe(true)
    expect(isKeyMakerRefusal(`Only the session's starter or its device rotates the key`)).toBe(false)
    for (const c of maker.serverCases) expect(keyMakerWaits(c), c.name).toBe(c.waits)
    for (const c of maker.makerCases as Array<{ name: string; joined: boolean; refused?: boolean; write: string; step: string }>) {
      expect(keyMakerStep({ joined: c.joined, write: c.write as KeyWrite, refused: c.refused }), c.name).toBe(c.step as never)
    }
    // A waiting install makes nothing, whatever the write.
    for (const write of [`firstKey`, `newRoot`, `rotation`, `deviceKey`] as const) expect(keyMakerStep({ joined: false, write })).not.toBe(`make`)
    for (const c of maker.strandedCases) expect(strandedRotationEpoch(c), c.name).toBe(c.epoch)
  })
})
