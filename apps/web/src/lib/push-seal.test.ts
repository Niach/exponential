import { describe, expect, it } from "vitest"
import { chacha20poly1305 } from "@noble/ciphers/chacha.js"
import {
  PUSH_OVERHEAD,
  pushOpen,
  pushPublicKey,
  pushSeal,
  pushSealWith,
} from "@/lib/push-seal"

import fixture from "@exp/domain-contract/fixtures/push-seal.json"

// Locks lib/push-seal.ts byte-for-byte to `push_seal` / `push_open` in
// apps/desktop/crates/exponential-net/src/push.rs (EXP-1428) through the
// shared vectors every implementation replays (EXP-1435: Rust, web, Swift,
// Kotlin), packages/domain-contract/fixtures/push-seal.json.

const hex = (s: string) => new Uint8Array(Buffer.from(s, `hex`))
const toHex = (b: Uint8Array) => Buffer.from(b).toString(`hex`)
const utf8 = new TextEncoder()

const RECIPIENT_SECRET = hex(fixture.recipient.secret)
const RECIPIENT_PUBLIC = fixture.recipient.public
const EPH_SECRET = hex(fixture.vector.ephemeralSecret)

describe(`ChaCha20-Poly1305 (RFC 8439)`, () => {
  it(`matches the §2.8.2 AEAD test vector`, () => {
    const key = hex(`808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f`)
    const nonce = hex(`070000004041424344454647`)
    const aad = hex(`50515253c0c1c2c3c4c5c6c7`)
    const pt = new TextEncoder().encode(
      `Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.`
    )
    const out = chacha20poly1305(key, nonce, aad).encrypt(pt)
    expect(toHex(out.subarray(0, 16))).toBe(`d31a8d34648e60db7b86afbc53ef7ec2`)
    expect(toHex(out.subarray(out.length - 16))).toBe(`1ae10b594f09e26a7e902ecbd0600691`)
    expect(toHex(chacha20poly1305(key, nonce, aad).decrypt(out))).toBe(toHex(pt))
  })
})

describe(`pushSeal (byte-exact with exponential-net push.rs)`, () => {
  it(`derives the recipient public key like Rust's mul_base_clamped`, () => {
    expect(toHex(pushPublicKey(RECIPIENT_SECRET))).toBe(RECIPIENT_PUBLIC)
  })

  it(`reproduces the Rust-verified vector from a fixed ephemeral secret`, () => {
    const sealed = pushSealWith(
      EPH_SECRET,
      hex(RECIPIENT_PUBLIC),
      utf8.encode(fixture.vector.plaintext)
    )
    expect(toHex(sealed)).toBe(fixture.vector.sealed)
    const opened = new TextDecoder().decode(pushOpen(RECIPIENT_SECRET, sealed))
    expect(JSON.parse(opened)).toEqual(fixture.vector.opened)
  })

  it(`opens a payload sealed by Rust`, () => {
    expect(
      new TextDecoder().decode(pushOpen(RECIPIENT_SECRET, hex(fixture.rustSealed.sealed)))
    ).toBe(fixture.rustSealed.plaintext)
  })

  it.each(fixture.refused)(`refuses to open: $name`, ({ secret, sealed }) => {
    expect(() => pushOpen(hex(secret), hex(sealed))).toThrow()
  })

  it(`round-trips with a fresh ephemeral key per seal`, () => {
    const pub = hex(RECIPIENT_PUBLIC)
    const msg = new TextEncoder().encode(`hello push`)
    const a = pushSeal(pub, msg)
    const b = pushSeal(pub, msg)
    expect(a.length).toBe(PUSH_OVERHEAD + msg.length)
    expect(a[0]).toBe(1)
    expect(toHex(a)).not.toBe(toHex(b))
    expect(toHex(pushOpen(RECIPIENT_SECRET, a))).toBe(toHex(msg))
  })

  it(`refuses every tampered byte, truncation and the wrong key`, () => {
    const sealed = pushSeal(hex(RECIPIENT_PUBLIC), new TextEncoder().encode(`hello push`))
    for (let i = 0; i < sealed.length; i++) {
      const t = sealed.slice()
      t[i] ^= 0x01
      expect(() => pushOpen(RECIPIENT_SECRET, t), `byte ${i}`).toThrow()
    }
    expect(() => pushOpen(RECIPIENT_SECRET, sealed.subarray(0, sealed.length - 1))).toThrow()
    expect(() => pushOpen(EPH_SECRET, sealed)).toThrow()
  })

  it(`refuses a low-order recipient key (all-zero shared secret)`, () => {
    expect(() => pushSeal(hex(fixture.lowOrderPublic), new Uint8Array([1]))).toThrow(
      /low-order/
    )
  })
})
