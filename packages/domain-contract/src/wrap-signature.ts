// #EXP-1950: signed tree key wraps, TS reference (fixtures/wrap-signature.json
// is the spec and holds the vectors). The install that makes a wrap
// (`session_keys.wrapped`, sealed-content.ts `wrapTreeKey`) signs it with its
// eNet identity, the ed25519 key whose public half is `installs.endpoint_id`;
// an install keeps a wrap only when an install it has PINNED signed it.
//
//   message   = "exp/enet/tree-wrap/1" ‖ 0x00 ‖ rootId[16] ‖ epoch u32 BE
//               ‖ treePublic[32] ‖ recipient[32] ‖ wrapped[81]      (186 bytes)
//   signature = ed25519(signer identity, message)                   (RFC 8032, 64 bytes)
//
// `rootId` = the UUID's 16 bytes; `treePublic` = the X25519 public half of
// that epoch; `recipient` = the recipient install's ed25519 public key (its
// endpoint id, never the row id: the server assigns those). Verification is
// strict, as iroh's (`verify_strict`): canonical encodings, and a low-order
// signer key or R is refused.
// Nothing here is wired into a holder yet: no platform signs or checks a
// wrap until the builds of #EXP-1950 follow, and who is pinned is theirs.
// Mirror: enet-core `wrap_sig` (Rust; the phones through exponential-net-ffi).
// Pure JS (@noble), so it runs in the browser too.

import { ed25519 } from "@noble/curves/ed25519"
import {
  b64urlDecode,
  b64urlEncode,
  pushPublicKey,
  SEAL_EPOCH_MAX,
  unwrapTreeKey,
  WRAPPED_TREE_KEY_BYTES,
} from "./sealed-content"

/** The constants of wrap-signature.json the bytes hang on. */
export const WRAP_SIGNATURE = {
  label: `exp/enet/tree-wrap/1`,
  messageBytes: 186,
  signatureBytes: 64,
} as const

const LABEL = new Uint8Array([...new TextEncoder().encode(WRAP_SIGNATURE.label), 0])
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
const ENDPOINT_ID_RE = /^[0-9a-f]{64}$/
// 64 bytes as unpadded base64url = 86 chars (the last carries 2 bits).
const SIGNATURE_RE = /^[A-Za-z0-9_-]{85}[AQgw]$/

/** What a wrap signature covers. */
export interface WrapStatement {
  /** The tree's root id (a UUID, any casing). */
  rootId: string
  epoch: number
  /** base64url of that epoch's 32-byte tree public key. */
  treePublicKey: string
  /** The recipient install's endpoint id (lowercase hex of its ed25519 public key). */
  recipientEndpointId: string
  /** `session_keys.wrapped` (base64url of 81 bytes). */
  wrapped: string
}

function hexBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2)
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(2 * i, 2 * i + 2), 16)
  return out
}

function hex(bytes: Uint8Array): string {
  return [...bytes].map((b) => b.toString(16).padStart(2, `0`)).join(``)
}

/** The 186 bytes a wrap's signer signs; null when a field is malformed. */
export function wrapSignatureMessage(statement: WrapStatement): Uint8Array | null {
  if (!UUID_RE.test(statement.rootId)) return null
  if (!Number.isInteger(statement.epoch) || statement.epoch < 1 || statement.epoch > SEAL_EPOCH_MAX) return null
  if (!ENDPOINT_ID_RE.test(statement.recipientEndpointId)) return null
  const treePublic = b64urlDecode(statement.treePublicKey)
  const wrapped = b64urlDecode(statement.wrapped)
  if (treePublic?.length !== 32 || wrapped?.length !== WRAPPED_TREE_KEY_BYTES) return null
  const out = new Uint8Array(WRAP_SIGNATURE.messageBytes)
  out.set(LABEL, 0)
  out.set(hexBytes(statement.rootId.replace(/-/g, ``).toLowerCase()), 21)
  new DataView(out.buffer).setUint32(37, statement.epoch, false)
  out.set(treePublic, 41)
  out.set(hexBytes(statement.recipientEndpointId), 73)
  out.set(wrapped, 105)
  return out
}

/** An install's endpoint id: lowercase hex of its identity's ed25519 public key. */
export function endpointIdOf(identity: Uint8Array): string {
  return hex(ed25519.getPublicKey(identity))
}

/** `session_keys.signature`: the identity's signature over the statement
 *  (base64url, 86 chars); throws on a malformed statement. */
export function signWrap(identity: Uint8Array, statement: WrapStatement): string {
  const message = wrapSignatureMessage(statement)
  if (!message) throw new Error(`malformed wrap statement`)
  return b64urlEncode(ed25519.sign(message, identity))
}

/** Strict ed25519 (iroh's `verify_strict`): canonical S, R and key, and
 *  neither R nor the key of low order. Never throws. */
function verifyStrict(signature: Uint8Array, message: Uint8Array, publicKey: Uint8Array): boolean {
  try {
    if (ed25519.ExtendedPoint.fromHex(publicKey, false).isSmallOrder()) return false
    if (ed25519.ExtendedPoint.fromHex(signature.subarray(0, 32), false).isSmallOrder()) return false
    return ed25519.verify(signature, message, publicKey, { zip215: false })
  } catch {
    return false
  }
}

/** Whether `signature` is `signerEndpointId`'s over the statement. Says
 *  nothing about the signer being trusted: `acceptWrap` asks the pins. */
export function verifyWrapSignature(statement: WrapStatement, signature: string, signerEndpointId: string): boolean {
  const message = wrapSignatureMessage(statement)
  if (!message || !SIGNATURE_RE.test(signature) || !ENDPOINT_ID_RE.test(signerEndpointId)) return false
  const bytes = b64urlDecode(signature)
  return bytes?.length === WRAP_SIGNATURE.signatureBytes && verifyStrict(bytes, message, hexBytes(signerEndpointId))
}

/** Why a wrap is not kept. */
export type WrapRefusal =
  /** A field is not what the wire allows, or a signature came without its signer (or the reverse). */
  | `malformed`
  /** No signature, where one is required. */
  | `unsigned`
  /** The signer is not a pinned install. */
  | `unpinned`
  /** It does not open with this install's secret. */
  | `unopened`
  /** The signature is not the signer's over this root, epoch, key, recipient and bytes. */
  | `signature`

/** A wrap as `sessionKeys.mine` answers it. */
export interface ServedWrap {
  rootId: string
  epoch: number
  wrapped: string
  signature?: string | null
  signerEndpointId?: string | null
}

export type WrapVerdict =
  | { status: `verified`; secret: Uint8Array; treePublicKey: string; signerEndpointId: string }
  /** Kept without a signature (only where none is required): an older build made it. */
  | { status: `unsigned`; secret: Uint8Array; treePublicKey: string }
  | { status: `refused`; reason: WrapRefusal }

/**
 * Whether this install keeps a served wrap. The install builds the signed
 * message ITSELF: the root and epoch it asked for, the public key of the
 * secret it unwrapped, its own endpoint id, the wrapped bytes. `pins` = the
 * endpoint ids of the installs it has pinned. A wrap that carries a
 * signature must pass with it, also where none is `required`.
 */
export function acceptWrap(
  wrap: ServedWrap,
  own: { identity: Uint8Array; installSecret: Uint8Array },
  pins: Iterable<string>,
  required: boolean
): WrapVerdict {
  const refused = (reason: WrapRefusal): WrapVerdict => ({ status: `refused`, reason })
  const signature = wrap.signature ?? null
  const signer = wrap.signerEndpointId ?? null
  const decoded = b64urlDecode(wrap.wrapped)
  if (
    !UUID_RE.test(wrap.rootId) ||
    !Number.isInteger(wrap.epoch) ||
    wrap.epoch < 1 ||
    wrap.epoch > SEAL_EPOCH_MAX ||
    decoded?.length !== WRAPPED_TREE_KEY_BYTES ||
    (signature === null) !== (signer === null) ||
    (signature !== null && !SIGNATURE_RE.test(signature)) ||
    (signer !== null && !ENDPOINT_ID_RE.test(signer))
  ) {
    return refused(`malformed`)
  }
  if (signature === null || signer === null) {
    if (required) return refused(`unsigned`)
  } else if (![...pins].includes(signer)) {
    return refused(`unpinned`)
  }
  let secret: Uint8Array
  try {
    secret = unwrapTreeKey(own.installSecret, wrap.wrapped)
  } catch {
    return refused(`unopened`)
  }
  const treePublicKey = b64urlEncode(pushPublicKey(secret))
  if (signature === null || signer === null) return { status: `unsigned`, secret, treePublicKey }
  const statement: WrapStatement = {
    rootId: wrap.rootId,
    epoch: wrap.epoch,
    treePublicKey,
    recipientEndpointId: endpointIdOf(own.identity),
    wrapped: wrap.wrapped,
  }
  if (!verifyWrapSignature(statement, signature, signer)) return refused(`signature`)
  return { status: `verified`, secret, treePublicKey, signerEndpointId: signer }
}

/**
 * Whether an install that holds NO secret of (root, epoch) may seal to
 * `statement.treePublicKey`: a signed wrap of that epoch made for another
 * install vouches for the key. `verified`, or why not (never `unopened`).
 */
export function acceptVouch(
  statement: WrapStatement,
  signature: string | null | undefined,
  signerEndpointId: string | null | undefined,
  pins: Iterable<string>
): `verified` | WrapRefusal {
  if (
    !wrapSignatureMessage(statement) ||
    (signature != null && !SIGNATURE_RE.test(signature)) ||
    (signerEndpointId != null && !ENDPOINT_ID_RE.test(signerEndpointId)) ||
    (signature == null) !== (signerEndpointId == null)
  ) {
    return `malformed`
  }
  if (signature == null || signerEndpointId == null) return `unsigned`
  if (![...pins].includes(signerEndpointId)) return `unpinned`
  return verifyWrapSignature(statement, signature, signerEndpointId) ? `verified` : `signature`
}
