// NEW-70 (voice V6): one web call, streamed to the member's voice device
// (`fixtures/voice-call.json` TRANSPORT). #EXP-2017: THE CALLER IS THE
// PHONES' PURE MACHINE: `callerStep` (@exp/etalk, voice-call.json `caller`,
// whose `$comment` is the spec; iOS and Android replay the same rows)
// decides, this class only does its effects, in order, and hands in what
// happened; a report of a call that ended is dropped (the generation).
//
// Web holds nothing at the tap (iOS's way: `held ok` at once), so it rings
// from the tap; the `ticket` effect opens the microphone and the Opus codec
// first (a browser without WebCodecs Opus fails `unsupported` and never
// dials: the host decodes nothing else), then waits for a key holder still
// starting and asks `voice.startCall`. `call_ready` opens the pair (the
// microphone's frames go up from then on as `up` frames, the reply comes
// down as `down` frames) while the host's `call_event`s drive the SAME
// `callStep` the voice device runs; its `rang` + `ready` answer the ring.
// Mute = `call_mute` once paired (the microphone keeps sending its
// silence), End call = `call_end`. An own failure once dialled (the codec,
// a path gone silent, the ring's bound at `maxRingMs`: `unavailable`)
// tells the host `call_end {reason: dropped}` before it lets go; an end from
// the far side (`call_end`, its `end` / `hungUp` events, `error`, `bye`, the
// socket going away) sends nothing.
//
// #EXP-1941: A SEALED CALL. A caller that holds a key holder claims `seal`;
// when `voice.startCall` answers `seal: true` (both ends can seal and the
// server turned it on) the call speaks the sealed wire of `seal` instead:
// the caller's hello after `call_ready`, the host's hello with its tag, then
// every microphone packet as ONE sealed frame of one size with mute inside
// (no `call_mute`), and the host's events and audio opened before anything
// else reads them (seal.ts). Nothing of the microphone leaves before the
// host proved the chat's key; a seal failure reads `dropped` (its line
// tells the host its own reason). A code about this browser's OWN device
// (`no_microphone`, `not_granted`, `unsupported`) is never taken from the
// wire: there it reads `dropped`.

import {
  CALL_IDLE,
  CALLER_IDLE,
  callerStep,
  callPathDead,
  VOICE_CALL_COPY,
  type CallerEffect,
  type CallerInput,
  type CallerOutgoing,
  type CallerState,
  type CallState,
} from "@exp/etalk/voice-call"

import { CallAudioError, type CallAudio } from "./audio"
import { CallSealLine, type CallKeySource, type CallKeyTarget } from "./seal"
import { audioFrame, KIND_DOWN, KIND_UP, parseAudioFrame, parseControl, seqAfter } from "./wire"

/** The slice of a WebSocket a call uses (the tests hand in a fake). */
export interface CallSocket {
  binaryType: BinaryType
  readyState: number
  onopen: ((event: Event) => void) | null
  onmessage: ((event: MessageEvent) => void) | null
  onclose: ((event: CloseEvent) => void) | null
  onerror: ((event: Event) => void) | null
  send(data: string | ArrayBuffer | Uint8Array<ArrayBuffer>): void
  close(code?: number, reason?: string): void
}

export interface CallDeps {
  /** EXP-1857: the server picks the target itself; `device` + `fallback`
   *  name the desktop that speaks (an older server answers neither).
   *  #EXP-1941: `seal` = this caller can seal the call; the answer's `seal`
   *  is the mode the server decided, for both ends, with the ids the keys
   *  bind (`callId`, `deviceId` = the host's). */
  startCall(
    agentId: string,
    seal: boolean
  ): Promise<{ url: string; device?: string; fallback?: boolean; seal?: boolean; callId?: string; deviceId?: string }>
  /** #EXP-1941: this browser's key holder; none = a caller that cannot seal. */
  keys?: CallKeySource
  openSocket(url: string): CallSocket
  openAudio(): Promise<CallAudio>
  /** S-3: play the ring tone, at most `ms`; answers a stop (none = silent). */
  ring?(ms: number): () => void
}

/** What a call shows (the microphone's level is its own store, read only
 *  by the card: it changes every frame). */
export interface CallSnapshot {
  state: CallState
}

/** `voice.startCall`'s usable answer: where to dial, and a sealed call's ids. */
interface CallTicket {
  url: string
  sealed: { callId: string; deviceId: string; target: CallKeyTarget } | null
}

const OPEN = 1
/** EXP-1929: how often the liveness deadline is looked at. */
const LIVENESS_TICK_MS = 1_000

const escapeRegExp = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, `\\$&`)

/** The fixture's availability reasons, `{device}` = any name. */
const REFUSALS = Object.values(VOICE_CALL_COPY.reasons).map(
  (reason) => new RegExp(`^${reason.split(`{device}`).map(escapeRegExp).join(`.+`)}$`)
)

/** `voice.startCall`'s refusal when it is one of the fixture's availability
 *  reasons (the server names the device), else null. */
function refusalReason(error: unknown): string | null {
  const message = error instanceof Error ? error.message.trim() : ``
  return message && REFUSALS.some((refusal) => refusal.test(message)) ? message : null
}

export class VoiceCall {
  /** The caller's session: the flow and its stage (`callerStep`). */
  private caller: CallerState = CALLER_IDLE
  private snapshot: CallSnapshot = { state: CALL_IDLE }
  private listeners = new Set<() => void>()
  private level = 0
  private levelListeners = new Set<() => void>()
  /** Bumped by every start and every `release`: what a call reports after
   *  its end is dropped. */
  private generation = 0
  private inbox: CallerInput[] = []
  private stepping = false
  private agentId: string | null = null
  /** The `ticket` effect's job: `start` settles with it. */
  private asking: Promise<void> | null = null
  private ticket: CallTicket | null = null
  private socket: CallSocket | null = null
  private audio: CallAudio | null = null
  /** The pair is up on today's wire: the microphone's frames go up. */
  private uplink = false
  private seq = 0
  private lastDown: number | null = null
  private stopRingTone: (() => void) | null = null
  /** The ring's bound (`maxRingMs`): `ringTimeout`. */
  private ringTimer: ReturnType<typeof setTimeout> | null = null
  /** EXP-1929: the relay beats (`call_alive`), when it last sent anything,
   *  and the check that ends a path gone silent. */
  private alive = false
  private heardAt = 0
  private liveness: ReturnType<typeof setInterval> | null = null
  /** #EXP-1941: the sealed line of a call the server marked sealed, else null (today's wire). */
  private line: CallSealLine | null = null
  /** A failure's toast: the fixture's (`failed`), or a refusal the server
   *  worded itself (an availability reason). */
  onToast: ((message: string) => void) | null = null

  constructor(private readonly deps: CallDeps) {}

  subscribe = (listener: () => void) => {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  getSnapshot = () => this.snapshot

  subscribeLevel = (listener: () => void) => {
    this.levelListeners.add(listener)
    return () => this.levelListeners.delete(listener)
  }

  getLevel = () => this.level

  private setLevel(level: number) {
    if (level === this.level) return
    this.level = level
    for (const listener of this.levelListeners) listener()
  }

  get active(): boolean {
    return this.caller.stage !== `idle`
  }

  /** Dial `agentId` on the desktop called `device`; `language` = the
   *  agent's voice language; EXP-1857: `fallback` = that desktop is not the
   *  agent's own (`via` shows it). Settles once the ticket was asked. */
  async start(agentId: string, device: string, language?: string, fallback = false): Promise<void> {
    if (this.active) return
    this.generation += 1
    this.agentId = agentId
    this.asking = null
    this.send({ t: `start`, device, language, fallback })
    await this.asking
  }

  toggleMute() {
    this.send({ t: `mute` })
  }

  /** End call (and the page going away). */
  end() {
    this.send({ t: `end` })
  }

  /** Hands `input` to `callerStep`, one at a time; `generation` = the call
   *  it belongs to (a report of an ended call is dropped). */
  private send(input: CallerInput, generation?: number) {
    if (generation !== undefined && generation !== this.generation) return
    this.inbox.push(input)
    if (this.stepping) return
    this.stepping = true
    try {
      while (this.inbox.length > 0) {
        const stepped = callerStep(this.caller, this.inbox.shift()!)
        this.caller = stepped.state
        this.publish()
        for (const effect of stepped.effects) this.perform(effect)
      }
    } finally {
      this.stepping = false
    }
  }

  private publish() {
    const state = this.caller.call
    if (state === this.snapshot.state) return
    this.snapshot = { state }
    for (const listener of this.listeners) listener()
    if (state.phase === `hidden` || state.muted) this.setLevel(0)
  }

  private perform(effect: CallerEffect) {
    const generation = this.generation
    switch (effect.do) {
      case `hold`:
        // A browser holds nothing before it rings (iOS's way): the
        // microphone is asked with the ticket.
        return this.send({ t: `held`, ok: true }, generation)
      case `ring`:
        this.stopRingTone = this.deps.ring?.(effect.ms) ?? null
        this.ringTimer = setTimeout(() => this.send({ t: `ringTimeout` }, generation), effect.ms)
        return
      case `ticket`:
        this.asking = this.ask(generation)
        return
      case `dial`:
        return this.dial(generation)
      case `startAudio`:
        // The microphone runs since the ticket: its frames go up from now on,
        // a sealed call's only once its line has the host's proof.
        if (this.line) this.line.ready()
        else this.uplink = true
        return
      case `stopRing`:
        return this.silenceRing()
      case `flush`:
        return this.audio?.flush()
      case `mute`:
        return this.audio?.setMuted(effect.muted)
      case `speaker`:
        // A browser has no earpiece to switch from.
        return
      case `send`:
        return this.sendControl(effect.frame)
      case `alive`:
        this.alive = true
        return
      case `release`:
        return this.teardown()
      case `toast`:
        return this.onToast?.(effect.text)
    }
  }

  /** The `ticket` effect: the microphone and the codec, a key holder that is
   *  still starting (then the claim), then `voice.startCall`. */
  private async ask(generation: number): Promise<void> {
    const live = () => generation === this.generation
    let audio: CallAudio
    try {
      audio = await this.deps.openAudio()
    } catch (error) {
      return this.send({ t: `failed`, code: error instanceof CallAudioError ? error.code : `no_microphone` }, generation)
    }
    if (!live()) return audio.close()
    this.audio = audio
    audio.setMuted(this.caller.call.muted)
    audio.onLevel = (level) => {
      if (live()) this.setLevel(level)
    }
    audio.onError = () => this.send({ t: `failed`, code: `dropped` }, generation)
    audio.onPacket = (packet) => {
      if (live()) this.up(packet)
    }
    const agentId = this.agentId!
    let ticket: CallTicket
    let named: { device?: string; fallback?: boolean }
    try {
      // A key holder that is still starting is waited for, then the caller
      // claims; one that cannot hold keys dials without the claim.
      const keys = this.deps.keys
      const claim = keys ? await keys.ready().catch(() => false) : false
      if (!live()) return
      const none: CallKeyTarget = { rootId: null, epoch: null }
      const [answer, target] = await Promise.all([
        this.deps.startCall(agentId, claim),
        claim ? keys!.target(agentId).catch(() => none) : none,
      ])
      // Told sealed without the claim or the ids: no usable answer, and no clear call instead.
      if (answer.seal === true && (!claim || !answer.callId || !answer.deviceId)) throw new Error(`a sealed call this caller cannot seal`)
      ticket = {
        url: answer.url,
        sealed: answer.seal === true ? { callId: answer.callId!, deviceId: answer.deviceId!, target } : null,
      }
      named = { device: answer.device, fallback: answer.fallback }
    } catch (error) {
      // Only an availability refusal (the fixture's own words) is shown;
      // anything else (a network error, a 500) is a dropped call.
      const reason = refusalReason(error)
      return this.send(reason ? { t: `refused`, message: reason } : { t: `failed`, code: `dropped` }, generation)
    }
    if (!live()) return
    this.ticket = ticket
    // #EXP-1935: the answer names the device that speaks, as on the phones.
    this.send(
      {
        t: `ticket`,
        ...(named.device !== undefined ? { device: named.device } : {}),
        ...(named.fallback !== undefined ? { fallback: named.fallback } : {}),
        sealed: ticket.sealed !== null,
      },
      generation
    )
  }

  /** The `dial` effect: the ticket's url, and a sealed call's line. */
  private dial(generation: number) {
    const ticket = this.ticket
    if (!ticket) return
    let socket: CallSocket
    try {
      socket = this.deps.openSocket(ticket.url)
    } catch {
      return this.send({ t: `failed`, code: `dropped` }, generation)
    }
    socket.binaryType = `arraybuffer`
    this.socket = socket
    this.heardAt = Date.now()
    // EXP-1929: once the relay beat, a path that carries nothing for
    // `idleTimeoutMs` is dead: an own failure (the host is told).
    this.liveness = setInterval(() => {
      if (callPathDead(this.alive, Date.now() - this.heardAt)) this.send({ t: `failed`, code: `dropped` }, generation)
    }, LIVENESS_TICK_MS)
    socket.onmessage = (message) => {
      if (generation !== this.generation) return
      this.heardAt = Date.now()
      if (typeof message.data === `string`) return this.control(message.data, generation)
      this.binary(new Uint8Array(message.data as ArrayBuffer))
    }
    socket.onclose = () => this.send({ t: `closed` }, generation)
    socket.onerror = () => {}
    const keys = this.deps.keys
    if (ticket.sealed && keys) {
      this.line = new CallSealLine({
        ...ticket.sealed,
        agentId: this.agentId!,
        keys,
        sendBinary: (frame) => {
          if (this.socket?.readyState === OPEN) this.socket.send(frame)
        },
        end: (reason) => this.sendControl({ t: `call_end`, reason }),
        // The line told the host its own reason first.
        fail: (code) => this.send({ t: `failed`, code, told: true }, generation),
      })
    }
  }

  /** One control frame, in arrival order; a sealed call's host events are opened first. */
  private control(text: string, generation: number) {
    const frame = parseControl(text)
    if (!frame) return
    if (this.line && frame.t === `call_event`) {
      const raw = frame.event as { t?: unknown; data?: unknown } | null
      if (raw && typeof raw === `object` && raw.t !== `failed`) {
        // The host's hello, or ONE control frame of today sealed. Clear
        // content ends it (the line says so).
        const inner = this.line.event(raw)
        const opened = inner === null ? null : parseControl(inner)
        if (opened?.t !== `call_event`) return
        return this.send({ t: `frame`, frame: opened }, generation)
      }
    }
    this.send({ t: `frame`, frame }, generation)
  }

  /** One binary frame: the host's audio, in order (a late frame is worthless in a realtime call). */
  private binary(bytes: Uint8Array) {
    if (this.line) {
      // Opened here, in arrival order; a frame with no packet reaches no decoder.
      const opened = this.line.frame(bytes)
      if (opened && opened.packet.length > 0) this.audio?.play(opened.packet)
      return
    }
    const frame = parseAudioFrame(bytes)
    if (!frame || frame.kind !== KIND_DOWN) return
    if (this.lastDown !== null && !seqAfter(frame.seq, this.lastDown)) return
    this.lastDown = frame.seq
    this.audio?.play(frame.packet)
  }

  /** One microphone packet. */
  private up(packet: Uint8Array) {
    if (this.socket?.readyState !== OPEN) return
    // A sealed call's ONE gate: a frame leaves only through the sealer,
    // which exists only after the host's proof opened.
    const frame = this.line
      ? this.line.sealPacket(this.seq, this.caller.call.muted, packet)
      : this.uplink
        ? audioFrame(KIND_UP, this.seq, packet)
        : null
    if (!frame) return
    this.socket?.send(frame)
    this.seq = (this.seq + 1) & 0xffff
  }

  private sendControl(frame: CallerOutgoing) {
    if (this.socket?.readyState === OPEN) this.socket.send(JSON.stringify(frame))
  }

  private silenceRing() {
    if (this.ringTimer) clearTimeout(this.ringTimer)
    this.ringTimer = null
    const stop = this.stopRingTone
    this.stopRingTone = null
    stop?.()
  }

  /** The `release` effect: everything the call holds goes; whatever it
   *  still reports turns inert. */
  private teardown() {
    this.generation += 1
    this.agentId = null
    this.ticket = null
    this.line?.close()
    this.line = null
    const socket = this.socket
    this.socket = null
    if (socket) {
      socket.onmessage = null
      socket.onclose = null
      socket.onerror = null
      try {
        socket.close(1000, `call_end`)
      } catch {}
    }
    this.silenceRing()
    this.audio?.close()
    this.audio = null
    this.uplink = false
    this.seq = 0
    this.lastDown = null
    if (this.liveness) clearInterval(this.liveness)
    this.liveness = null
    this.alive = false
  }
}
