import { createHmac, randomBytes, timingSafeEqual } from "node:crypto"

// #EXP-1941: THE CALL ID VOUCHES. `voice.startCall` mints a call's id and
// `voice.callHostTicket` (and the op that makes the chat of a first call)
// recompute it, so a host answers only a ring the SERVER made: for this
// member, this device, THIS agent and this mode, in the last minutes. The
// relay writes the ring (`call_ring {callId, agentId}`) and can invent one;
// it cannot invent an id that verifies, and it cannot move a real id to
// another agent, device or mode.
//
// A v4-shaped UUID (zod's `uuid()` and every client keep taking it):
//   bytes 0-7   random, the version nibble set
//   bytes 8-15  the first eight bytes of HMAC-SHA256(BETTER_AUTH_SECRET, input),
//               the two variant bits written over the top of byte 8: 62 MAC bits
//   input = "exp-call-id:v1:" ‖ bytes 0-7 ‖ len[u16 BE] ‖ utf8(userId)
//           ‖ len[u16 BE] ‖ utf8(deviceId) ‖ sealed[1] ‖ agentId[16 raw bytes]
//           ‖ minute[u32 BE]            minute = Unix seconds / 60 at the mint
// Every field has a fixed size or its length in front: a binary layout, not a
// text template. The token family of lib/reporter/token.ts (deterministic,
// nothing stored; rotating BETTER_AUTH_SECRET ends the rings of that minute).
//
// What it does NOT close: whoever holds BETTER_AUTH_SECRET mints any id; and
// the check keeps no state, so an id verifies again for as long as it lives
// (a host answers an id once: that set lives in the host).

const CONTEXT = `exp-call-id:v1:`
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
/** The minutes a verifier tries, against its own clock: one ahead (a second
 *  server process whose clock is behind the minting one), now, and two back.
 *  So an id lives two to three minutes; a call needs about 90 s at most. */
const MINUTE_OFFSETS = [1, 0, -1, -2]

export interface CallIdBinding {
  userId: string
  /** The steer device the call rings. */
  deviceId: string
  agentId: string
  /** The call's mode: the ends seal it (#EXP-1941) or speak today's wire. */
  sealed: boolean
}

function secret(): string | null {
  return process.env.BETTER_AUTH_SECRET || null
}

function uuidBytes(id: string): Buffer | null {
  return UUID.test(id) ? Buffer.from(id.replace(/-/g, ``), `hex`) : null
}

function uuidText(bytes: Buffer): string {
  const hex = bytes.toString(`hex`)
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}

function prefixed(text: string): Buffer {
  const bytes = Buffer.from(text, `utf8`)
  const length = Buffer.alloc(2)
  length.writeUInt16BE(bytes.length)
  return Buffer.concat([length, bytes])
}

/** The eight MAC bytes of an id, the variant bits in place. Null: an agent id that is no UUID. */
function macTail(key: string, head: Buffer, binding: CallIdBinding, minute: number): Buffer | null {
  const agent = uuidBytes(binding.agentId)
  if (!agent || Buffer.byteLength(binding.userId) > 0xffff || Buffer.byteLength(binding.deviceId) > 0xffff) return null
  const at = Buffer.alloc(4)
  at.writeUInt32BE(minute)
  const input = Buffer.concat([
    Buffer.from(CONTEXT, `utf8`),
    head,
    prefixed(binding.userId),
    prefixed(binding.deviceId),
    Buffer.from([binding.sealed ? 1 : 0]),
    agent,
    at,
  ])
  const tail = createHmac(`sha256`, key).update(input).digest().subarray(0, 8)
  tail[0] = (tail[0] & 0x3f) | 0x80
  return tail
}

const minuteOf = (now: Date) => Math.floor(now.getTime() / 60_000)

/** A fresh call id that vouches for `binding`. Null without BETTER_AUTH_SECRET:
 *  an id never vouches for nothing, so the caller refuses the call. */
export function mintCallId(binding: CallIdBinding, now: Date = new Date()): string | null {
  const key = secret()
  if (!key) return null
  const head = randomBytes(8)
  head[6] = (head[6] & 0x0f) | 0x40
  const tail = macTail(key, head, binding, minuteOf(now))
  return tail ? uuidText(Buffer.concat([head, tail])) : null
}

/** True when the server minted `callId` for exactly this binding within the
 *  id's life. Stateless; a constant-time compare of the eight bytes. */
export function verifyCallId(callId: string, binding: CallIdBinding, now: Date = new Date()): boolean {
  const key = secret()
  const id = uuidBytes(callId)
  if (!key || !id) return false
  const head = id.subarray(0, 8)
  const presented = Buffer.from(id.subarray(8))
  presented[0] = (presented[0] & 0x3f) | 0x80
  let match = false
  for (const offset of MINUTE_OFFSETS) {
    const tail = macTail(key, head, binding, minuteOf(now) + offset)
    // No early exit: every candidate minute is computed and compared.
    if (tail && timingSafeEqual(tail, presented)) match = true
  }
  return match
}

/** #FEED-268: a call in the server's log: the first eight hex digits of its
 *  id (random bytes, never the MAC half), the tag the relay logs too and the
 *  head of the whole id a device's log holds. Anything that is no call id
 *  (a call a desktop runs itself names its own, `call-3`) reads `local`. */
export function callLogTag(callId: string | null | undefined): string {
  return callId && UUID.test(callId) ? callId.slice(0, 8).toLowerCase() : `local`
}
