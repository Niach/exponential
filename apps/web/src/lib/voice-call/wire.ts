// NEW-70 (voice V6): a streamed call's wire as web speaks it
// (`fixtures/voice-call.json` `wire`, the relay's protocol.ts, the desktop
// host's `call_host.rs`). Binary frames carry the audio: byte 0 = kind
// (`up` caller → host, `down` host → caller), bytes 1-2 = a u16 big-endian
// sequence that wraps, then ONE Opus packet of `frameMs`. JSON text frames
// carry the control.

import { VOICE_CALL_WIRE, type CallEvent } from "@exp/domain-contract/voice-call"

export const KIND_UP = VOICE_CALL_WIRE.kinds.up
export const KIND_DOWN = VOICE_CALL_WIRE.kinds.down
export const MAX_FRAME_BYTES = VOICE_CALL_WIRE.maxFrameBytes
const HEADER_BYTES = 3

/** One audio frame: kind, sequence, packet. */
export function audioFrame(kind: number, seq: number, packet: Uint8Array): Uint8Array {
  const frame = new Uint8Array(HEADER_BYTES + packet.length)
  frame[0] = kind
  frame[1] = (seq >> 8) & 0xff
  frame[2] = seq & 0xff
  frame.set(packet, HEADER_BYTES)
  return frame
}

/** A well-formed frame within the size cap, else null. */
export function parseAudioFrame(data: ArrayBuffer | Uint8Array): { kind: number; seq: number; packet: Uint8Array } | null {
  const bytes = data instanceof Uint8Array ? data : new Uint8Array(data)
  if (bytes.length <= HEADER_BYTES || bytes.length > MAX_FRAME_BYTES) return null
  return { kind: bytes[0]!, seq: (bytes[1]! << 8) | bytes[2]!, packet: bytes.subarray(HEADER_BYTES) }
}

/** Whether `seq` comes after `last` on the wrapping u16 sequence. */
export function seqAfter(seq: number, last: number): boolean {
  const delta = (seq - last + 0x10000) & 0xffff
  return delta !== 0 && delta < 0x8000
}

/** The flow events a HOST may send in `call_event` (wire.control): anything
 *  else, or a malformed one, is ignored. */
export function hostCallEvent(raw: unknown): CallEvent | null {
  if (!raw || typeof raw !== `object`) return null
  const event = raw as Record<string, unknown>
  const text = typeof event.text === `string` ? event.text : null
  switch (event.t) {
    case `partial`:
    case `said`:
    case `replyStarted`:
      return text === null ? null : { t: event.t, text }
    case `queued`:
      return typeof event.count === `number` && text !== null ? { t: `queued`, count: event.count, text } : null
    case `replyFinished`:
    case `speechStarted`:
      return { t: event.t }
    default:
      return null
  }
}

/** The relay's control frames a caller reads. */
export type CallerInbound =
  | { t: `call_ready` }
  | { t: `call_event`; event: unknown }
  | { t: `call_end`; reason?: string }
  | { t: `error`; code: string }
  | { t: `bye`; outcome?: string }

export function parseControl(text: string): CallerInbound | null {
  try {
    const frame = JSON.parse(text) as { t?: unknown }
    if (frame && typeof frame === `object` && typeof frame.t === `string`) return frame as CallerInbound
  } catch {}
  return null
}
