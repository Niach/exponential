import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/wrap-signature.json"
import { signedWrapColumns, WRAP_SIGNATURE_REFUSAL } from "@/lib/sessions/wrap-signatures"

// #EXP-1950: the server's check of a signed wrap on write replays the
// contract's vector byte-exact (fixtures/wrap-signature.json `vector`).

const v = fixture.vector
const key = { rootId: v.rootId, epoch: v.epoch, treePublicKey: v.treePublicKey }
const wrap = { recipientEndpointId: v.recipientEndpointId, wrapped: v.wrapped, signature: v.signature }
const check = (signer: string, k: typeof key, w: { recipientEndpointId?: string | null; wrapped?: string; signature?: string | null }) =>
  () => signedWrapColumns(signer, k, [{ ...wrap, ...w }])

describe(`signedWrapColumns (#EXP-1950)`, () => {
  it(`keeps the fixture's signature and its signer, any root id casing`, () => {
    expect(signedWrapColumns(fixture.signer.endpointId, key, [wrap])).toEqual([{ signature: v.signature, signerEndpointId: fixture.signer.endpointId }])
    expect(check(fixture.signer.endpointId, { ...key, rootId: v.rootId.toUpperCase() }, {})()).toHaveLength(1)
  })

  it(`stores an unsigned wrap as null, beside a signed one`, () => {
    expect(signedWrapColumns(fixture.signer.endpointId, key, [{ ...wrap, signature: null }, wrap, { recipientEndpointId: null, wrapped: v.wrapped }])).toEqual([
      { signature: null, signerEndpointId: null },
      { signature: v.signature, signerEndpointId: fixture.signer.endpointId },
      { signature: null, signerEndpointId: null },
    ])
  })

  const refused: Array<[string, () => unknown]> = [
    [`a tampered signature`, check(fixture.signer.endpointId, key, { signature: fixture.refused[0]!.signature! })],
    [`another signer`, check(fixture.other.endpointId, key, {})],
    [`another root`, check(fixture.signer.endpointId, { ...key, rootId: crypto.randomUUID() }, {})],
    [`another epoch`, check(fixture.signer.endpointId, { ...key, epoch: 2 }, {})],
    [`another tree public key`, check(fixture.signer.endpointId, { ...key, treePublicKey: fixture.otherTree.publicKey }, {})],
    [`another recipient`, check(fixture.signer.endpointId, key, { recipientEndpointId: fixture.other.endpointId })],
    [`an unknown recipient`, check(fixture.signer.endpointId, key, { recipientEndpointId: null })],
    [`a malformed signature`, check(fixture.signer.endpointId, key, { signature: `x` })],
  ]
  for (const [name, run] of refused) {
    it(`refuses ${name}`, () => expect(run).toThrow(WRAP_SIGNATURE_REFUSAL))
  }
})
