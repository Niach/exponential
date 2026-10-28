import { createPublicKey, verify } from "node:crypto"
import { and, eq, ne, sql } from "drizzle-orm"
import { deviceKeys, devices } from "@/db/schema"
import type { Tx } from "@/lib/sessions/tx"

// FEED-154: proof of possession for a machine id. A machine's id shows in
// team-visible sessions, so anyone can register it; what they cannot do is
// sign with the machine's private key. The CLI daemon keeps an ed25519 key
// in its data dir (`cli-device-key`) and signs every `devices.register`
// with it (`deviceKeyMessage`); the server stores the proven public key on
// the caller's row and PINS the first proven key per machine id
// (`device_keys`, trust-on-first-use). A share needs the row's proven key to
// equal the pin (`devices.setShared`), so a squat of the id never shares.

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

/** Whether `proof` is a fresh ed25519 signature of `deviceKeyMessage` by
 *  `proof.publicKey` (lowercase hex). Never throws. */
export function verifyDeviceKeyProof(
  args: { userId: string; deviceId: string; proof: DeviceKeyProof },
  now: Date = new Date()
): boolean {
  const { publicKey, signedAt, signature } = args.proof
  if (!HEX_KEY.test(publicKey) || !HEX_SIGNATURE.test(signature)) return false
  const at = Date.parse(signedAt)
  if (!Number.isFinite(at) || Math.abs(now.getTime() - at) > DEVICE_KEY_PROOF_SKEW_MS) return false
  try {
    const key = createPublicKey({
      key: { kty: `OKP`, crv: `Ed25519`, x: Buffer.from(publicKey, `hex`).toString(`base64url`) },
      format: `jwk`,
    })
    const message = Buffer.from(deviceKeyMessage(args.userId, args.deviceId, publicKey, signedAt))
    return verify(null, message, key, Buffer.from(signature, `hex`))
  } catch {
    return false
  }
}

export type SquatRow = { id: string; userId: string; deviceId: string; label: string; sharedTeamIds: string[] }

/**
 * Pins `publicKey` for the machine, or keeps the pin it has. Call it under
 * `lockDeviceShares(deviceId)`, after the caller's row carries the proven
 * key. The first proven key wins; only the pinning account rotates it. One
 * bootstrap rule for machines registered before keys existed: no first pin
 * while ANOTHER account's row shares the id and the caller's does not (the
 * sharing row is the box's host until it proves otherwise).
 *
 * Returns whether the caller now holds the pin, and the other accounts'
 * rows that share the id WITHOUT the pinned key (squats; legacy shares from
 * before the proof): the caller withdraws them.
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
  if (!pin) {
    const sharing = await tx
      .select({ userId: devices.userId })
      .from(devices)
      .where(and(eq(devices.deviceId, args.deviceId), sql`cardinality(${devices.sharedTeamIds}) > 0`))
    if (sharing.some((row) => row.userId !== args.userId) && !sharing.some((row) => row.userId === args.userId)) {
      return { pinned: false, squats: [] }
    }
    await tx
      .insert(deviceKeys)
      .values({ deviceId: args.deviceId, publicKey: args.publicKey, userId: args.userId })
      .onConflictDoNothing()
  } else if (pin.publicKey !== args.publicKey) {
    if (pin.userId !== args.userId) return { pinned: false, squats: [] }
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
