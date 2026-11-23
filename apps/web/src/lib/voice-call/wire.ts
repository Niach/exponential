// NEW-70 (voice V6): a streamed call's wire as web speaks it
// (`fixtures/voice-call.json` `wire`, the relay's protocol.ts, the desktop
// host's `call_host.rs`). Binary frames carry the audio: byte 0 = kind
// (`up` caller → host, `down` host → caller), bytes 1-2 = a u16 big-endian
// sequence that wraps, then ONE Opus packet of `frameMs`. JSON text frames
// carry the control. That is TODAY'S wire, which the relay can read: it is
// spoken whenever `voice.startCall` does not answer `seal: true`: this tab
// holds no key holder, or the host is an older build or has none
// (#EXP-1941). A sealed call's wire is seal.ts (`seal` in the fixture).
// #EXP-2017: what a control frame MEANS (the host's events, the codes, the
// ends) is `callerStep`'s (@exp/etalk `callerFrame`): here it is only parsed.

import { VOICE_CALL_WIRE } from "@exp/etalk/voice-call"

export const KIND_UP = VOICE_CALL_WIRE.kinds.up
export const KIND_DOWN = VOICE_CALL_WIRE.kinds.down
export const MAX_FRAME_BYTES = VOICE_CALL_WIRE.maxFrameBytes
const HEADER_BYTES = 3

/** One audio frame: kind, sequence, packet. */
export function audioFrame(kind: number, seq: number, packet: Uint8Array): Uint8Array<ArrayBuffer> {
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

/** The relay's control frames a caller reads. */
export type CallerInbound =
  | { t: `call_ready` }
  // EXP-1929: the relay's beat once paired.
  | { t: `call_alive` }
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
