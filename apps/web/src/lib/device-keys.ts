import { createPublicKey, verify } from "node:crypto"
import { and, eq, ne, sql } from "drizzle-orm"
import { deviceFirstSeen, deviceKeys, devices } from "@/db/schema"
import type { Tx } from "@/lib/sessions/tx"

// FEED-154: proof of possession for a machine id. A machine's id shows in
// team-visible sessions, so anyone can register it; what they cannot do is
// sign with the machine's private key. The CLI daemon keeps an ed25519 key
// in its data dir (`cli-device-key`) and signs every `devices.register`
// with it (`deviceKeyMessage`); the server stores the proven public key on
// the caller's row and PINS the first proven key per machine id
// (`device_keys`, trust-on-first-use). A share needs the row's proven key to
// equal the pin (`devices.setShared`), so a squat of the id never shares.
//
// FEED-159: WHO may pin. The id becomes visible (team sessions) only after
// the real daemon registered it, so the account that registered the id
// FIRST (`device_first_seen`, written on every register, never deleted) is
// the owner signal: a first pin is that account's only, and that account
// re-pins over a pin another account took first (a squat that beat the
// owner's upgrade), which withdraws the other account's shares like any
// squat. Not the eldest `devices` row: Remove deletes the owner's row and
// the relaunch makes a younger one, so a squat's row would turn eldest. An
// instance admin settles a dispute (`admin.clearDeviceKey({pinTo})`).
// Deleting the first-seen account frees the id (`device_first_seen` cascades
// with the user): the next account to register it is first-seen and pins.

/** How old (or how far ahead) a proof's `signedAt` may be. */
export const DEVICE_KEY_PROOF_SKEW_MS = 10 * 60 * 1000

const HEX_KEY = /^[0-9a-f]{64}$/
const HEX_SIGNATURE = /^[0-9a-f]{128}$/

export type DeviceKeyProof = { publicKey: string; signedAt: string; signature: string }

/** The bytes the device signs: bound to the account, so a proof captured
 *  for one account never registers the key under another. */
export function deviceKeyMessage(userId: string, deviceId: string, publicKey: string, signedAt: string): string {
  return `exponential-device-key:v1\n${userId}\n${deviceId}\n${publicKey}\n${signedAt}`
}

export type DeviceKeyProofCheck =
  | { ok: true }
  | { ok: false; reason: `format` | `signature` }
  /** `skewMs` = the proof's `signedAt` minus the server's clock: positive =
   *  the device runs ahead, negative = behind (FEED-159: logged so a
   *  drifting clock is diagnosable). */
  | { ok: false; reason: `skew`; skewMs: number }

/** Why `proof` is not a fresh ed25519 signature of `deviceKeyMessage` by
 *  `proof.publicKey` (lowercase hex), or ok. Never throws. */
export function checkDeviceKeyProof(
  args: { userId: string; deviceId: string; proof: DeviceKeyProof },
  now: Date = new Date()
): DeviceKeyProofCheck {
  const { publicKey, signedAt, signature } = args.proof
  if (!HEX_KEY.test(publicKey) || !HEX_SIGNATURE.test(signature)) return { ok: false, reason: `format` }
  const at = Date.parse(signedAt)
  if (!Number.isFinite(at)) return { ok: false, reason: `format` }
  const skewMs = at - now.getTime()
  if (Math.abs(skewMs) > DEVICE_KEY_PROOF_SKEW_MS) return { ok: false, reason: `skew`, skewMs }
  try {
    const key = createPublicKey({
      key: { kty: `OKP`, crv: `Ed25519`, x: Buffer.from(publicKey, `hex`).toString(`base64url`) },
      format: `jwk`,
    })
    const message = Buffer.from(deviceKeyMessage(args.userId, args.deviceId, publicKey, signedAt))
    return verify(null, message, key, Buffer.from(signature, `hex`)) ? { ok: true } : { ok: false, reason: `signature` }
  } catch {
    return { ok: false, reason: `signature` }
  }
}

/** Whether `proof` is a fresh ed25519 signature of `deviceKeyMessage` by
 *  `proof.publicKey` (lowercase hex). Never throws. */
export function verifyDeviceKeyProof(
  args: { userId: string; deviceId: string; proof: DeviceKeyProof },
  now: Date = new Date()
): boolean {
  return checkDeviceKeyProof(args, now).ok
}

/** The log line of a rejected proof (FEED-159): a stale or future
 *  `signedAt` names the machine and how far its clock is off. */
export function deviceKeyRejectionLog(deviceId: string, check: DeviceKeyProofCheck): string {
  if (check.ok) return ``
  if (check.reason === `skew`) {
    const direction = check.skewMs > 0 ? `ahead of` : `behind`
    return `[devices.register] rejected a key proof for ${deviceId}: its clock is ${Math.abs(check.skewMs)} ms ${direction} the server's (limit ${DEVICE_KEY_PROOF_SKEW_MS} ms)`
  }
  return `[devices.register] rejected a key proof for ${deviceId} (${check.reason})`
}

export type SquatRow = { id: string; userId: string; deviceId: string; label: string; sharedTeamIds: string[] }

/**
 * Pins `publicKey` for the machine, or keeps the pin it has. Call it under
 * `lockDeviceShares(deviceId)`, after the caller's row carries the proven
 * key and its register recorded `device_first_seen`. Only the pinning
 * account rotates its key. FEED-159: a FIRST pin is the first-seen
 * account's only (see the header), and still waits while ANOTHER account's
 * row shares the id and the caller's does not (machines registered before
 * keys existed: the sharing row is the box's host until it proves
 * otherwise). The first-seen account takes over a pin another account
 * holds. Another account proving the PINNED key is the same box under a
 * second login: accepted, the pin stays.
 *
 * Returns whether the caller's key is the pinned one, and the other
 * accounts' rows that share the id WITHOUT the pinned key (squats; legacy
 * shares from before the proof; a taken-over holder's shares): the caller
 * withdraws them.
 */
export async function pinDeviceKeyInTx(
  tx: Tx,
  args: { userId: string; deviceId: string; publicKey: string }
): Promise<{ pinned: boolean; squats: SquatRow[] }> {
  const [pin] = await tx
    .select({ publicKey: deviceKeys.publicKey, userId: deviceKeys.userId })
    .from(deviceKeys)
    .where(eq(deviceKeys.deviceId, args.deviceId))
    .limit(1)
  const refused = { pinned: false, squats: [] }
  if (pin && pin.userId !== args.userId && pin.publicKey !== args.publicKey) {
    const [owner] = await tx
      .select({ userId: deviceFirstSeen.userId })
      .from(deviceFirstSeen)
      .where(eq(deviceFirstSeen.deviceId, args.deviceId))
      .limit(1)
    if (owner?.userId !== args.userId) return refused
    const now = new Date()
    await tx
      .update(deviceKeys)
      .set({ publicKey: args.publicKey, userId: args.userId, createdAt: now, updatedAt: now })
      .where(eq(deviceKeys.deviceId, args.deviceId))
  } else if (!pin) {
    const [owner] = await tx
      .select({ userId: deviceFirstSeen.userId })
      .from(deviceFirstSeen)
      .where(eq(deviceFirstSeen.deviceId, args.deviceId))
      .limit(1)
    if (owner?.userId !== args.userId) return refused
    const sharing = await tx
      .select({ userId: devices.userId })
      .from(devices)
      .where(and(eq(devices.deviceId, args.deviceId), sql`cardinality(${devices.sharedTeamIds}) > 0`))
    if (sharing.some((row) => row.userId !== args.userId) && !sharing.some((row) => row.userId === args.userId)) {
      return refused
    }
    await tx
      .insert(deviceKeys)
      .values({ deviceId: args.deviceId, publicKey: args.publicKey, userId: args.userId })
      .onConflictDoNothing()
  } else if (pin.userId === args.userId && pin.publicKey !== args.publicKey) {
    await tx
      .update(deviceKeys)
      .set({ publicKey: args.publicKey, updatedAt: new Date() })
      .where(eq(deviceKeys.deviceId, args.deviceId))
  }
  const squats = await tx
    .select({
      id: devices.id,
      userId: devices.userId,
      deviceId: devices.deviceId,
      label: devices.label,
      sharedTeamIds: devices.sharedTeamIds,
    })
    .from(devices)
    .where(
      and(
        eq(devices.deviceId, args.deviceId),
        ne(devices.userId, args.userId),
        sql`cardinality(${devices.sharedTeamIds}) > 0`,
        sql`${devices.publicKey} IS DISTINCT FROM ${args.publicKey}`
      )
    )
  return { pinned: true, squats: squats.map((row) => ({ ...row, sharedTeamIds: row.sharedTeamIds ?? [] })) }
}

/** Why a share is refused for the caller's row (null = it may share). */
export function shareKeyRefusal(row: { publicKey: string | null; pinnedKey: string | null }): string | null {
  if (!row.publicKey) return `Update Exponential on this machine to share it`
  if (row.pinnedKey !== row.publicKey) {
    return `Another account holds this machine's key. An instance admin can reset it.`
  }
  return null
}
