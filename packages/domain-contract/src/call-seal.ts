// #EXP-1941: the call seal, TS reference (packages/etalk/fixtures/voice-call.json
// `seal` is the spec and holds the vectors; packages/etalk/src/call-seal.test.ts
// replays them). Two sealing ends of a Voice call prove that both hold the
// chat's tree SECRET and seal the call's audio and events under keys of a
// fresh X25519 exchange, so the relay forwards bytes it cannot read.
//
//   hello      = version 0x01 ‖ role (1 caller, 2 host) ‖ epoch u32 BE ‖ rootId[16] ‖ eph[32]
//                the host's is these 54 bytes ‖ tag[16]
//   dh         = X25519(own eph secret, peer eph)             (all-zero = refused)
//   transcript = "exp/etalk/call/1" ‖ callId[16] ‖ agentId[16] ‖ SHA-256(utf8(deviceId))
//                ‖ callerHello[54] ‖ hostHello[0..54]         (188 bytes; never the tag)
//   key(label) = HKDF-SHA256(ikm = dh ‖ clamp(S), salt = transcript, info = label, L = 32)
//   tag        = ChaCha20-Poly1305(key("down/hello"), nonce = 00 × 12, aad = hostHello[0..54], empty)
//   frame      = ChaCha20-Poly1305(key, nonce = 00 × 8 ‖ ctr u32 BE, aad = the clear header)
//
// The order is Noise NNpsk2's (-> e; <- e, ee, psk): the host's hello carries
// the host's proof, so a caller that HAS a `CallSeal` has it, and the caller's
// first frame that opens is the caller's proof, before which a host's
// `CallSeal` seals nothing. The sealer is an OBJECT: it holds the keys and the
// counters, and no method takes a counter, so one nonce is never used twice.
// `finish` works once, a failed one too.
// Nothing here is wired into a call yet: until the callers, the hosts and the
// server follow, a call's events and audio cross the relay readable.
// Mirror: enet-core `call_seal` (Rust; the phones through exponential-net-ffi).
// Pure JS (@noble), so it runs in the browser too.

import { chacha20poly1305 } from "@noble/ciphers/chacha.js"
import { x25519 } from "@noble/curves/ed25519"
import { hkdf } from "@noble/hashes/hkdf.js"
import { sha256 } from "@noble/hashes/sha2.js"

/** The constants of voice-call.json `seal` (and `wire.kinds`) the bytes hang on. */
export const CALL_SEAL = {
  version: 1,
  domain: `exp/etalk/call/1`,
  labels: { upAudio: `up/audio`, downAudio: `down/audio`, downControl: `down/control`, downHello: `down/hello` },
  roles: { caller: 1, host: 2 },
  frameTypes: { audio: 0, hello: 1 },
  kinds: { up: 1, down: 2 },
  helloBytes: { caller: 54, host: 70 },
} as const

export type CallRole = keyof typeof CALL_SEAL.roles

/** Why the seal refused (the `reason` of voice-call.json `seal.refused`). Logs name it, never bytes. */
export type CallSealRefusal =
  | `hello` // not a hello of the other role, its length and this version
  | `root` // the host's hello names another root or epoch than the caller's own answer
  | `unproven` // a host's seal before a frame of the caller opened
  | `low_order` // the exchange gave all zeros
  | `finished` // a second `finish`
  | `input` // an id, a secret or a number of the wrong shape; a method of the other role
  | `too_large` // a packet above the step, a text above maxPlainBytes
  | `exhausted` // the counter is used up
  | `frame` // not a frame of this channel: its size, kind, type or inner length
  | `tag` // it does not open: a frame, or the tag of the host's hello
  | `replay` // an audio ctr not above the last accepted
  | `order` // a control ctr that is not exactly the next

export class CallSealError extends Error {
  readonly reason: CallSealRefusal
  constructor(reason: CallSealRefusal) {
    super(`call seal: ${reason}`)
    this.name = `CallSealError`
    this.reason = reason
  }
}

/** The sizes a sealer pads to: voice-call.json `seal` itself fits. */
export interface CallSealPad {
  audioPadStep: { up: number; down: number }
  controlPadBytes: number
  maxPlainBytes: number
}

export interface CallHello {
  version: number
  role: CallRole
  epoch: number
  /** Lowercase; all zeros from a caller that knew no root. */
  rootId: string
  eph: Uint8Array
  /** The 54 bytes the transcript holds. */
  body: Uint8Array
  /** The host's proof (16 bytes); null on a caller's hello. */
  tag: Uint8Array | null
}

export interface CallFinishInput {
  /** The tree SECRET (32 bytes) of the HOST hello's root and epoch. */
  secret: Uint8Array
  callId: string
  agentId: string
  /** The host's steer device id. */
  deviceId: string
  peerHello: Uint8Array
  /**
   * CALLER only, and required there: the root and the epoch its OWN
   * `sendTarget` answer names now. A host hello that names others is refused.
   */
  rootId?: string
  epoch?: number
  pad: CallSealPad
}

export interface CallFinished {
  seal: CallSeal
  /** The host's hello with its tag: what a host sends (once), what a caller took. */
  hostHello: Uint8Array
}

export interface OpenedAudio {
  /** The packet's number in its encoder run. Never a reason to refuse. */
  seq: number
  /** Bit 0 on `up` = the microphone is muted; unknown bits pass. */
  flags: number
  /** Empty for a frame with `len` 0: nothing of it reaches a decoder. */
  packet: Uint8Array
  /** Frames missing before this one. */
  missing: number
}

interface CallKeys {
  upAudio: Uint8Array
  downAudio: Uint8Array
  downControl: Uint8Array
}

const HELLO_BODY_BYTES = CALL_SEAL.helloBytes.caller
const HEADER_BYTES = 6 // kind ‖ type ‖ ctr
const INNER_BYTES = 5 // seq ‖ flags ‖ len
const TAG_BYTES = 16
const CTR_BYTES = 4
const MAX_CTR = 0xffff_ffff
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
const utf8 = new TextEncoder()

const refuse = (reason: CallSealRefusal): never => {
  throw new CallSealError(reason)
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

const u32 = (n: number) => Uint8Array.of(n >>> 24, (n >>> 16) & 0xff, (n >>> 8) & 0xff, n & 0xff)
const readU32 = (bytes: Uint8Array, at: number) => ((bytes[at]! << 24) | (bytes[at + 1]! << 16) | (bytes[at + 2]! << 8) | bytes[at + 3]!) >>> 0
const nonce = (ctr: number) => concat(new Uint8Array(8), u32(ctr))
const isCount = (n: number, max: number) => Number.isInteger(n) && n >= 0 && n <= max

/** The 16 raw bytes of a UUID in either case. */
function uuidBytes(id: string): Uint8Array {
  if (!UUID.test(id)) refuse(`input`)
  const digits = id.replace(/-/g, ``)
  return Uint8Array.from({ length: 16 }, (_, i) => Number.parseInt(digits.slice(i * 2, i * 2 + 2), 16))
}

function uuidText(bytes: Uint8Array): string {
  const digits = Array.from(bytes, (b) => b.toString(16).padStart(2, `0`)).join(``)
  return `${digits.slice(0, 8)}-${digits.slice(8, 12)}-${digits.slice(12, 16)}-${digits.slice(16, 20)}-${digits.slice(20)}`
}

function toBase64Url(bytes: Uint8Array): string {
  let binary = ``
  for (const byte of bytes) binary += String.fromCharCode(byte)
  return btoa(binary).replace(/\+/g, `-`).replace(/\//g, `_`).replace(/=+$/, ``)
}

function fromBase64Url(text: string): Uint8Array | null {
  if (!/^[A-Za-z0-9_-]*$/.test(text) || text.length % 4 === 1) return null
  const binary = atob(text.replace(/-/g, `+`).replace(/_/g, `/`))
  return Uint8Array.from(binary, (c) => c.charCodeAt(0))
}

/** Reads a hello. Refused (`hello`): another version or role, or not the length of its role. */
export function parseCallHello(hello: Uint8Array): CallHello {
  if (hello[0] !== CALL_SEAL.version) refuse(`hello`)
  const role = hello[1] === CALL_SEAL.roles.caller ? `caller` : hello[1] === CALL_SEAL.roles.host ? `host` : refuse(`hello`)
  if (hello.length !== CALL_SEAL.helloBytes[role]) refuse(`hello`)
  const body = hello.slice(0, HELLO_BODY_BYTES)
  const tag = role === `host` ? hello.slice(HELLO_BODY_BYTES) : null
  return { version: CALL_SEAL.version, role, epoch: readU32(hello, 2), rootId: uuidText(hello.slice(6, 22)), eph: body.slice(22), body, tag }
}

/** The caller's hello on the wire: the binary frame kind `up` ‖ type hello ‖ hello. */
export function callHelloFrame(hello: Uint8Array): Uint8Array {
  return concat(Uint8Array.of(CALL_SEAL.kinds.up, CALL_SEAL.frameTypes.hello), hello)
}

/** The hello of a binary frame, or null when the frame is none (a hello never goes down). */
export function callHelloOfFrame(frame: Uint8Array): Uint8Array | null {
  const isHello = frame.length === 2 + CALL_SEAL.helloBytes.caller && frame[0] === CALL_SEAL.kinds.up && frame[1] === CALL_SEAL.frameTypes.hello
  return isHello ? frame.slice(2) : null
}

/** The host's hello (with its tag) on the wire: the `data` of `call_event {t: "hello"}`. */
export function callHelloText(hello: Uint8Array): string {
  return toBase64Url(hello)
}

export function callHelloOfText(data: string): Uint8Array | null {
  const hello = fromBase64Url(data)
  return hello && hello.length === CALL_SEAL.helloBytes.host ? hello : null
}

/** One end's half of the exchange. It holds the eph secret until `finish` takes it. */
export class CallHandshake {
  readonly role: CallRole
  /** The 54 bytes of the own hello; a host's goes out only with its tag, from `finish`. */
  private readonly ownHello: Uint8Array
  private ephSecret: Uint8Array | null

  private constructor(role: CallRole, rootId: string | null, epoch: number, ephSecret: Uint8Array) {
    if (!isCount(epoch, MAX_CTR) || ephSecret.length !== 32) refuse(`input`)
    const root = rootId === null ? new Uint8Array(16) : uuidBytes(rootId)
    this.role = role
    this.ephSecret = ephSecret
    this.ownHello = concat(Uint8Array.of(CALL_SEAL.version, CALL_SEAL.roles[role]), u32(epoch), root, x25519.getPublicKey(ephSecret))
  }

  /** A fresh eph key per socket. `rootId` null = a caller that knows no root yet (zeros). */
  static create(role: CallRole, rootId: string | null, epoch: number): CallHandshake {
    const ephSecret = new Uint8Array(32)
    globalThis.crypto.getRandomValues(ephSecret)
    return new CallHandshake(role, rootId, epoch, ephSecret)
  }

  /** VECTORS ONLY, so private: a fixed eph secret must never reach a call. */
  private static withEphSecret(role: CallRole, rootId: string | null, epoch: number, ephSecret: Uint8Array): CallHandshake {
    return new CallHandshake(role, rootId, epoch, ephSecret.slice())
  }

  /** The CALLER's hello: made once, resent unchanged. A host has none before `finish`. */
  get hello(): Uint8Array {
    if (this.role !== `caller`) refuse(`input`)
    return this.ownHello.slice()
  }

  /**
   * The keys of the call. The HOST's also gives its hello with the tag, its
   * proof. The CALLER's takes that hello, refuses one that names another root
   * or epoch than the caller's own answer, and opens the tag: a caller that
   * has a `CallSeal` has the host's proof.
   *
   * WORKS ONCE: the eph secret is taken out and wiped whatever the answer (a
   * refusal has used the handshake up too), so a hello or a `call_ready` that
   * comes again can never give a second sealer whose counters start at 0
   * under the same keys.
   */
  finish(input: CallFinishInput): CallFinished {
    const ephSecret = this.ephSecret
    if (!ephSecret) return refuse(`finished`)
    this.ephSecret = null
    try {
      const peer = parseCallHello(input.peerHello)
      if (peer.role === this.role) refuse(`hello`)
      const caller = this.role === `caller`
      if (caller) {
        if (input.rootId === undefined || input.epoch === undefined) return refuse(`input`)
        const named = uuidBytes(input.rootId)
        if (input.epoch !== peer.epoch || !named.every((byte, i) => byte === peer.body[6 + i])) refuse(`root`)
      }
      const [callerHello, hostBody] = caller ? [this.ownHello, peer.body] : [peer.body, this.ownHello]
      const { dh, keys, helloKey } = CallHandshake.derive(ephSecret, peer.eph, input.secret, input.callId, input.agentId, input.deviceId, callerHello, hostBody)
      dh.fill(0)
      try {
        // The hello key seals exactly once, the tag, at nonce zero.
        const proof = chacha20poly1305(helloKey, new Uint8Array(12), hostBody)
        if (!caller) return { seal: new CallSeal(MINT, this.role, keys, input.pad), hostHello: concat(hostBody, proof.encrypt(new Uint8Array(0))) }
        try {
          proof.decrypt(peer.tag!)
        } catch {
          return refuse(`tag`)
        }
        return { seal: new CallSeal(MINT, this.role, keys, input.pad), hostHello: input.peerHello.slice() }
      } finally {
        helloKey.fill(0)
      }
    } finally {
      ephSecret.fill(0)
    }
  }

  private static derive(
    ephSecret: Uint8Array,
    peerEph: Uint8Array,
    secret: Uint8Array,
    callId: string,
    agentId: string,
    deviceId: string,
    callerHello: Uint8Array,
    hostBody: Uint8Array
  ): { dh: Uint8Array; transcript: Uint8Array; keys: CallKeys; helloKey: Uint8Array } {
    if (secret.length !== 32) refuse(`input`)
    const transcript = concat(utf8.encode(CALL_SEAL.domain), uuidBytes(callId), uuidBytes(agentId), sha256(utf8.encode(deviceId)), callerHello, hostBody)
    let dh: Uint8Array
    try {
      dh = x25519.getSharedSecret(ephSecret, peerEph)
    } catch {
      return refuse(`low_order`)
    }
    if (dh.every((byte) => byte === 0)) refuse(`low_order`)
    // X25519 clamps at use: two byte strings that are one tree key give one call key.
    const clamped = secret.slice()
    clamped[0]! &= 0xf8
    clamped[31] = (clamped[31]! & 0x7f) | 0x40
    const ikm = concat(dh, clamped)
    const key = (label: string) => hkdf(sha256, ikm, transcript, utf8.encode(label), 32)
    const keys = { upAudio: key(CALL_SEAL.labels.upAudio), downAudio: key(CALL_SEAL.labels.downAudio), downControl: key(CALL_SEAL.labels.downControl) }
    const helloKey = key(CALL_SEAL.labels.downHello)
    clamped.fill(0)
    ikm.fill(0)
    return { dh, transcript, keys, helloKey }
  }
}

/** Only `finish` makes a `CallSeal`. */
const MINT = Symbol(`call-seal`)

/**
 * The sealed call of ONE end: the caller seals `up` audio and opens `down`
 * audio and text, the host the reverse. It owns its counters.
 */
export class CallSeal {
  readonly role: CallRole
  private readonly keys: CallKeys
  private readonly pad: CallSealPad
  /** The next ctr this end seals with, per channel. */
  private audioCtr = 0
  private controlCtr = 0
  /** The last audio ctr accepted, and the control ctr that must come next. */
  private audioSeen = -1
  private controlNext = 0
  /** A host seals nothing before a frame of the caller opened: the caller's proof. */
  private proven: boolean

  constructor(mint: typeof MINT, role: CallRole, keys: CallKeys, pad: CallSealPad) {
    if (mint !== MINT) refuse(`input`)
    this.role = role
    this.keys = keys
    this.pad = pad
    this.proven = role === `caller`
  }

  /**
   * One audio frame of this end's direction, always ONE size: 27 bytes plus
   * the direction's step. A packet above the step is refused (`too_large`)
   * and takes no counter: the sender seals an empty packet in its place, with
   * the seq of the packet it replaces.
   */
  sealAudio(seq: number, flags: number, packet: Uint8Array): Uint8Array {
    if (!this.proven) refuse(`unproven`)
    const up = this.role === `caller`
    const step = up ? this.pad.audioPadStep.up : this.pad.audioPadStep.down
    if (!isCount(seq, 0xffff) || !isCount(flags, 0xff) || !isCount(step, 0xffff)) refuse(`input`)
    if (packet.length > step) refuse(`too_large`)
    const ctr = this.audioCtr
    if (ctr > MAX_CTR) refuse(`exhausted`)
    this.audioCtr = ctr + 1
    const plain = new Uint8Array(INNER_BYTES + step)
    plain.set([seq >>> 8, seq & 0xff, flags, packet.length >>> 8, packet.length & 0xff])
    plain.set(packet, INNER_BYTES)
    const header = concat(Uint8Array.of(up ? CALL_SEAL.kinds.up : CALL_SEAL.kinds.down, CALL_SEAL.frameTypes.audio), u32(ctr))
    const key = up ? this.keys.upAudio : this.keys.downAudio
    return concat(header, chacha20poly1305(key, nonce(ctr), header).encrypt(plain))
  }

  /**
   * Opens an audio frame of the OTHER direction. Only a ctr above the last
   * accepted opens; gaps pass and are counted, and a frame with `len` 0 is
   * itself ONE missing frame (the packet its sender could not seal). A
   * refused frame moves no mark.
   */
  openAudio(frame: Uint8Array): OpenedAudio {
    const kind = this.role === `caller` ? CALL_SEAL.kinds.down : CALL_SEAL.kinds.up
    if (frame.length < HEADER_BYTES + INNER_BYTES + TAG_BYTES || frame[0] !== kind || frame[1] !== CALL_SEAL.frameTypes.audio) refuse(`frame`)
    const ctr = readU32(frame, 2)
    if (ctr <= this.audioSeen) refuse(`replay`)
    const key = this.role === `caller` ? this.keys.downAudio : this.keys.upAudio
    let plain: Uint8Array
    try {
      plain = chacha20poly1305(key, nonce(ctr), frame.slice(0, HEADER_BYTES)).decrypt(frame.slice(HEADER_BYTES))
    } catch {
      return refuse(`tag`)
    }
    const len = (plain[3]! << 8) | plain[4]!
    if (len > plain.length - INNER_BYTES) refuse(`frame`)
    const missing = ctr - this.audioSeen - 1 + (len === 0 ? 1 : 0)
    this.audioSeen = ctr
    this.proven = true
    return { seq: (plain[0]! << 8) | plain[1]!, flags: plain[2]!, packet: plain.slice(INNER_BYTES, INNER_BYTES + len), missing }
  }

  /**
   * Host only: ONE control frame of today as JSON text, padded with spaces to
   * a multiple of `controlPadBytes`. Gives the `data` of `call_event {t: "sealed"}`.
   */
  sealControl(text: string): string {
    if (this.role !== `host`) refuse(`input`)
    if (!this.proven) refuse(`unproven`)
    const raw = utf8.encode(text)
    const { controlPadBytes, maxPlainBytes } = this.pad
    if (!isCount(controlPadBytes, maxPlainBytes) || controlPadBytes === 0) refuse(`input`)
    const size = Math.max(1, Math.ceil(raw.length / controlPadBytes)) * controlPadBytes
    if (size > maxPlainBytes) refuse(`too_large`)
    const ctr = this.controlCtr
    if (ctr > MAX_CTR) refuse(`exhausted`)
    this.controlCtr = ctr + 1
    const plain = new Uint8Array(size).fill(0x20)
    plain.set(raw)
    const header = u32(ctr)
    return toBase64Url(concat(header, chacha20poly1305(this.keys.downControl, nonce(ctr), header).encrypt(plain)))
  }

  /**
   * Caller only: the text of a `sealed` event, trailing spaces included. The
   * ctr is strict (the first is 0, then exactly last + 1): the relay never
   * drops a text frame, so a gap is a deleted event and ends the call.
   */
  openControl(data: string): string {
    if (this.role !== `caller`) refuse(`input`)
    const frame = fromBase64Url(data)
    if (!frame || frame.length < CTR_BYTES + TAG_BYTES) return refuse(`frame`)
    const ctr = readU32(frame, 0)
    if (ctr !== this.controlNext) refuse(`order`)
    let plain: Uint8Array
    try {
      plain = chacha20poly1305(this.keys.downControl, nonce(ctr), frame.slice(0, CTR_BYTES)).decrypt(frame.slice(CTR_BYTES))
    } catch {
      return refuse(`tag`)
    }
    let text: string
    try {
      text = new TextDecoder(`utf-8`, { fatal: true }).decode(plain)
    } catch {
      return refuse(`frame`)
    }
    this.controlNext = ctr + 1
    return text
  }
}
