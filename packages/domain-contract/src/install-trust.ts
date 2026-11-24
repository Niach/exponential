// #EXP-1950: which installs an install TRUSTS, and what it does with a key
// the server hands it (fixtures/install-trust.json is the spec and holds the
// vectors; fixtures/wrap-signature.json the wrap signature it builds on).
//
// - THE CODE an install shows while it waits for approval, and the row of a
//   trusted install shows beside it: `installCode`.
// - THE APPROVALS, signed by an install's eNet identity: `approvalMessage`.
// - THE PIN STORE of one install (per account, beside its tree keys) and
//   how it follows the server's answers: `reconcileTrust`. The server only
//   ever SHRINKS the pins (a revoked install, a removed member); it grows
//   them only with a signature this install checks itself.
// - THE FOUR RULES, record-only in release 1 (`TRUST_MODE`): `keepWrap`
//   (R1), `sealEpochCheck` (R2), `sealTo` (R3), `wrapTarget` (R4) and
//   `callKey` (a call's key). Each answers what a build does NOW and records,
//   and what it does once signing is mandatory (decision d).
// Mirrors: desktop/CLI (enet-core), iOS, Android replay the fixture.
// Pure JS (@noble), so it runs in the browser too.

import { ed25519 } from "@noble/curves/ed25519"
import { hkdf } from "@noble/hashes/hkdf.js"
import { sha256 } from "@noble/hashes/sha2.js"
import { b64urlDecode, b64urlEncode, pushPublicKey, treeKeyFromSecret } from "./sealed-content"
import { acceptVouch, acceptWrap, type ServedWrap, type WrapRefusal, type WrapStatement } from "./wrap-signature"
import fixture from "../fixtures/install-trust.json" with { type: "json" }

/** The approval rows' and the waiting state's copy (`copy`), the same ×4. */
export const INSTALL_TRUST_COPY = fixture.copy as {
  newInstall: string
  reset: string
  approve: string
  remove: string
  waiting: string
}

// ── the code ──────────────────────────────────────────────────────────────

export const INSTALL_CODE_LABEL = `exp/enet/install-code/1`
const CROCKFORD = `0123456789ABCDEFGHJKMNPQRSTVWXYZ`
const ENDPOINT_ID_RE = /^[0-9a-f]{64}$/
const SIGNATURE_RE = /^[A-Za-z0-9_-]{85}[AQgw]$/
const enc = new TextEncoder()

function hexBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2)
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(2 * i, 2 * i + 2), 16)
  return out
}

function hex(bytes: Uint8Array): string {
  return [...bytes].map((b) => b.toString(16).padStart(2, `0`)).join(``)
}

/** The 10 characters (two groups of five) an install shows to be approved:
 *  the first 50 bits of SHA-256("exp/enet/install-code/1" ‖ 0x00 ‖
 *  endpointId[32]) in Crockford base32. Both ends derive it from the key;
 *  never read from the server. Null for a malformed endpoint id. */
export function installCode(endpointId: string): string | null {
  if (!ENDPOINT_ID_RE.test(endpointId)) return null
  const digest = sha256(new Uint8Array([...enc.encode(INSTALL_CODE_LABEL), 0, ...hexBytes(endpointId)]))
  let bits = 0n
  for (let i = 0; i < 7; i++) bits = (bits << 8n) | BigInt(digest[i]!)
  bits >>= 6n // 56 bits read, the first 50 kept
  let out = ``
  for (let i = 9; i >= 0; i--) out += CROCKFORD[Number((bits >> BigInt(5 * i)) & 31n)]
  return `${out.slice(0, 5)} ${out.slice(5)}`
}

// ── identities, registrations, approvals ──────────────────────────────────

/** An install's X25519 secret: HKDF-SHA256(identity, no salt, `exp/enet/push-x25519/1`). */
export function installSecretOf(identity: Uint8Array): Uint8Array {
  return hkdf(sha256, identity, new Uint8Array(0), enc.encode(`exp/enet/push-x25519/1`), 32)
}

/** Its public half (base64url), the key wraps are sealed to. */
export function installX25519Of(identity: Uint8Array): string {
  return b64urlEncode(pushPublicKey(installSecretOf(identity)))
}

/** Strict ed25519 (as wrap-signature.ts). Never throws. */
function verifyStrict(signature: string, message: Uint8Array, endpointId: string): boolean {
  if (!SIGNATURE_RE.test(signature) || !ENDPOINT_ID_RE.test(endpointId)) return false
  const bytes = b64urlDecode(signature)
  if (bytes?.length !== 64) return false
  try {
    const key = hexBytes(endpointId)
    if (ed25519.ExtendedPoint.fromHex(key, false).isSmallOrder()) return false
    if (ed25519.ExtendedPoint.fromHex(bytes.subarray(0, 32), false).isSmallOrder()) return false
    return ed25519.verify(bytes, message, key, { zip215: false })
  } catch {
    return false
  }
}

/** `installs.register`'s message (install-register.json). */
export function registrationMessage(userId: string, endpointId: string, x25519Public: string, signedAt: string): string {
  return `exp-install-v1\n${userId}\n${endpointId}\n${x25519Public}\n${signedAt}`
}

export function signRegistration(identity: Uint8Array, userId: string, signedAt: string): string {
  const endpointId = hex(ed25519.getPublicKey(identity))
  return b64urlEncode(ed25519.sign(enc.encode(registrationMessage(userId, endpointId, installX25519Of(identity), signedAt)), identity))
}

export const APPROVAL_LABEL = `exp/enet/install-approval/1`
export const APPROVAL_MESSAGE_BYTES = 125

/** What an approval says. `root` = an install starts its account's chain
 *  (signed by itself: the first install, or a reset); `approve` = a trusted
 *  install of the SAME account approves it; `reset` = a team-mate's trusted
 *  install confirms a member's new chain; `decline` = a team-mate's install
 *  turns that reset down (the row's Remove). */
export type ApprovalKind = `root` | `approve` | `reset` | `decline`
export const APPROVAL_KINDS: Record<ApprovalKind, number> = { root: 1, approve: 2, reset: 3, decline: 4 }

/** The install an approval is about, as the registration binds it. */
export interface ApprovalSubject {
  userId: string
  endpointId: string
  x25519Public: string
}

/** "exp/enet/install-approval/1" ‖ 0x00 ‖ kind u8 ‖ SHA-256(userId utf8)[32]
 *  ‖ endpointId[32] ‖ x25519Public[32] (125 bytes); null when malformed. */
export function approvalMessage(kind: ApprovalKind, subject: ApprovalSubject): Uint8Array | null {
  const x = b64urlDecode(subject.x25519Public)
  if (!ENDPOINT_ID_RE.test(subject.endpointId) || x?.length !== 32 || subject.userId.length === 0) return null
  const label = enc.encode(APPROVAL_LABEL)
  const out = new Uint8Array(APPROVAL_MESSAGE_BYTES)
  out.set(label, 0)
  out[label.length] = 0
  out[label.length + 1] = APPROVAL_KINDS[kind]
  out.set(sha256(enc.encode(subject.userId)), label.length + 2)
  out.set(hexBytes(subject.endpointId), label.length + 34)
  out.set(x, label.length + 66)
  return out
}

export function signApproval(identity: Uint8Array, kind: ApprovalKind, subject: ApprovalSubject): string {
  const message = approvalMessage(kind, subject)
  if (!message) throw new Error(`malformed approval subject`)
  return b64urlEncode(ed25519.sign(message, identity))
}

export function verifyApproval(kind: ApprovalKind, subject: ApprovalSubject, signature: string, approverEndpointId: string): boolean {
  const message = approvalMessage(kind, subject)
  return !!message && verifyStrict(signature, message, approverEndpointId)
}

// ── what the server serves (`installs.trust`) ─────────────────────────────

/** A live install of the viewer's account or of a team-mate. */
export interface ServedInstall {
  installId: string
  userId: string
  endpointId: string
  x25519Public: string
  registrationSignature: string | null
  registrationSignedAt: string | null
  /** A build that follows the rule asked to be approved (`installs.requestApproval`). */
  approvalRequested: boolean
  label?: string | null
}

export interface ServedApproval {
  kind: ApprovalKind
  subjectEndpointId: string
  approverEndpointId: string
  signature: string
}

/** Whether the registration binds the row's user, endpoint id and X25519 key. */
export function registrationValid(install: ServedInstall): boolean {
  if (!install.registrationSignature || !install.registrationSignedAt) return false
  const message = enc.encode(registrationMessage(install.userId, install.endpointId, install.x25519Public, install.registrationSignedAt))
  return verifyStrict(install.registrationSignature, message, install.endpointId)
}

// ── the pin store ─────────────────────────────────────────────────────────

/** Why an install is pinned: `self` = this install; `chain` = on the path
 *  from its account's root to this install when it joined; `approved` = an
 *  `approve` by a pinned install of the same account; `first` = a
 *  team-mate's root at first contact; `reset` = a team-mate's new root a
 *  pinned install confirmed. */
export type PinVia = `self` | `chain` | `approved` | `first` | `reset`

export interface Pin {
  userId: string
  x25519Public: string
  via: PinVia
  /** The approver (`approved`, `reset`); null otherwise. */
  by: string | null
}

/** What this install recorded about a key it holds. */
export type KeyRecord = `verified` | `legacy` | `unsigned` | `refused-would-be`

export interface TrustStore {
  /** This install's endpoint id and account. */
  self: string
  userId: string
  /** Placed in its account's chain (a root, or approved). */
  joined: boolean
  pins: Record<string, Pin>
  /** Every team-mate ever pinned: a new root of one is a Reset, not a first contact. */
  members: string[]
  /** Endpoint ids this install's account turned down (a Reset row's Remove). */
  declined: string[]
  /** Per root: the highest VERIFIED epoch and what each held epoch is. */
  roots: Record<string, { verifiedEpoch: number; epochs: Record<string, KeyRecord> }>
}

export function emptyTrustStore(self: string, userId: string): TrustStore {
  return { self, userId, joined: false, pins: {}, members: [], declined: [], roots: {} }
}

/** A row of Needs you: approve or remove an install. */
export interface TrustRow {
  kind: `newInstall` | `reset`
  installId: string
  endpointId: string
  userId: string
  label: string | null
  code: string
}

/** `trusted` = joined; `waiting` = shows "Waiting for approval" and its
 *  code; `claimRoot` = its account has no chain: it signs `root` for itself. */
export type TrustStatus = `trusted` | `waiting` | `claimRoot`

export interface TrustView {
  store: TrustStore
  status: TrustStatus
  rows: TrustRow[]
}

/**
 * Follows the server's answer (`installs.trust`: every live install of the
 * account and of its team-mates, and their approvals). The server may only
 * SHRINK the pins: a pin missing from the answer, or whose registration no
 * longer verifies, is dropped; a pin is only ever ADDED for a signature
 * this install verified, by an install already pinned (or, once, at
 * joining and at first contact: decided in the `$comment`).
 */
export function reconcileTrust(before: TrustStore, served: { installs: ServedInstall[]; approvals: ServedApproval[] }): TrustView {
  const store: TrustStore = {
    ...before,
    pins: { ...before.pins },
    members: [...before.members],
    declined: [...before.declined],
    roots: before.roots,
  }
  const valid = new Map<string, ServedInstall>()
  for (const install of served.installs) if (registrationValid(install)) valid.set(install.endpointId, install)
  const me = valid.get(store.self)

  // Shrink: what the server no longer lists (or binds) is no longer pinned.
  for (const endpointId of Object.keys(store.pins)) {
    const row = valid.get(endpointId)
    if (endpointId === store.self) continue
    if (!row || row.userId !== store.pins[endpointId]!.userId || row.x25519Public !== store.pins[endpointId]!.x25519Public) {
      delete store.pins[endpointId]
    }
  }
  if (me) store.pins[store.self] = { userId: store.userId, x25519Public: me.x25519Public, via: `self`, by: null }

  // The approvals that verify, against the subject as its registration binds it.
  const approvals = served.approvals.filter((approval) => {
    const subject = valid.get(approval.subjectEndpointId)
    if (!subject) return false
    if (approval.kind === `root` && approval.approverEndpointId !== approval.subjectEndpointId) return false
    return verifyApproval(approval.kind, subject, approval.signature, approval.approverEndpointId)
  })
  const isRoot = (endpointId: string) => approvals.some((a) => a.kind === `root` && a.subjectEndpointId === endpointId)
  const userOf = (endpointId: string) => valid.get(endpointId)?.userId

  // Joining: this install's own root claim, or the chain from its account's root to it.
  if (!store.joined && me) {
    if (isRoot(store.self)) {
      store.joined = true
    } else {
      const path = chainTo(store.self, store.userId, approvals, isRoot, userOf)
      if (path) {
        for (const endpointId of path) {
          if (endpointId === store.self) continue
          const row = valid.get(endpointId)!
          store.pins[endpointId] = { userId: row.userId, x25519Public: row.x25519Public, via: `chain`, by: null }
        }
        store.joined = true
      }
    }
  }

  // Declines by this account's pinned installs.
  for (const approval of approvals) {
    if (approval.kind !== `decline`) continue
    const approver = store.pins[approval.approverEndpointId]
    if (approver?.userId === store.userId && !store.declined.includes(approval.subjectEndpointId)) store.declined.push(approval.subjectEndpointId)
  }

  if (store.joined) {
    // First contact with a team-mate: their root(s) as served, once.
    for (const install of valid.values()) {
      if (install.userId === store.userId || store.members.includes(install.userId)) continue
      if (Object.values(store.pins).some((pin) => pin.userId === install.userId)) continue
      for (const root of valid.values()) {
        if (root.userId === install.userId && isRoot(root.endpointId) && !store.declined.includes(root.endpointId)) {
          store.pins[root.endpointId] = { userId: root.userId, x25519Public: root.x25519Public, via: `first`, by: null }
        }
      }
    }
    // Grow by signatures of pinned installs, to a fixpoint.
    for (let changed = true; changed; ) {
      changed = false
      for (const approval of approvals) {
        const subject = valid.get(approval.subjectEndpointId)!
        if (store.pins[subject.endpointId] || store.declined.includes(subject.endpointId)) continue
        const approver = store.pins[approval.approverEndpointId]
        if (!approver) continue
        const pinned =
          (approval.kind === `approve` && approver.userId === subject.userId) ||
          (approval.kind === `reset` && approver.userId !== subject.userId && subject.userId !== store.userId && isRoot(subject.endpointId))
        if (!pinned) continue
        store.pins[subject.endpointId] = {
          userId: subject.userId,
          x25519Public: subject.x25519Public,
          via: approval.kind === `approve` ? `approved` : `reset`,
          by: approval.approverEndpointId,
        }
        changed = true
      }
    }
    for (const pin of Object.values(store.pins)) if (pin.userId !== store.userId && !store.members.includes(pin.userId)) store.members.push(pin.userId)
  }

  const rows: TrustRow[] = []
  if (store.joined) {
    for (const install of valid.values()) {
      if (store.pins[install.endpointId] || store.declined.includes(install.endpointId)) continue
      const own = install.userId === store.userId
      // Own: a build that asked, or a competing root. A team-mate's: only a
      // new root of a member pinned before (their own installs approve the rest).
      const kind = own
        ? install.approvalRequested || isRoot(install.endpointId)
          ? (`newInstall` as const)
          : null
        : isRoot(install.endpointId) && store.members.includes(install.userId)
          ? (`reset` as const)
          : null
      if (kind) {
        rows.push({ kind, installId: install.installId, endpointId: install.endpointId, userId: install.userId, label: install.label ?? null, code: installCode(install.endpointId)! })
      }
    }
    rows.sort((a, b) => (a.kind === b.kind ? (a.endpointId < b.endpointId ? -1 : 1) : a.kind === `newInstall` ? -1 : 1))
  }

  const ownChain = [...valid.values()].some(
    (install) => install.userId === store.userId && approvals.some((a) => (a.kind === `root` || a.kind === `approve`) && a.subjectEndpointId === install.endpointId)
  )
  const status: TrustStatus = store.joined ? `trusted` : ownChain ? `waiting` : `claimRoot`
  return { store, status, rows }
}

/** The approvals from a root of `userId` down to `target` (`approve` only,
 *  every approver of `userId`), shortest first; null when none. */
function chainTo(
  target: string,
  userId: string,
  approvals: ServedApproval[],
  isRoot: (endpointId: string) => boolean,
  userOf: (endpointId: string) => string | undefined
): string[] | null {
  const parents = new Map<string, string>()
  const roots = [...new Set(approvals.filter((a) => a.kind === `root` && userOf(a.subjectEndpointId) === userId).map((a) => a.subjectEndpointId))].sort()
  const queue = [...roots]
  const seen = new Set(roots)
  while (queue.length > 0) {
    const at = queue.shift()!
    if (at === target) {
      const path = [at]
      while (parents.has(path[0]!)) path.unshift(parents.get(path[0]!)!)
      return path
    }
    for (const approval of approvals) {
      if (approval.kind !== `approve` || approval.approverEndpointId !== at || seen.has(approval.subjectEndpointId)) continue
      if (userOf(approval.subjectEndpointId) !== userId || isRoot(approval.subjectEndpointId)) continue
      seen.add(approval.subjectEndpointId)
      parents.set(approval.subjectEndpointId, at)
      queue.push(approval.subjectEndpointId)
    }
  }
  return null
}

/** The endpoint ids an install takes wraps and vouches from. */
export function pinnedSigners(store: TrustStore): string[] {
  return Object.keys(store.pins)
}

// ── the four rules ────────────────────────────────────────────────────────

/** Release 1 = `record`: nothing a released build makes is refused;
 *  `mandatory` = release 2 of decision (d), a BUILD constant. */
export type TrustMode = `record` | `mandatory`
export const TRUST_MODE: TrustMode = `record`

export type KeepVerdict =
  | { keep: true; record: KeyRecord; secret: Uint8Array; treePublicKey: string }
  | { keep: false; reason: WrapRefusal }

/**
 * R1: whether this install keeps a served wrap (of ANY epoch: old content
 * stays readable; R2 is about sealing). A signature that fails, a malformed
 * wrap and one that does not open are refused in every mode. In `record` mode an unsigned wrap is kept as
 * `unsigned` and one signed by an UNPINNED install that verifies as
 * `refused-would-be`; `mandatory` refuses both.
 */
export function keepWrap(
  wrap: ServedWrap,
  own: { identity: Uint8Array; installSecret: Uint8Array },
  store: TrustStore,
  mode: TrustMode
): KeepVerdict {
  const verdict = acceptWrap(wrap, own, pinnedSigners(store), mode === `mandatory`)
  if (verdict.status === `verified`) return { keep: true, record: `verified`, secret: verdict.secret, treePublicKey: verdict.treePublicKey }
  if (verdict.status === `unsigned`) return { keep: true, record: `unsigned`, secret: verdict.secret, treePublicKey: verdict.treePublicKey }
  if (verdict.reason === `unpinned` && mode === `record`) {
    // Is it at least its signer's? Then it is kept, and recorded.
    const signer = wrap.signerEndpointId!
    const again = acceptWrap(wrap, own, [signer], false)
    if (again.status === `verified`) return { keep: true, record: `refused-would-be`, secret: again.secret, treePublicKey: again.treePublicKey }
    return { keep: false, reason: again.status === `refused` ? again.reason : `signature` }
  }
  return { keep: false, reason: verdict.reason }
}

/** Records a kept key in the store (`legacy` = held before the store existed, decision c). */
export function recordKey(store: TrustStore, rootId: string, epoch: number, record: KeyRecord): TrustStore {
  const root = store.roots[rootId] ?? { verifiedEpoch: 0, epochs: {} }
  const next = {
    verifiedEpoch: record === `verified` ? Math.max(root.verifiedEpoch, epoch) : root.verifiedEpoch,
    epochs: { ...root.epochs, [String(epoch)]: record },
  }
  return { ...store, roots: { ...store.roots, [rootId]: next } }
}

/** R2: whether a server answer naming `epoch` of `rootId` may be sealed to
 *  (`rollback` in every mode when this install verified a higher one). */
export function sealEpochCheck(store: TrustStore, rootId: string, epoch: number): `ok` | `rollback` {
  return epoch < (store.roots[rootId]?.verifiedEpoch ?? 0) ? `rollback` : `ok`
}

export type SealVerdict = { seal: true; record: KeyRecord } | { seal: false; reason: WrapRefusal | `rollback` | `keyMismatch` }

/**
 * R3: whether this install seals to (root, epoch, public key) as the server
 * names it. With a held secret of that epoch: only to ITS public key (the
 * held key's record). Without: a vouch (a signed wrap of that epoch for
 * another install) by a pinned install = `verified`; no vouch = `unsigned`
 * and one by an unpinned install that verifies = `refused-would-be`, both
 * sealed in `record` mode and refused (the send waits as `noKey`) once
 * `mandatory`; a vouch whose signature fails is refused in every mode.
 */
export function sealTo(
  input: {
    rootId: string
    epoch: number
    publicKey: string
    held?: { secret: Uint8Array; record: KeyRecord } | null
    vouch?: { recipientEndpointId: string; wrapped: string; signature: string; signerEndpointId: string } | null
  },
  store: TrustStore,
  mode: TrustMode
): SealVerdict {
  if (sealEpochCheck(store, input.rootId, input.epoch) === `rollback`) return { seal: false, reason: `rollback` }
  if (input.held) {
    if (treeKeyFromSecret(input.held.secret).publicKey !== input.publicKey) return { seal: false, reason: `keyMismatch` }
    return mode === `mandatory` && (input.held.record === `unsigned` || input.held.record === `refused-would-be`)
      ? { seal: false, reason: input.held.record === `unsigned` ? `unsigned` : `unpinned` }
      : { seal: true, record: input.held.record }
  }
  const vouch = input.vouch ?? null
  const statement: WrapStatement | null = vouch
    ? { rootId: input.rootId, epoch: input.epoch, treePublicKey: input.publicKey, recipientEndpointId: vouch.recipientEndpointId, wrapped: vouch.wrapped }
    : null
  const verdict = statement ? acceptVouch(statement, vouch!.signature, vouch!.signerEndpointId, pinnedSigners(store)) : `unsigned`
  if (verdict === `verified`) return { seal: true, record: `verified` }
  if (verdict === `unsigned`) return mode === `record` ? { seal: true, record: `unsigned` } : { seal: false, reason: `unsigned` }
  if (verdict === `unpinned`) {
    const own = acceptVouch(statement!, vouch!.signature, vouch!.signerEndpointId, [vouch!.signerEndpointId])
    if (own !== `verified`) return { seal: false, reason: own }
    return mode === `record` ? { seal: true, record: `refused-would-be` } : { seal: false, reason: `unpinned` }
  }
  return { seal: false, reason: verdict }
}

export type TargetVerdict = { wrap: true; record: `verified` | `legacy` } | { wrap: false; reason: `unapproved` | `keyMismatch` | `registration` | `notTrusted` }

/**
 * R4: whether this install wraps a key for a target the server names
 * (`pending`, a new key's readers, a rotation, a new root's seal). Only a
 * trusted (joined) install wraps for others. A pinned target with its pinned
 * key = `verified`. A target that ASKED to be approved and is not pinned is
 * refused in every mode (that is the waiting state). A target that never
 * asked and claims no root is an older build: `legacy`, wrapped in `record`
 * mode, refused once `mandatory`. A registration that does not verify, or
 * a pinned endpoint served with another key, is refused in every mode.
 */
export function wrapTarget(target: ServedInstall, store: TrustStore, mode: TrustMode, roots: ReadonlySet<string> = new Set()): TargetVerdict {
  if (target.endpointId === store.self) return { wrap: true, record: `verified` }
  if (!store.joined) return { wrap: false, reason: `notTrusted` }
  const pin = store.pins[target.endpointId]
  if (pin) {
    return pin.x25519Public === target.x25519Public && pin.userId === target.userId ? { wrap: true, record: `verified` } : { wrap: false, reason: `keyMismatch` }
  }
  // An older build that registered before #EXP-1950 kept no signature: legacy too.
  if (target.registrationSignature !== null && !registrationValid(target)) return { wrap: false, reason: `registration` }
  if (target.approvalRequested || roots.has(target.endpointId)) return { wrap: false, reason: `unapproved` }
  return mode === `record` ? { wrap: true, record: `legacy` } : { wrap: false, reason: `unapproved` }
}

export type CallKeyVerdict = { key: Uint8Array; record: KeyRecord } | { noKey: `noKey` | `rollback` }

/**
 * A call's key (attack 4): the secret of the (root, epoch) `sendTarget`
 * names, from this install's KEYRING only (never a key the call itself is
 * handed). `record` mode uses any held key and the record says what it is;
 * `mandatory` only `verified` and `legacy` ones; else the call ends `no_key`.
 */
export function callKey(
  target: { rootId: string; epoch: number },
  held: { secret: Uint8Array; record: KeyRecord } | null | undefined,
  store: TrustStore,
  mode: TrustMode
): CallKeyVerdict {
  if (sealEpochCheck(store, target.rootId, target.epoch) === `rollback`) return { noKey: `rollback` }
  if (!held) return { noKey: `noKey` }
  if (mode === `mandatory` && held.record !== `verified` && held.record !== `legacy`) return { noKey: `noKey` }
  return { key: held.secret, record: held.record }
}
