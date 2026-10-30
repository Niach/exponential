// Sealed push payloads for the push relay's accountless mode (EXP-1428).
// Byte-exact with `push_seal` / `push_open` in
// apps/desktop/crates/exponential-net/src/push.rs (format v1):
//
//   sealed = 0x01 ‖ eph_pub[32] ‖ ciphertext ‖ tag[16]
//   shared = X25519(eph_secret, recipient_pub)          (all-zero = refused)
//   okm    = HKDF-SHA256(salt = eph_pub ‖ recipient_pub, ikm = shared,
//                        info = "exp/enet/push/1", L = 44)
//   key    = okm[0..32], nonce = okm[32..44]
//   ciphertext ‖ tag = ChaCha20-Poly1305(key, nonce, aad = 0x01, plaintext)
//
// The relay only ever sees the opaque bytes; the recipient's notification
// extension opens them with its X25519 secret. node:crypto takes X25519 keys
// only as KeyObjects, so raw 32-byte keys are wrapped in fixed PKCS#8/SPKI
// DER prefixes (RFC 8410). The AEAD comes from @noble/ciphers: Bun's
// node:crypto has no chacha20-poly1305 cipher.

import {
  createPrivateKey,
  createPublicKey,
  diffieHellman,
  hkdfSync,
  randomBytes,
  type KeyObject,
} from "node:crypto"
import { chacha20poly1305 } from "@noble/ciphers/chacha.js"

export const PUSH_VERSION = 1
/** Bytes added to the plaintext (version + ephemeral key + tag). */
export const PUSH_OVERHEAD = 1 + 32 + 16
const INFO = Buffer.from(`exp/enet/push/1`)
const AAD = Buffer.from([PUSH_VERSION])

const PKCS8_X25519_PREFIX = Buffer.from(`302e020100300506032b656e04220420`, `hex`)
const SPKI_X25519_PREFIX = Buffer.from(`302a300506032b656e032100`, `hex`)

function privateKeyFromRaw(secret: Uint8Array): KeyObject {
  if (secret.length !== 32) throw new Error(`X25519 secret must be 32 bytes`)
  return createPrivateKey({
    key: Buffer.concat([PKCS8_X25519_PREFIX, secret]),
    format: `der`,
    type: `pkcs8`,
  })
}

function publicKeyFromRaw(pub: Uint8Array): KeyObject {
  if (pub.length !== 32) throw new Error(`X25519 public key must be 32 bytes`)
  return createPublicKey({
    key: Buffer.concat([SPKI_X25519_PREFIX, pub]),
    format: `der`,
    type: `spki`,
  })
}

function rawPublic(key: KeyObject): Buffer {
  const der = createPublicKey(key).export({ format: `der`, type: `spki` })
  return der.subarray(der.length - 32)
}

function x25519(secret: KeyObject, pub: Uint8Array): Buffer {
  let shared: Buffer
  try {
    shared = diffieHellman({ privateKey: secret, publicKey: publicKeyFromRaw(pub) })
  } catch {
    // OpenSSL itself refuses an all-zero result (low-order point).
    throw new Error(`low-order public key`)
  }
  if (shared.every((b) => b === 0)) throw new Error(`low-order public key`)
  return shared
}

// @noble/ciphers takes plain Uint8Arrays only (a Buffer fails its check).
function u8(b: Uint8Array): Uint8Array {
  return new Uint8Array(b.buffer, b.byteOffset, b.byteLength)
}

function deriveKey(
  shared: Buffer,
  ephPub: Uint8Array,
  recipientPub: Uint8Array
): { key: Buffer; nonce: Buffer } {
  const okm = Buffer.from(
    hkdfSync(`sha256`, shared, Buffer.concat([ephPub, recipientPub]), INFO, 44)
  )
  return { key: okm.subarray(0, 32), nonce: okm.subarray(32, 44) }
}

/** Seals with a caller-chosen ephemeral secret. Tests only (vectors). */
export function pushSealWith(
  ephSecret: Uint8Array,
  recipientPublic: Uint8Array,
  plaintext: Uint8Array
): Uint8Array {
  const eph = privateKeyFromRaw(ephSecret)
  const ephPub = rawPublic(eph)
  const shared = x25519(eph, recipientPublic)
  const { key, nonce } = deriveKey(shared, ephPub, recipientPublic)
  const ct = chacha20poly1305(u8(key), u8(nonce), u8(AAD)).encrypt(u8(plaintext))
  return new Uint8Array(Buffer.concat([AAD, ephPub, ct]))
}

/** Seals `plaintext` to `recipientPublic` (raw X25519, 32 bytes) with a fresh ephemeral key. */
export function pushSeal(
  recipientPublic: Uint8Array,
  plaintext: Uint8Array
): Uint8Array {
  return pushSealWith(randomBytes(32), recipientPublic, plaintext)
}

/** Opens a sealed payload with the recipient's raw X25519 secret. Throws on
 * an unknown version, truncation or any tampering. */
export function pushOpen(
  recipientSecret: Uint8Array,
  sealed: Uint8Array
): Uint8Array {
  if (sealed.length < PUSH_OVERHEAD) throw new Error(`sealed payload too short`)
  if (sealed[0] !== PUSH_VERSION) {
    throw new Error(`unknown push format version ${sealed[0]}`)
  }
  const ephPub = sealed.subarray(1, 33)
  const secret = privateKeyFromRaw(recipientSecret)
  const recipientPub = rawPublic(secret)
  const shared = x25519(secret, ephPub)
  const { key, nonce } = deriveKey(shared, ephPub, recipientPub)
  try {
    return chacha20poly1305(u8(key), u8(nonce), u8(AAD)).decrypt(u8(sealed.subarray(33)))
  } catch {
    throw new Error(`sealed payload failed authentication`)
  }
}

/** The raw X25519 public key for a raw secret (clamped, as Rust's `mul_base_clamped`). */
export function pushPublicKey(secret: Uint8Array): Uint8Array {
  return new Uint8Array(rawPublic(privateKeyFromRaw(secret)))
}
