// NEW-70 (voice V6): one web call, streamed to the member's voice device
// (`fixtures/voice-call.json` TRANSPORT). `voice.startCall` mints the
// caller's relay ticket; the relay rings the voice device and answers
// `call_ready` once its host joined; from then on the microphone goes up as
// `up` frames and the reply comes down as `down` frames, while the host's
// `call_event`s drive the SAME `callStep` machine the voice device runs.
// Mute = `call_mute` (the microphone keeps sending its silence), End call =
// `call_end`; a refusal, an `error` code, a codec error, a socket that
// simply went away or the relay's `call_end` with a reason (`idle`,
// `max_length`: the flow's `hungUp`) ends the call with the fixture's toast;
// only a reason-less `call_end` (the host hung up) ends it silently. A
// browser without WebCodecs Opus toasts `unsupported` before the microphone
// opens and never dials (Opus only: the host decodes nothing else).
//
// #EXP-1941: A SEALED CALL. A caller that holds a key holder claims `seal`;
// when `voice.startCall` answers `seal: true` (both ends can seal and the
// server turned it on) the call speaks the sealed wire of `seal` instead:
// the caller's hello after `call_ready`, the host's hello with its tag, then
// every microphone packet as ONE sealed frame of one size with mute inside
// (no `call_mute`), and the host's events and audio opened before anything
// else reads them (seal.ts). Nothing of the microphone leaves before the
// host proved the chat's key; a seal failure reads `dropped`. A code about
// this browser's OWN device (`no_microphone`, `not_granted`, `unsupported`)
// is never taken from the wire: there it reads `dropped`.
//
// S-3: a call RINGS first. The caller plays its own ring tone (EXP-1423: the
// shared cadence, looped until the ring ends, at most `maxRingMs`; the voice
// device rings nothing for a remote call) while the host warms up; the host's `call_event`s `rang` + `ready` answer it (`call_ready` only
// opens the pair: the microphone's frames start flowing then), its
// `noSound` is the mic check.

import {
  CALL_IDLE,
  callPathDead,
  callStep,
  VOICE_CALL_CONSTANTS,
  VOICE_CALL_COPY,
  type CallEvent,
  type CallState,
} from "@exp/etalk/voice-call"

import { CallAudioError, type CallAudio } from "./audio"
import { CallSealLine, type CallKeySource, type CallKeyTarget } from "./seal"
import { audioFrame, hostCallEvent, KIND_DOWN, KIND_UP, parseAudioFrame, parseControl, seqAfter, wireCode } from "./wire"

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

const OPEN = 1

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
  private snapshot: CallSnapshot = { state: CALL_IDLE }
  private listeners = new Set<() => void>()
  private level = 0
  private levelListeners = new Set<() => void>()
  private socket: CallSocket | null = null
  private audio: CallAudio | null = null
  private ready = false
  private seq = 0
  private lastDown: number | null = null
  private generation = 0
  private stopRing: (() => void) | null = null
  /** EXP-1929: the relay beats (`call_alive`), when it last sent anything,
   *  and the check that ends a path gone silent. */
  private alive = false
  /** #EXP-1941: the sealed line of a call the server marked sealed, else null (today's wire). */
  private line: CallSealLine | null = null
  private heardAt = 0
  private liveness: ReturnType<typeof setInterval> | null = null
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
    return this.snapshot.state.phase !== `hidden`
  }

  private step(event: CallEvent) {
    const state = callStep(this.snapshot.state, event)
    this.snapshot = { state }
    for (const listener of this.listeners) listener()
    if (state.phase === `hidden` || state.muted) this.setLevel(0)
    if (state.phase !== `ringing`) this.silenceRing()
    if (state.toast) this.onToast?.(state.toast)
  }

  private silenceRing() {
    const stop = this.stopRing
    this.stopRing = null
    stop?.()
  }

  /** Dial `agentId` on the desktop called `device`; `language` = the
   *  agent's voice language; EXP-1857:
   *  `fallback` = that desktop is not the agent's own (`via` shows it). */
  async start(agentId: string, device: string, language?: string, fallback = false): Promise<void> {
    if (this.active) return
    const generation = ++this.generation
    this.step({ t: `start`, device, language, fallback })
    this.stopRing = this.deps.ring?.(VOICE_CALL_CONSTANTS.maxRingMs) ?? null
    let audio: CallAudio
    try {
      audio = await this.deps.openAudio()
    } catch (error) {
      if (generation !== this.generation) return
      const code = error instanceof CallAudioError ? error.code : `no_microphone`
      return this.step({ t: `failed`, code })
    }
    if (generation !== this.generation) return audio.close()
    this.audio = audio
    audio.setMuted(this.snapshot.state.muted)
    audio.onLevel = (level) => this.setLevel(level)
    audio.onError = () => {
      if (generation === this.generation) this.fail(`dropped`)
    }
    audio.onPacket = (packet) => {
      if (this.socket?.readyState !== OPEN) return
      // A sealed call's ONE gate: a frame leaves only through the sealer,
      // which exists only after the host's proof opened.
      const frame = this.line ? this.line.sealPacket(this.seq, this.snapshot.state.muted, packet) : this.ready ? audioFrame(KIND_UP, this.seq, packet) : null
      if (!frame) return
      this.socket?.send(frame)
      this.seq = (this.seq + 1) & 0xffff
    }
    let url: string
    let sealed: { callId: string; deviceId: string; target: CallKeyTarget } | null = null
    try {
      // A key holder that is still starting is waited for, then the caller
      // claims; one that cannot hold keys dials without the claim.
      const keys = this.deps.keys
      const claim = keys ? await keys.ready().catch(() => false) : false
      if (generation !== this.generation) return
      const none: CallKeyTarget = { rootId: null, epoch: null }
      const [answer, target] = await Promise.all([
        this.deps.startCall(agentId, claim),
        claim ? keys!.target(agentId).catch(() => none) : none,
      ])
      url = answer.url
      if (answer.seal === true) {
        // Told sealed without the claim or the ids: no clear call instead.
        if (!claim || !answer.callId || !answer.deviceId) throw new Error(`a sealed call this caller cannot seal`)
        sealed = { callId: answer.callId, deviceId: answer.deviceId, target }
      }
      // #EXP-1935: the answer names the device that speaks, as on the phones.
      if (generation === this.generation && (answer.device !== undefined || answer.fallback !== undefined)) {
        const state = this.snapshot.state
        this.snapshot = {
          state: { ...state, device: answer.device ?? state.device, fallback: answer.fallback ?? state.fallback },
        }
        for (const listener of this.listeners) listener()
      }
    } catch (error) {
      if (generation !== this.generation) return
      this.teardown()
      // Only an availability refusal (the fixture's own words) is shown;
      // anything else (a network error, a 500) is a dropped call.
      const reason = refusalReason(error)
      if (!reason) return this.step({ t: `failed`, code: `dropped` })
      this.step({ t: `end` })
      this.onToast?.(reason)
      return
    }
    if (generation !== this.generation) return
    this.dial(url, generation)
    if (sealed && this.deps.keys) {
      this.line = new CallSealLine({
        ...sealed,
        agentId,
        keys: this.deps.keys,
        sendBinary: (frame) => {
          if (this.socket?.readyState === OPEN) this.socket.send(frame)
        },
        end: (reason) => this.sendControl({ t: `call_end`, reason }),
        fail: (code) => this.fail(code),
      })
    }
  }

  private dial(url: string, generation: number) {
    const socket = this.deps.openSocket(url)
    socket.binaryType = `arraybuffer`
    this.socket = socket
    this.heardAt = Date.now()
    this.liveness = setInterval(() => {
      if (generation === this.generation && callPathDead(this.alive, Date.now() - this.heardAt)) this.fail(`dropped`)
    }, 1_000)
    socket.onmessage = (message) => {
      if (generation !== this.generation) return
      this.heardAt = Date.now()
      if (typeof message.data === `string`) return this.control(message.data)
      if (this.line) {
        // Opened here, in arrival order; a frame with no packet reaches no decoder.
        const opened = this.line.frame(new Uint8Array(message.data as ArrayBuffer))
        if (opened && opened.packet.length > 0) this.audio?.play(opened.packet)
        return
      }
      const frame = parseAudioFrame(message.data as ArrayBuffer)
      if (!frame || frame.kind !== KIND_DOWN) return
      // A late frame is worthless in a realtime call.
      if (this.lastDown !== null && !seqAfter(frame.seq, this.lastDown)) return
      this.lastDown = frame.seq
      this.audio?.play(frame.packet)
    }
    socket.onclose = () => {
      if (generation !== this.generation) return
      this.fail(`dropped`)
    }
    socket.onerror = () => {}
  }

  private control(text: string) {
    const frame = parseControl(text)
    if (!frame) return
    switch (frame.t) {
      case `call_ready`:
        if (this.line) return this.line.ready()
        this.ready = true
        if (this.snapshot.state.muted) this.sendControl({ t: `call_mute`, muted: true })
        return
      case `call_event`: {
        const raw = frame.event as { t?: unknown; data?: unknown } | null
        if (this.line && raw && typeof raw === `object` && raw.t !== `failed`) {
          // A sealed call: the host's hello, or ONE control frame of today
          // sealed. Clear content ends it (the line says so).
          const inner = this.line.event(raw)
          const opened = inner === null ? null : parseControl(inner)
          if (opened?.t !== `call_event`) return
          return this.hostEvent(opened.event)
        }
        return this.hostEvent(frame.event)
      }
      case `call_end`:
        this.teardown()
        return this.step({ t: `hungUp`, reason: frame.reason ?? null })
      case `error`:
        return this.fail(wireCode(frame.code))
      case `bye`:
        return this.fail(wireCode(frame.outcome ?? `dropped`))
      case `call_alive`:
        this.alive = true
        return
    }
  }

  /** One flow event of the host (clear in today's call, opened in a sealed one). */
  private hostEvent(raw: unknown) {
    const event = hostCallEvent(raw)
    if (!event) return
    // The host failed the call (its `call_end` follows): its toast.
    if (event.t === `failed`) return this.fail(wireCode(event.code))
    // Barge-in: the reply still queued must not keep talking over it.
    if (event.t === `speechStarted`) this.audio?.flush()
    return this.step(event)
  }

  private sendControl(frame: Record<string, unknown>) {
    if (this.socket?.readyState === OPEN) this.socket.send(JSON.stringify(frame))
  }

  private fail(code: string) {
    if (!this.active) return
    this.teardown()
    this.step({ t: `failed`, code })
  }

  toggleMute() {
    if (!this.active) return
    const muted = !this.snapshot.state.muted
    this.audio?.setMuted(muted)
    // A sealed call sends no `call_mute`: mute rides inside every sealed frame.
    if (this.ready && !this.line) this.sendControl({ t: `call_mute`, muted })
    this.step({ t: muted ? `mute` : `unmute` })
  }

  /** End call (and the page going away). */
  end() {
    if (!this.active) return
    this.sendControl({ t: `call_end` })
    this.teardown()
    this.step({ t: `end` })
  }

  private teardown() {
    this.generation += 1
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
    this.ready = false
    this.seq = 0
    this.lastDown = null
    if (this.liveness) clearInterval(this.liveness)
    this.liveness = null
    this.alive = false
  }
}
