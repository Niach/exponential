// VAPP-33: steer over eNet's chat channel (fixtures/steer-enet.json is the
// spec). One chat message = one envelope `{t:"exp-steer", v:1, sessionId,
// frames}` carrying the steer wire frames unchanged; the device serves the
// join replay from its journal and fans live activity out per viewer.
// Mirrors: crates/steer `enet_steer.rs` (device + desktop viewer), the web
// steer store, iOS and Android.

import fixture from "../fixtures/steer-enet.json" with { type: "json" }

export const STEER_ENET_TAG = fixture.tag
export const STEER_ENET_VERSION = fixture.version
export const STEER_ENET_BATCH_MAX_BYTES = fixture.batchMaxBytes
export const STEER_ENET_CHAT_MAX_BYTES = fixture.chatMaxBytes
export const STEER_ENET_KEEPALIVE_MS = fixture.keepaliveMs
export const STEER_ENET_STALE_MS = fixture.staleMs
export const STEER_ENET_JOIN_TIMEOUT_MS = fixture.joinTimeoutMs
export const STEER_ENET_SEND_TIMEOUT_MS = fixture.sendTimeoutMs

export type SteerFrame = { type: string } & Record<string, unknown>

export type SteerEnvelope = {
  t: string
  v: number
  sessionId: string
  frames: SteerFrame[]
}

/** One envelope's chat body. */
export function encodeSteerEnvelope(sessionId: string, frames: SteerFrame[]): Uint8Array {
  return new TextEncoder().encode(
    JSON.stringify({ t: STEER_ENET_TAG, v: STEER_ENET_VERSION, sessionId, frames })
  )
}

/** A chat body (bytes or text) → the envelope, or null when it is none. */
export function parseSteerEnvelope(body: Uint8Array | string): SteerEnvelope | null {
  const text = typeof body === `string` ? body : new TextDecoder().decode(body)
  if (!text.startsWith(`{`)) return null
  let value: unknown
  try {
    value = JSON.parse(text)
  } catch {
    return null
  }
  return steerEnvelopeFrom(value)
}

/** The parse rules over an already-parsed JSON value. */
export function steerEnvelopeFrom(value: unknown): SteerEnvelope | null {
  if (!value || typeof value !== `object` || Array.isArray(value)) return null
  const v = value as Record<string, unknown>
  if (v.t !== STEER_ENET_TAG || v.v !== STEER_ENET_VERSION) return null
  if (typeof v.sessionId !== `string` || v.sessionId.length === 0) return null
  if (!Array.isArray(v.frames) || v.frames.length === 0) return null
  for (const frame of v.frames) {
    if (!frame || typeof frame !== `object` || Array.isArray(frame)) return null
    if (typeof (frame as Record<string, unknown>).type !== `string`) return null
  }
  return { t: v.t, v: v.v, sessionId: v.sessionId, frames: v.frames as SteerFrame[] }
}

/** The device's access rule for one envelope's sender. */
export function steerViewerAllowed(input: {
  viewerUserId: string | null
  starterUserId: string
  shared: boolean
}): boolean {
  if (!input.viewerUserId) return false
  return input.viewerUserId === input.starterUserId || input.shared
}

/**
 * The device's batching over serialized frame sizes: groups of frame
 * indexes, one envelope each, plus the frames too big for any envelope.
 * `overhead` = the serialized envelope with an empty frames array.
 */
export function steerBatches(
  sizes: number[],
  limits: { overhead: number; batchMaxBytes?: number; chatMaxBytes?: number }
): { groups: number[][]; dropped: number[] } {
  const batchMax = limits.batchMaxBytes ?? STEER_ENET_BATCH_MAX_BYTES
  const chatMax = limits.chatMaxBytes ?? STEER_ENET_CHAT_MAX_BYTES
  const groups: number[][] = []
  const dropped: number[] = []
  let current: number[] = []
  let bytes = limits.overhead
  sizes.forEach((size, index) => {
    if (limits.overhead + size > chatMax) {
      dropped.push(index)
      return
    }
    const next = bytes + size + (current.length > 0 ? 1 : 0)
    if (current.length > 0 && next > batchMax) {
      groups.push(current)
      current = [index]
      bytes = limits.overhead + size
      return
    }
    current.push(index)
    bytes = next
  })
  if (current.length > 0) groups.push(current)
  return { groups, dropped }
}
