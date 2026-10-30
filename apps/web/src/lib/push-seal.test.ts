import { describe, expect, it } from "vitest"
import { chacha20poly1305 } from "@noble/ciphers/chacha.js"
import {
  PUSH_OVERHEAD,
  pushOpen,
  pushPublicKey,
  pushSeal,
  pushSealWith,
} from "@/lib/push-seal"

// Locks lib/push-seal.ts byte-for-byte to `push_seal` / `push_open` in
// apps/desktop/crates/exponential-net/src/push.rs (EXP-1428). The vectors
// below were cross-checked against the Rust crate on 2026-10-30 with a
// throwaway binary path-depending on exponential-net: Rust's push_open opened
// TS_SEALED to PLAINTEXT, and RUST_SEALED is a Rust push_seal output that the
// TS side must open.

const hex = (s: string) => new Uint8Array(Buffer.from(s, `hex`))
const toHex = (b: Uint8Array) => Buffer.from(b).toString(`hex`)
const bytes = (f: (i: number) => number) =>
  new Uint8Array(Array.from({ length: 32 }, (_, i) => f(i)))

const RECIPIENT_SECRET = bytes((i) => i + 1) // 01 02 … 20
const EPH_SECRET = bytes((i) => 0x80 + i) // 80 81 … 9f
const RECIPIENT_PUBLIC = `07a37cbc142093c8b755dc1b10e86cb426374ad16aa853ed0bdfc0b2b86d1c7c`
const PLAINTEXT = `{"title":"Hello","body":"World","data":{"userId":"u1"}}`
const TS_SEALED =
  `01493e82fc74464a59268817623d2053c5eb8e2cc4a988b4fee179ec6b010d531d2e644e142a277df71d0d69c6ed885dc581701b86040561c808cccd027100eeedf02ae302fffde8b091315b8377c7cb14645ecf8615fca2de8fd6263ebd9d49726edb0b90ccd2e4`
const RUST_SEALED =
  `01b47474edd575050ba7eb926ba31ca5ce7a6a4b3b67b69c804505406ecf1f4a695ce01e126391e8394cac225f83134bda4d04a059e4083d9554`

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
      new TextEncoder().encode(PLAINTEXT)
    )
    expect(toHex(sealed)).toBe(TS_SEALED)
  })

  it(`opens a payload sealed by Rust`, () => {
    expect(new TextDecoder().decode(pushOpen(RECIPIENT_SECRET, hex(RUST_SEALED)))).toBe(
      `from rust`
    )
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
    expect(() => pushSeal(new Uint8Array(32), new Uint8Array([1]))).toThrow(
      /low-order/
    )
  })
})
