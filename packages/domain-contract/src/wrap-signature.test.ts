// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import installFixture from "../fixtures/install-register.json" with { type: "json" }
import pushFixture from "../fixtures/push-seal.json" with { type: "json" }
import sealedFixture from "../fixtures/sealed-content.json" with { type: "json" }
import fixture from "../fixtures/wrap-signature.json" with { type: "json" }
import { b64urlDecode, b64urlEncode, pushPublicKey, treeKeyFromSecret, wrapTreeKey, wrapTreeKeyWith } from "./sealed-content"
import {
  acceptVouch,
  acceptWrap,
  endpointIdOf,
  RESIGN_MAX_WRAPS,
  resignAfter,
  resignOutcome,
  signWrap,
  verifyWrapSignature,
  WRAP_SIGNATURE,
  wrapSignatureMessage,
  type WrapStatement,
} from "./wrap-signature"

const hex = (text: string) => new Uint8Array((text.match(/../g) ?? []).map((b) => parseInt(b, 16)))
const toHex = (bytes: Uint8Array) => [...bytes].map((b) => b.toString(16).padStart(2, `0`)).join(``)

const own = { identity: hex(fixture.recipient.identity), installSecret: hex(fixture.recipient.installSecret) }
const vector = fixture.vector
const statement: WrapStatement = {
  rootId: vector.rootId,
  epoch: vector.epoch,
  treePublicKey: vector.treePublicKey,
  recipientEndpointId: vector.recipientEndpointId,
  wrapped: vector.wrapped,
}

describe(`wrap-signature.json`, () => {
  test(`the constants and the keys`, () => {
    expect(fixture.label).toBe(WRAP_SIGNATURE.label)
    expect(fixture.messageBytes).toBe(WRAP_SIGNATURE.messageBytes)
    expect(fixture.signatureBytes).toBe(WRAP_SIGNATURE.signatureBytes)
    expect(Object.values(fixture.layout).reduce((sum, n) => sum + n, 0)).toBe(fixture.messageBytes)
    expect(fixture.layout.label).toBe(fixture.label.length + 1)
    for (const install of [fixture.signer, fixture.recipient, fixture.other]) {
      expect(endpointIdOf(hex(install.identity))).toBe(install.endpointId)
    }
    // The signer is install-register.json's install; the recipient's X25519
    // key is push-seal.json's derivation of its identity.
    expect(fixture.signer.identity).toBe(installFixture.identity)
    expect(fixture.signer.endpointId).toBe(installFixture.endpointId)
    expect(fixture.recipient.identity).toBe(pushFixture.derive.identity)
    expect(fixture.recipient.installSecret).toBe(pushFixture.derive.secret)
    expect(b64urlEncode(pushPublicKey(own.installSecret))).toBe(fixture.recipient.x25519Public)
    expect(fixture.tree.secret).toBe(sealedFixture.tree.secret)
    expect(treeKeyFromSecret(hex(fixture.tree.secret)).publicKey).toBe(fixture.tree.publicKey)
    expect(treeKeyFromSecret(hex(fixture.otherTree.secret)).publicKey).toBe(fixture.otherTree.publicKey)
  })

  test(`the vector, byte-exact`, () => {
    expect(wrapTreeKeyWith(hex(vector.ephemeralSecret), fixture.recipient.x25519Public, hex(fixture.tree.secret))).toBe(vector.wrapped)
    const message = wrapSignatureMessage(statement)!
    expect(message.length).toBe(fixture.messageBytes)
    expect(toHex(message)).toBe(vector.message)
    const signature = signWrap(hex(fixture.signer.identity), statement)
    expect(signature).toBe(vector.signature)
    expect(toHex(b64urlDecode(signature)!)).toBe(vector.signatureHex)
    expect(verifyWrapSignature(statement, signature, fixture.signer.endpointId)).toBe(true)
    expect(verifyWrapSignature(statement, signature, fixture.other.endpointId)).toBe(false)
    // Every byte of the message is covered.
    for (const field of [`rootId`, `epoch`, `treePublicKey`, `recipientEndpointId`, `wrapped`] as const) {
      const moved: WrapStatement = {
        ...statement,
        rootId: field === `rootId` ? `11234567-89ab-4def-8123-456789abcdef` : statement.rootId,
        epoch: field === `epoch` ? 2 : statement.epoch,
        treePublicKey: field === `treePublicKey` ? fixture.otherTree.publicKey : statement.treePublicKey,
        recipientEndpointId: field === `recipientEndpointId` ? fixture.other.endpointId : statement.recipientEndpointId,
        wrapped: field === `wrapped` ? wrapTreeKey(fixture.recipient.x25519Public, hex(fixture.tree.secret)) : statement.wrapped,
      }
      expect(verifyWrapSignature(moved, signature, fixture.signer.endpointId)).toBe(false)
    }
    expect(() => signWrap(hex(fixture.signer.identity), { ...statement, epoch: 0 })).toThrow()
  })

  test(`accepted`, () => {
    for (const c of fixture.accepted) {
      const got = acceptWrap(c, own, c.pins, c.required)
      expect(got.status, c.name).toBe(c.status as typeof got.status)
      if (got.status === `refused`) continue
      expect(toHex(got.secret), c.name).toBe(fixture.tree.secret)
      expect(got.treePublicKey, c.name).toBe(fixture.tree.publicKey)
      if (got.status === `verified`) expect(got.signerEndpointId, c.name).toBe(c.signerEndpointId!)
    }
  })

  test(`refused, each with its reason`, () => {
    const reasons = new Set<string>()
    for (const c of fixture.refused) {
      expect(fixture.refusals, c.name).toContain(c.reason)
      expect(acceptWrap(c, own, c.pins, c.required), c.name).toEqual({ status: `refused`, reason: c.reason } as ReturnType<typeof acceptWrap>)
      reasons.add(c.reason)
    }
    expect([...reasons].sort()).toEqual([...fixture.refusals].sort())
  })

  test(`a vouch`, () => {
    for (const c of fixture.vouchCases) {
      expect(acceptVouch(c, c.signature, c.signerEndpointId, c.pins), c.name).toBe(c.result as ReturnType<typeof acceptVouch>)
    }
  })

  test(`resign: every case, the order and the holder's answer`, () => {
    const r = fixture.resign
    expect(r.maxWraps).toBe(RESIGN_MAX_WRAPS)
    for (const c of r.cases) {
      expect(resignOutcome(c as Parameters<typeof resignOutcome>[0]), c.name).toEqual({ rule: c.rule, outcome: c.outcome } as ReturnType<typeof resignOutcome>)
      expect(r.outcomes[c.rule as keyof typeof r.outcomes], c.name).toBe(c.outcome)
    }
    // Every rule is exercised, in the fixture's order.
    expect([...new Set(r.cases.map((c) => c.rule))].sort()).toEqual([...r.order].sort())
    for (const c of r.failureCases) expect(resignAfter({ code: c.code }), c.name).toBe(c.after as ReturnType<typeof resignAfter>)
  })
})
