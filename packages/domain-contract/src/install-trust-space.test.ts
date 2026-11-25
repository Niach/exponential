// #EXP-2161: the `space` section of install-trust.json (member spaces).
import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/install-trust.json"
import { MEMBER_SPACE } from "./member-space"
import {
  APPROVAL_KINDS,
  approvalMessage,
  emptyTrustStore,
  installCode,
  installX25519Of,
  keepWrap,
  reconcileTrust,
  registrationClass,
  sealTo,
  signApproval,
  signRegistration,
  verifyApproval,
  wrapTarget,
  type ApprovalKind,
  type PinVia,
  type ServedInstall,
  type TrustMode,
  type TrustStore,
} from "./install-trust"
import { endpointIdOf } from "./wrap-signature"

const space = fixture.space
type Key = { identity: string; userId: string; endpointId: string; installSecret: string; x25519Public: string; signedAt: string; registrationSignature: string; code: string; label: string }
const keys = { ...fixture.keys, ...space.keys } as Record<string, Key>
const nameOf = (endpointId: string) => Object.keys(keys).find((name) => keys[name]!.endpointId === endpointId)!
const fromHex = (text: string) => new Uint8Array((text.match(/../g) ?? []).map((b) => parseInt(b, 16)))
const toHex = (bytes: Uint8Array) => [...bytes].map((b) => b.toString(16).padStart(2, `0`)).join(``)

type InstallSpec = { key: string; approvalRequested?: boolean; registration?: string }
function served(spec: InstallSpec): ServedInstall {
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

type StoreSpec = { viewer: string; space?: boolean; joined: boolean; existing?: boolean; pins: Record<string, unknown>; members?: string[] }
function build(spec: StoreSpec): TrustStore {
  const viewer = keys[spec.viewer]!
  const store = emptyTrustStore(viewer.endpointId, viewer.userId, spec.existing ?? false)
  store.joined = spec.joined
  if (spec.space) store.space = true
  for (const [name, pin] of Object.entries(spec.pins)) {
    const [via, by, kind] = pin as [PinVia, string | null, string | undefined]
    store.pins[keys[name]!.endpointId] = {
      userId: keys[name]!.userId,
      x25519Public: keys[name]!.x25519Public,
      via,
      by: by ? keys[by]!.endpointId : null,
      ...(kind === `space` ? { class: `space` as const } : {}),
    }
  }
  store.members = spec.members ?? []
  return store
}
const stores = space.stores as unknown as Record<string, StoreSpec>
const modes: TrustMode[] = [`record`, `mandatory`]
const at = (c: { now: unknown; mandatory: unknown }, mode: TrustMode) => (mode === `record` ? c.now : c.mandatory)

describe(`install-trust.json space (#EXP-2161)`, () => {
  test(`the constants, the registrations and the approval vector`, () => {
    expect(space.registrationLabel).toBe(MEMBER_SPACE.registrationLabel)
    expect(space.kind).toBe(MEMBER_SPACE.approvalKind)
    expect(space.kindByte).toBe(MEMBER_SPACE.approvalKindByte)
    expect(APPROVAL_KINDS).toEqual({ ...fixture.approval.kinds, space: space.kindByte })
    for (const [name, key] of Object.entries(space.keys)) {
      const identity = fromHex(key.identity)
      expect(endpointIdOf(identity)).toBe(key.endpointId)
      expect(installX25519Of(identity)).toBe(key.x25519Public)
      expect(signRegistration(identity, key.userId, key.signedAt, `space`)).toBe(key.registrationSignature)
      expect(installCode(key.endpointId)).toBe(key.code)
      expect(registrationClass(served({ key: name }))).toBe(`space`)
    }
    expect(registrationClass(served({ key: `A` }))).toBe(`install`)
    expect(registrationClass(served({ key: `S`, registration: `none` }))).toBe(`install`)
    const v = space.approval
    expect(toHex(approvalMessage(`space`, v.subject)!)).toBe(v.message)
    expect(signApproval(fromHex(keys.A!.identity), `space`, v.subject)).toBe(v.signature)
    expect(verifyApproval(`space`, v.subject, v.signature, v.approverEndpointId)).toBe(true)
    expect(verifyApproval(`approve`, v.subject, v.signature, v.approverEndpointId)).toBe(false)
  })

  test(`trustCases`, () => {
    for (const c of space.trustCases) {
      const view = reconcileTrust(build(c.before as unknown as StoreSpec), {
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
        Object.entries(view.store.pins).map(([endpointId, pin]) => [nameOf(endpointId), [pin.via, pin.by ? nameOf(pin.by) : null, ...(pin.class ? [pin.class] : [])]])
      )
      expect(pins, c.name).toEqual(c.after.pins as unknown as typeof pins)
      expect(view.store.members, c.name).toEqual(c.after.members)
      expect(view.rows.map((row) => ({ kind: row.kind as string, key: nameOf(row.endpointId) })), c.name).toEqual(c.rows)
      if (view.store.space) expect(view.approveExisting, c.name).toEqual([])
    }
  })

  test(`keepWrap, sealTo and wrapTarget cover the member's own Assistant alone`, () => {
    for (const mode of modes) {
      for (const c of space.keepCases) {
        const viewer = keys[stores[c.store]!.viewer]!
        const got = keepWrap(c.wrap, { identity: fromHex(viewer.identity), installSecret: fromHex(viewer.installSecret) }, build(stores[c.store]!), mode, c.rootIsOwnAssistant)
        expect(got.keep ? { keep: true, record: got.record } : { keep: false, reason: got.reason }, `${c.name} (${mode})`).toEqual(at(c, mode) as never)
      }
      for (const c of space.sealCases) {
        const got = sealTo({ ...c, held: null }, build(stores[c.store]!), mode, c.rootIsOwnAssistant)
        expect(got.seal ? { seal: true, record: got.record } : { seal: false, reason: got.reason }, `${c.name} (${mode})`).toEqual(at(c, mode) as never)
      }
      for (const c of space.targetCases) {
        const got = wrapTarget(served(c.target), build(stores[c.store]!), mode, new Set(), c.rootIsOwnAssistant)
        expect(got as unknown, `${c.name} (${mode})`).toEqual(at(c, mode) as never)
      }
    }
  })
})
