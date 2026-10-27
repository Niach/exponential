// NEW-70 (voice V6): one web call, streamed to the member's voice device
// (`fixtures/voice-call.json` TRANSPORT). `voice.startCall` mints the
// caller's relay ticket; the relay rings the voice device and answers
// `call_ready` once its host joined; from then on the microphone goes up as
// `up` frames and the reply comes down as `down` frames, while the host's
// `call_event`s drive the SAME `callStep` machine the voice device runs.
// Mute = `call_mute`, End call = `call_end`; a refusal, an `error` code or a
// socket that simply went away ends the call with the fixture's toast. A
// browser without WebCodecs Opus toasts `unsupported` before the microphone
// opens and never dials (Opus only: the host decodes nothing else).

import { CALL_IDLE, callStep, type CallEvent, type CallState } from "@exp/domain-contract/voice-call"

import { CallAudioError, type CallAudio } from "./audio"
import { audioFrame, hostCallEvent, KIND_DOWN, KIND_UP, parseAudioFrame, parseControl, seqAfter } from "./wire"

/** The slice of a WebSocket a call uses (the tests hand in a fake). */
export interface CallSocket {
  binaryType: BinaryType
  readyState: number
  onopen: ((event: Event) => void) | null
  onmessage: ((event: MessageEvent) => void) | null
  onclose: ((event: CloseEvent) => void) | null
  onerror: ((event: Event) => void) | null
  send(data: string | ArrayBufferLike | Uint8Array): void
  close(code?: number, reason?: string): void
}

export interface CallDeps {
  startCall(agentId: string): Promise<{ url: string }>
  openSocket(url: string): CallSocket
  openAudio(): Promise<CallAudio>
}

/** What a call shows: the flow state and the microphone's level. */
export interface CallSnapshot {
  state: CallState
  level: number
}

const OPEN = 1

export class VoiceCall {
  private snapshot: CallSnapshot = { state: CALL_IDLE, level: 0 }
  private listeners = new Set<() => void>()
  private socket: CallSocket | null = null
  private audio: CallAudio | null = null
  private ready = false
  private seq = 0
  private lastDown: number | null = null
  private generation = 0
  /** A failure's toast: the fixture's (`failed`), or a refusal the server
   *  worded itself (an availability reason). */
  onToast: ((message: string) => void) | null = null

  constructor(private readonly deps: CallDeps) {}

  subscribe = (listener: () => void) => {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  getSnapshot = () => this.snapshot

  get active(): boolean {
    return this.snapshot.state.phase !== `hidden`
  }

  private set(patch: Partial<CallSnapshot>) {
    this.snapshot = { ...this.snapshot, ...patch }
    for (const listener of this.listeners) listener()
  }

  private step(event: CallEvent) {
    const state = callStep(this.snapshot.state, event)
    this.set({ state, level: state.phase === `hidden` || state.muted ? 0 : this.snapshot.level })
    if (state.toast) this.onToast?.(state.toast)
  }

  /** Dial `agentId` on the voice device called `device`. */
  async start(agentId: string, device: string): Promise<void> {
    if (this.active) return
    const generation = ++this.generation
    this.step({ t: `start`, device })
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
    audio.onLevel = (level) => this.set({ level })
    audio.onPacket = (packet) => {
      if (!this.ready || this.socket?.readyState !== OPEN) return
      this.socket.send(audioFrame(KIND_UP, this.seq, packet))
      this.seq = (this.seq + 1) & 0xffff
    }
    let url: string
    try {
      url = (await this.deps.startCall(agentId)).url
    } catch (error) {
      if (generation !== this.generation) return
      this.teardown()
      const message = error instanceof Error ? error.message.trim() : ``
      this.step({ t: `end` })
      if (message) this.onToast?.(message)
      else this.step({ t: `failed`, code: `dropped` })
      return
    }
    if (generation !== this.generation) return
    this.dial(url, generation)
  }

  private dial(url: string, generation: number) {
    const socket = this.deps.openSocket(url)
    socket.binaryType = `arraybuffer`
    this.socket = socket
    socket.onmessage = (message) => {
      if (generation !== this.generation) return
      if (typeof message.data === `string`) return this.control(message.data)
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
        this.ready = true
        if (this.snapshot.state.muted) this.sendControl({ t: `call_mute`, muted: true })
        return this.step({ t: `connected` })
      case `call_event`: {
        const event = hostCallEvent(frame.event)
        if (!event) return
        // Barge-in: the reply still queued must not keep talking over it.
        if (event.t === `speechStarted`) this.audio?.flush()
        return this.step(event)
      }
      case `call_end`:
        this.teardown()
        return this.step({ t: `end` })
      case `error`:
        return this.fail(frame.code)
      case `bye`:
        return this.fail(frame.outcome ?? `dropped`)
    }
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
    if (this.ready) this.sendControl({ t: `call_mute`, muted })
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
    this.audio?.close()
    this.audio = null
    this.ready = false
    this.seq = 0
    this.lastDown = null
  }
}
