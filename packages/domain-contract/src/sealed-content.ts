// EXP-1458: sealed session content (fixtures/sealed-content.json is the
// spec). One X25519 keypair per session tree and epoch; content is
// `push_seal(tree_public, plaintext)` (fixtures/push-seal.json, byte-exact
// with enet-core push.rs); the tree secret reaches every reader install
// wrapped with the same primitive. Mirrors: enet-core `sealed.rs` (Rust,
// exported through exponential-net-ffi for Swift/Kotlin and through
// exponential-net-wasm). Pure JS (@noble), so it runs in the browser too.

import { chacha20poly1305 } from "@noble/ciphers/chacha.js"
import { x25519 } from "@noble/curves/ed25519"
import { hkdf } from "@noble/hashes/hkdf.js"
import { sha256 } from "@noble/hashes/sha2.js"
import placeholderFixture from "../fixtures/sealed-content.json" with { type: "json" }

// ── the push_seal primitive (format v1) ─────────────────────────────────────
//   sealed = 0x01 ‖ eph_pub[32] ‖ ciphertext ‖ tag[16]
//   shared = X25519(eph_secret, recipient_pub)          (all-zero = refused)
//   okm    = HKDF-SHA256(salt = eph_pub ‖ recipient_pub, ikm = shared,
//                        info = "exp/enet/push/1", L = 44)
//   key    = okm[0..32], nonce = okm[32..44]
//   ciphertext ‖ tag = ChaCha20-Poly1305(key, nonce, aad = 0x01, plaintext)

export const PUSH_VERSION = 1
/** Bytes `pushSeal` adds to the plaintext (version + ephemeral key + tag). */
export const PUSH_OVERHEAD = 1 + 32 + 16
const PUSH_INFO = new TextEncoder().encode(`exp/enet/push/1`)
const PUSH_AAD = new Uint8Array([PUSH_VERSION])

function randomBytes(n: number): Uint8Array {
  const out = new Uint8Array(n)
  globalThis.crypto.getRandomValues(out)
  return out
}

function concat(...parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0))
  let at = 0
  for (const part of parts) {
    out.set(part, at)
    at += part.length
  }
  return out
}

function key32(bytes: Uint8Array, what: string): Uint8Array {
  if (bytes.length !== 32) throw new Error(`${what} must be 32 bytes`)
  return new Uint8Array(bytes)
}

function sharedSecret(secret: Uint8Array, pub: Uint8Array): Uint8Array {
  let shared: Uint8Array
  try {
    shared = x25519.getSharedSecret(secret, pub)
  } catch {
    throw new Error(`low-order public key`)
  }
  if (shared.every((b) => b === 0)) throw new Error(`low-order public key`)
  return shared
}

function pushKey(shared: Uint8Array, ephPub: Uint8Array, recipientPub: Uint8Array) {
  const okm = hkdf(sha256, shared, concat(ephPub, recipientPub), PUSH_INFO, 44)
  return { key: okm.subarray(0, 32), nonce: okm.subarray(32, 44) }
}

/** The raw X25519 public key of a raw secret (clamped). */
export function pushPublicKey(secret: Uint8Array): Uint8Array {
  return x25519.getPublicKey(key32(secret, `X25519 secret`))
}

/** `pushSeal` with a caller-chosen ephemeral secret: test vectors only. */
export function pushSealWith(ephSecret: Uint8Array, recipientPublic: Uint8Array, plaintext: Uint8Array): Uint8Array {
  const eph = key32(ephSecret, `ephemeral secret`)
  const recipient = key32(recipientPublic, `recipient public key`)
  const ephPub = x25519.getPublicKey(eph)
  const { key, nonce } = pushKey(sharedSecret(eph, recipient), ephPub, recipient)
  const ct = chacha20poly1305(key, nonce, PUSH_AAD).encrypt(new Uint8Array(plaintext))
  return concat(PUSH_AAD, ephPub, ct)
}

/** Seals `plaintext` to a raw X25519 public key with a fresh ephemeral key. */
export function pushSeal(recipientPublic: Uint8Array, plaintext: Uint8Array): Uint8Array {
  return pushSealWith(randomBytes(32), recipientPublic, plaintext)
}

/** Opens a `pushSeal`ed payload; throws on an unknown version, truncation or tampering. */
export function pushOpen(recipientSecret: Uint8Array, sealed: Uint8Array): Uint8Array {
  if (sealed.length < PUSH_OVERHEAD) throw new Error(`sealed payload too short`)
  if (sealed[0] !== PUSH_VERSION) throw new Error(`unknown push format version ${sealed[0]}`)
  const secret = key32(recipientSecret, `recipient secret`)
  const ephPub = sealed.slice(1, 33)
  const { key, nonce } = pushKey(sharedSecret(secret, ephPub), ephPub, x25519.getPublicKey(secret))
  try {
    return chacha20poly1305(key, nonce, PUSH_AAD).decrypt(sealed.slice(33))
  } catch {
    throw new Error(`sealed payload failed authentication`)
  }
}

// ── base64url (no padding, canonical) ───────────────────────────────────────

const B64URL = `ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_`

export function b64urlEncode(bytes: Uint8Array): string {
  let out = ``
  for (let i = 0; i < bytes.length; i += 3) {
    const n = (bytes[i]! << 16) | ((bytes[i + 1] ?? 0) << 8) | (bytes[i + 2] ?? 0)
    const chars = Math.min(4, Math.ceil(((bytes.length - i) * 8) / 6))
    for (let c = 0; c < chars; c++) out += B64URL[(n >> (18 - 6 * c)) & 63]
  }
  return out
}

/** Strict: the URL-safe alphabet only, no padding, canonical trailing bits.
 *  Null when `text` is not such an encoding. */
export function b64urlDecode(text: string): Uint8Array | null {
  if (!/^[A-Za-z0-9_-]*$/.test(text) || text.length % 4 === 1) return null
  const out = new Uint8Array(Math.floor((text.length * 6) / 8))
  let acc = 0
  let bits = 0
  let at = 0
  for (const ch of text) {
    acc = (acc << 6) | B64URL.indexOf(ch)
    bits += 6
    if (bits >= 8) {
      bits -= 8
      out[at++] = (acc >> bits) & 0xff
    }
    acc &= (1 << bits) - 1
  }
  return acc === 0 ? out : null
}

// ── tree keys ───────────────────────────────────────────────────────────────

/** A wrapped tree secret: `pushSeal(install_x25519_public, secret[32])`. */
export const WRAPPED_TREE_KEY_BYTES = PUSH_OVERHEAD + 32
/** Epochs are 1-based and fit the `int` columns. */
export const SEAL_EPOCH_MAX = 2_147_483_647

export interface TreeKey {
  /** The tree's X25519 secret (never leaves a reader install unwrapped). */
  secret: Uint8Array
  /** base64url of the 32-byte public key (`sessions.seal_public_key`). */
  publicKey: string
}

export function treeKeyFromSecret(secret: Uint8Array): TreeKey {
  return { secret: key32(secret, `tree secret`), publicKey: b64urlEncode(pushPublicKey(secret)) }
}

export function generateTreeKey(): TreeKey {
  return treeKeyFromSecret(randomBytes(32))
}

function publicKeyBytes(publicKey: string | Uint8Array, what: string): Uint8Array {
  const bytes = typeof publicKey === `string` ? b64urlDecode(publicKey) : publicKey
  if (!bytes || bytes.length !== 32) throw new Error(`${what} must be 32 bytes (base64url)`)
  return bytes
}

/** `wrapTreeKey` with a fixed ephemeral secret: test vectors only. */
export function wrapTreeKeyWith(ephSecret: Uint8Array, installPublic: string | Uint8Array, treeSecret: Uint8Array): string {
  return b64urlEncode(pushSealWith(ephSecret, publicKeyBytes(installPublic, `install public key`), key32(treeSecret, `tree secret`)))
}

/** `session_keys.wrapped` for one reader install (its `installs.x25519_public`). */
export function wrapTreeKey(installPublic: string | Uint8Array, treeSecret: Uint8Array): string {
  return wrapTreeKeyWith(randomBytes(32), installPublic, treeSecret)
}

/** The tree secret from a `session_keys.wrapped` value; throws when it does not open. */
export function unwrapTreeKey(installSecret: Uint8Array, wrapped: string): Uint8Array {
  const bytes = b64urlDecode(wrapped)
  if (!bytes || bytes.length !== WRAPPED_TREE_KEY_BYTES) throw new Error(`malformed wrapped tree key`)
  return pushOpen(installSecret, bytes)
}

// ── envelopes ───────────────────────────────────────────────────────────────

/** A body envelope = this prefix + a sealed value. */
export const SEALED_BODY_PREFIX = `exp-sealed:`
export const SEALED_VERSION = `v1`

/** `version:epoch:payload` (the card's `sealed`, the body after the prefix). */
export type SealedValue =
  | { kind: `sealed`; epoch: number; sealed: Uint8Array }
  | { kind: `unsupported`; version: string }
  | { kind: `malformed` }

export function parseSealedValue(value: string): SealedValue {
  const firstColon = value.indexOf(`:`)
  const version = firstColon < 0 ? value : value.slice(0, firstColon)
  if (version !== SEALED_VERSION) return { kind: `unsupported`, version }
  const rest = value.slice(firstColon + 1)
  const secondColon = rest.indexOf(`:`)
  if (secondColon < 0) return { kind: `malformed` }
  const epochText = rest.slice(0, secondColon)
  if (!/^[1-9][0-9]{0,9}$/.test(epochText)) return { kind: `malformed` }
  const epoch = Number(epochText)
  if (epoch > SEAL_EPOCH_MAX) return { kind: `malformed` }
  const sealed = b64urlDecode(rest.slice(secondColon + 1))
  if (!sealed || sealed.length < PUSH_OVERHEAD) return { kind: `malformed` }
  return { kind: `sealed`, epoch, sealed }
}

function sealedValue(epoch: number, sealed: Uint8Array): string {
  if (!Number.isInteger(epoch) || epoch < 1 || epoch > SEAL_EPOCH_MAX) throw new Error(`epoch must be 1…${SEAL_EPOCH_MAX}`)
  return `${SEALED_VERSION}:${epoch}:${b64urlEncode(sealed)}`
}

export type BodyEnvelope = { kind: `plain`; text: string } | SealedValue

/** A body without the prefix is plaintext (server lines, older rows). */
export function parseBodyEnvelope(body: string): BodyEnvelope {
  if (!body.startsWith(SEALED_BODY_PREFIX)) return { kind: `plain`, text: body }
  return parseSealedValue(body.slice(SEALED_BODY_PREFIX.length))
}

/** The tree secrets this install holds, by epoch. */
export type TreeKeys = ReadonlyMap<number, Uint8Array>

/** Why sealed content did not open: `noKey` = no secret for its epoch
 *  (yet), `unsupported` = a newer envelope version, `malformed` = not an
 *  envelope, `refused` = authentication failed. */
export type OpenStatus = `plain` | `opened` | `noKey` | `unsupported` | `malformed` | `refused`

export type OpenedBody =
  | { status: `plain` | `opened`; epoch: number | null; text: string }
  | { status: `noKey` | `unsupported` | `malformed` | `refused`; epoch: number | null; text: null }

const utf8 = new TextEncoder()
const strictUtf8 = new TextDecoder(`utf-8`, { fatal: true })

/** `sealBody` with a fixed ephemeral secret: test vectors only. */
export function sealBodyWith(ephSecret: Uint8Array, treePublic: string | Uint8Array, epoch: number, text: string): string {
  const sealed = pushSealWith(ephSecret, publicKeyBytes(treePublic, `tree public key`), utf8.encode(text))
  return SEALED_BODY_PREFIX + sealedValue(epoch, sealed)
}

/** `messages.body` / `sessions.sealed_title` sealed to the tree. */
export function sealBody(treePublic: string | Uint8Array, epoch: number, text: string): string {
  return sealBodyWith(randomBytes(32), treePublic, epoch, text)
}

function openValue(value: SealedValue, keys: TreeKeys): { status: Exclude<OpenStatus, `plain`>; epoch: number | null; bytes: Uint8Array | null } {
  if (value.kind !== `sealed`) return { status: value.kind, epoch: null, bytes: null }
  const secret = keys.get(value.epoch)
  if (!secret) return { status: `noKey`, epoch: value.epoch, bytes: null }
  try {
    return { status: `opened`, epoch: value.epoch, bytes: pushOpen(secret, value.sealed) }
  } catch {
    return { status: `refused`, epoch: value.epoch, bytes: null }
  }
}

export function openBody(body: string, keys: TreeKeys): OpenedBody {
  const envelope = parseBodyEnvelope(body)
  if (envelope.kind === `plain`) return { status: `plain`, epoch: null, text: envelope.text }
  const opened = openValue(envelope, keys)
  if (opened.status !== `opened`) return { status: opened.status, epoch: opened.epoch, text: null }
  try {
    return { status: `opened`, epoch: opened.epoch, text: strictUtf8.decode(opened.bytes!) }
  } catch {
    return { status: `refused`, epoch: opened.epoch, text: null }
  }
}

// ── cards ───────────────────────────────────────────────────────────────────

/** The fields of `messages.card` that stay in clear (routing, wake rules,
 *  answered state, the attachments a card shows); everything else exists
 *  only inside `sealed`. `pictures` stays in clear PROJECTED: each entry's
 *  `label` and `attachmentId` only (captions and sizes stay sealed; a
 *  label without an id = a picture still to upload). Opening fills a sealed
 *  picture's missing `attachmentId` from the clear entry of its label
 *  (`resolveClearPictures`); the clear part never replaces it. */
export const CARD_CLEAR_FIELDS = [
  `kind`,
  `messageId`,
  `sessionId`,
  `issueId`,
  `pullRequestId`,
  `event`,
  `status`,
  `final`,
  `decision`,
  `fresh`,
  `answered`,
  `pictures`,
] as const

/** The clear projection of a card's `pictures`: `{label, attachmentId}`
 *  per entry that names an attachment, `{label}` per entry still to upload
 *  (the server mints its upload and files it by label); null when none
 *  names either. */
export function clearPictures(pictures: unknown): Array<{ label?: string; attachmentId?: string }> | null {
  if (!Array.isArray(pictures)) return null
  const out: Array<{ label?: string; attachmentId?: string }> = []
  for (const picture of pictures) {
    if (!picture || typeof picture !== `object`) continue
    const { label, attachmentId } = picture as Record<string, unknown>
    const entry: { label?: string; attachmentId?: string } = {}
    if (typeof label === `string`) entry.label = label
    if (typeof attachmentId === `string`) entry.attachmentId = attachmentId
    if (entry.label !== undefined || entry.attachmentId !== undefined) out.push(entry)
  }
  return out.length > 0 ? out : null
}

/** A sealed card's pictures uploaded AFTER it was sealed: the server filed
 *  them on the clear `pictures` entry of their label. Each sealed picture
 *  without an `attachmentId` takes its label's clear id and, on a plan,
 *  every `](upload:<label>)` embed points at the attachment (plan-card.ts
 *  `resolveUpload`). A sealed id is never replaced. */
export function resolveClearPictures(card: Card, clear: unknown): Card {
  if (!Array.isArray(clear) || !Array.isArray(card.pictures)) return card
  const uploaded = new Map<string, string>()
  for (const entry of clear) {
    const { label, attachmentId } = (entry ?? {}) as Record<string, unknown>
    if (typeof label === `string` && typeof attachmentId === `string` && !uploaded.has(label)) uploaded.set(label, attachmentId)
  }
  if (uploaded.size === 0) return card
  const resolved: Array<[string, string]> = []
  const pictures = card.pictures.map((picture: unknown) => {
    if (!picture || typeof picture !== `object`) return picture
    const { label, attachmentId } = picture as Record<string, unknown>
    const id = typeof label === `string` && typeof attachmentId !== `string` ? uploaded.get(label) : undefined
    if (id === undefined) return picture
    resolved.push([label as string, id])
    return { ...(picture as Card), attachmentId: id }
  })
  if (resolved.length === 0) return card
  const next: Card = { ...card, pictures }
  if (card.kind === `plan` && typeof card.plan === `string`) {
    next.plan = resolved.reduce((plan, [label, id]) => plan.split(`](upload:${label})`).join(`](/api/attachments/${id})`), card.plan)
  }
  return next
}

type Card = Record<string, unknown>

/** True when the card carries a person's answer (`answer`/`answers`) or
 *  says so itself (`answered: true`). */
function cardAnswered(card: Card): boolean {
  return card.answered === true || (card.answer !== undefined && card.answer !== null) || (card.answers !== undefined && card.answers !== null)
}

/** The clear part (allowlisted fields that are set, `answered` only when
 *  true) and the plaintext the sealed part carries: the JSON of the FULL
 *  card (minus a stale `sealed`). */
export function splitCard(card: Card): { clear: Card; plaintext: string } {
  if (typeof card.kind !== `string`) throw new Error(`a card needs a kind`)
  const { sealed: _stale, ...full } = card
  const clear: Card = {}
  for (const field of CARD_CLEAR_FIELDS) {
    if (field === `answered`) {
      if (cardAnswered(full)) clear.answered = true
    } else if (field === `pictures`) {
      const pictures = clearPictures(full.pictures)
      if (pictures) clear.pictures = pictures
    } else if (full[field] !== undefined && full[field] !== null) {
      clear[field] = full[field]
    }
  }
  return { clear, plaintext: JSON.stringify(full) }
}

/** `sealCard` with a fixed ephemeral secret: test vectors only. */
export function sealCardWith(ephSecret: Uint8Array, treePublic: string | Uint8Array, epoch: number, card: Card): Card {
  const { clear, plaintext } = splitCard(card)
  const sealed = pushSealWith(ephSecret, publicKeyBytes(treePublic, `tree public key`), utf8.encode(plaintext))
  return { ...clear, sealed: sealedValue(epoch, sealed) }
}

/** `messages.card` sealed to the tree: `{...clear, sealed: "v1:<epoch>:<b64url>"}`. */
export function sealCard(treePublic: string | Uint8Array, epoch: number, card: Card): Card {
  return sealCardWith(randomBytes(32), treePublic, epoch, card)
}

/** `card` = the full card (plain / opened: the sealed card with the clear
 *  fields laid over it, since the server may update those) or, when it did
 *  not open, the clear part alone. */
export type OpenedCard = { status: OpenStatus; epoch: number | null; card: Card }

export function openCard(card: Card, keys: TreeKeys): OpenedCard {
  if (typeof card.sealed !== `string`) {
    if (`sealed` in card) {
      const { sealed: _bad, ...clear } = card
      return { status: `malformed`, epoch: null, card: clear }
    }
    return { status: `plain`, epoch: null, card }
  }
  const { sealed, ...clear } = card
  const opened = openValue(parseSealedValue(sealed as string), keys)
  if (opened.status !== `opened`) return { status: opened.status, epoch: opened.epoch, card: clear }
  let inner: unknown
  try {
    inner = JSON.parse(strictUtf8.decode(opened.bytes!))
  } catch {
    return { status: `refused`, epoch: opened.epoch, card: clear }
  }
  if (!inner || typeof inner !== `object` || Array.isArray(inner) || (inner as Card).kind !== clear.kind) {
    return { status: `refused`, epoch: opened.epoch, card: clear }
  }
  // The clear `pictures` is a projection: the sealed ones stay whole, and
  // only take the ids of pictures uploaded after sealing.
  const { pictures: projected, ...over } = clear
  return { status: `opened`, epoch: opened.epoch, card: resolveClearPictures({ ...(inner as Card), ...over }, projected) }
}

// ── the locked state (fixtures/sealed-content.json `placeholder`) ───────────

/** Why content stays locked here: `noKey` = waiting for a holder's wrap
 *  (opens by itself later), `unreadable` = it never opens on this install. */
export type LockedReason = `noKey` | `unreadable`

export const SEALED_PLACEHOLDER = placeholderFixture.placeholder as {
  icon: string
  copy: Record<LockedReason, string>
  reasons: Record<Exclude<OpenStatus, `plain` | `opened`>, LockedReason>
}

/** Null when the content opened (or was plaintext). */
export function lockedReason(status: OpenStatus): LockedReason | null {
  return status === `plain` || status === `opened` ? null : SEALED_PLACEHOLDER.reasons[status]
}

/** A message row opened with its tree's keys: `body` / `card` as the
 *  sender wrote them, or, when either stays sealed, `locked` with an empty
 *  body and the card's clear part (its kind and routing flags). */
export function openMessage<M extends { body: string; card: Card | null }>(
  message: M,
  keys: TreeKeys
): M & { locked: LockedReason | null } {
  const body = openBody(message.body, keys)
  const card = message.card ? openCard(message.card, keys) : null
  const reasons = [lockedReason(body.status), card ? lockedReason(card.status) : null]
  const locked = reasons.includes(`noKey`) ? `noKey` : reasons.includes(`unreadable`) ? `unreadable` : null
  return {
    ...message,
    body: locked ? `` : (body.text ?? ``),
    card: card ? card.card : null,
    locked,
  }
}

/** A sealed title opened, else null (callers keep the neutral `title`). */
export function openTitle(sealedTitle: string | null | undefined, keys: TreeKeys): string | null {
  if (!sealedTitle) return null
  const opened = openBody(sealedTitle, keys)
  return opened.status === `opened` || opened.status === `plain` ? opened.text : null
}
