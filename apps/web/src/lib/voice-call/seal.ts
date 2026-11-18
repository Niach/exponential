// #EXP-1941: the CALLER's half of a sealed call (`fixtures/voice-call.json`
// `seal`; the sealer is @exp/domain-contract/call-seal, the steps are
// `callSealStep`). The server marks a call sealed (`voice.startCall` answers
// `seal`); then this line sends the caller's hello, takes the host's hello
// with its tag, resolves the chat's root and secret, and finishes. ONE gate
// holds the microphone: `sealPacket` seals only through a `CallSeal`, which
// exists only after `finish` opened the host's tag (the host's proof that it
// holds the chat's secret and saw the same two hellos). Until then nothing
// of the microphone leaves, whatever the relay sends.

import {
  CallHandshake,
  callHelloFrame,
  callHelloOfText,
  CallSealError,
  parseCallHello,
  type CallSeal,
  type OpenedAudio,
} from "@exp/domain-contract/call-seal"
import {
  CALL_SEAL_IDLE,
  callSealStep,
  VOICE_CALL_SEAL,
  type CallSealEvent,
  type CallSealState,
} from "@exp/etalk/voice-call"

/** The chat a call is keyed with, as `sessions.sendTarget {agentId}` names it. */
export interface CallKeyTarget {
  rootId: string | null
  epoch: number | null
  rotationDue?: boolean
}

/** Where a caller gets the chat's key: this browser's key holder. */
export interface CallKeySource {
  /** Waits (bounded) for a key holder that is still starting. False = this
   *  caller cannot hold keys: it dials without the claim, today's wire. */
  ready(): Promise<boolean>
  target(agentId: string): Promise<CallKeyTarget>
  /** The tree secret of (root, epoch): the keyring, else ONE pull. */
  secret(rootId: string, epoch: number): Promise<Uint8Array | null>
  /** The highest epoch this install holds for the root. */
  highestEpoch(rootId: string): number | null
}

export interface CallSealLineIo {
  callId: string
  agentId: string
  /** The host's steer device id, as `startCall` answered it. */
  deviceId: string
  keys: CallKeySource
  /** The caller's answer from before it dialled (null root = a first call). */
  target: CallKeyTarget
  sendBinary(frame: Uint8Array<ArrayBuffer>): void
  /** `call_end {reason}`. */
  end(reason: string): void
  /** The call is over with this toast code. */
  fail(code: string): void
}

const ZERO_ROOT = `00000000-0000-0000-0000-000000000000`

export class CallSealLine {
  private state: CallSealState = CALL_SEAL_IDLE
  private handshake: CallHandshake
  private hostHello: Uint8Array | null = null
  private secret: Uint8Array | null = null
  private target: CallKeyTarget
  private seal: CallSeal | null = null
  private helloTimer: ReturnType<typeof setInterval> | null = null
  private handshakeTimer: ReturnType<typeof setTimeout> | null = null
  /** Bumped by `abandon`: a key step that still blocks answers nobody. */
  private keyRun = 0
  /** Frames dropped (not well-formed, did not open) and packets replaced by an empty frame. */
  dropped = 0

  constructor(private readonly io: CallSealLineIo) {
    this.target = io.target
    this.handshake = CallHandshake.create(`caller`, io.target.rootId, io.target.rootId === null ? 0 : (io.target.epoch ?? 0))
    this.step({ t: `start`, seal: true })
  }

  /** True once `finish` opened the host's tag: the microphone may be sealed. */
  get sealed(): boolean {
    return this.seal !== null
  }

  step(event: CallSealEvent) {
    const stepped = callSealStep(`caller`, this.state, event)
    this.state = stepped.state
    for (const action of stepped.actions) this.act(action)
  }

  private act(action: string) {
    const [verb, argument] = action.split(`:`)
    switch (verb) {
      case `timer`:
        this.handshakeTimer = setTimeout(() => this.step({ t: `timeout`, timer: `handshake` }), VOICE_CALL_SEAL.handshakeMs)
        return
      case `hello`: {
        const frame = callHelloFrame(this.handshake.hello) as Uint8Array<ArrayBuffer>
        this.io.sendBinary(frame)
        this.helloTimer = setInterval(() => this.io.sendBinary(frame), VOICE_CALL_SEAL.helloEveryMs)
        return
      }
      case `helloStop`:
        return this.stopHello()
      case `key`:
        return void this.resolveKey(++this.keyRun)
      case `finish`:
        return this.finish()
      case `go`:
        return
      case `count`:
        this.dropped += 1
        return
      case `end`:
        return this.io.end(argument!)
      case `fail`:
        return this.io.fail(argument!)
      case `abandon`:
        return this.close()
    }
  }

  /** The host's hello was taken: its root and epoch against the caller's own
   *  answer (asked again once), then the secret, pulled until it is there. */
  private async resolveKey(run: number) {
    const live = () => run === this.keyRun
    try {
      const hello = parseCallHello(this.hostHello!)
      const agrees = (target: CallKeyTarget) => target.rootId === hello.rootId && target.epoch === hello.epoch && !target.rotationDue
      if (!agrees(this.target)) {
        const again = await this.io.keys.target(this.io.agentId)
        if (!live()) return
        this.target = again
        if (!agrees(again)) return this.step({ t: `noKey` })
      }
      // No rollback: never an epoch below the highest this install holds.
      const highest = this.io.keys.highestEpoch(hello.rootId)
      if (highest !== null && hello.epoch < highest) {
        console.warn(`[call] epoch_below_held root=${hello.rootId} named=${hello.epoch} held=${highest}`)
        return this.step({ t: `noKey` })
      }
      for (;;) {
        const secret = await this.io.keys.secret(hello.rootId, hello.epoch)
        if (!live()) return
        if (secret) {
          this.secret = secret
          return this.step({ t: `key` })
        }
        await new Promise((resolve) => setTimeout(resolve, VOICE_CALL_SEAL.pullEveryMs))
        if (!live()) return
      }
    } catch {
      if (live()) this.step({ t: `noKey` })
    }
  }

  /** Once. It refuses a hello that names another root or epoch than the
   *  caller's own answer and opens the tag; a refusal has used it up. */
  private finish() {
    try {
      this.seal = this.handshake.finish({
        secret: this.secret!,
        callId: this.io.callId,
        agentId: this.io.agentId,
        deviceId: this.io.deviceId,
        peerHello: this.hostHello!,
        rootId: this.target.rootId ?? ZERO_ROOT,
        epoch: this.target.epoch ?? 0,
        pad: VOICE_CALL_SEAL,
      }).seal
    } catch (error) {
      if (error instanceof CallSealError) console.warn(`[call] the handshake failed: ${error.reason}`)
      this.seal = null
    }
    this.secret = null
    this.step({ t: `finished`, ok: this.seal !== null })
  }

  ready() {
    this.step({ t: `ready` })
  }

  /** A `call_event` of the host that is not `failed`: its hello, a sealed
   *  event (answers the control frame inside, else null) or clear content. */
  event(event: { t?: unknown; data?: unknown }): string | null {
    const data = typeof event.data === `string` ? event.data : ``
    if (event.t === `hello`) {
      const hello = callHelloOfText(data)
      let ok = false
      try {
        ok = hello !== null && parseCallHello(hello).role === `host`
      } catch {}
      const taking = ok && this.state.phase === `offered`
      if (taking) this.hostHello = hello
      this.step({ t: `hello`, ok })
      return null
    }
    if (event.t !== `sealed`) {
      this.step({ t: `clear` })
      return null
    }
    let text: string | null = null
    try {
      text = this.seal?.openControl(data) ?? null
    } catch {}
    this.step({ t: `sealed`, opens: text !== null })
    return text
  }

  /** A binary frame: the opened audio, or null (dropped, or the call ended). */
  frame(bytes: Uint8Array): OpenedAudio | null {
    let opened: OpenedAudio | null = null
    try {
      opened = this.seal?.openAudio(bytes) ?? null
    } catch {}
    this.step({ t: `frame`, opens: opened !== null })
    return opened
  }

  /** One microphone packet as a sealed frame, mute inside it. Null before
   *  the host's proof. A packet above the step leaves as an empty frame with
   *  its seq: never a second frame size. */
  sealPacket(seq: number, muted: boolean, packet: Uint8Array): Uint8Array<ArrayBuffer> | null {
    if (!this.seal || this.state.phase !== `sealed`) return null
    const flags = muted ? 1 : 0
    try {
      return this.seal.sealAudio(seq, flags, packet) as Uint8Array<ArrayBuffer>
    } catch (error) {
      if (!(error instanceof CallSealError) || error.reason !== `too_large`) {
        this.step({ t: `sealed`, opens: false })
        return null
      }
      this.dropped += 1
      return this.seal.sealAudio(seq, flags, new Uint8Array(0)) as Uint8Array<ArrayBuffer>
    }
  }

  private stopHello() {
    if (this.helloTimer) clearInterval(this.helloTimer)
    this.helloTimer = null
  }

  /** The call is over: the timers stop, the key source is abandoned. */
  close() {
    this.keyRun += 1
    this.stopHello()
    if (this.handshakeTimer) clearTimeout(this.handshakeTimer)
    this.handshakeTimer = null
    this.secret = null
    this.seal = null
    this.state = { phase: `over`, failures: 0 }
  }
}
