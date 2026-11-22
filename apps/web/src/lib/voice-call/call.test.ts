import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/etalk/fixtures/voice-call.json"
import { callView, type CallEvent, type CallPlatform } from "@exp/etalk/voice-call"

import { CallAudioError, type CallAudio } from "./audio"
import { VoiceCall, type CallSocket } from "./call"
import type { CallKeySource, CallKeyTarget } from "./seal"
import { CallHandshake, callHelloOfFrame, callHelloText, type CallSeal } from "@exp/domain-contract/call-seal"
import { audioFrame, KIND_DOWN, KIND_UP, parseAudioFrame, seqAfter } from "./wire"

// NEW-70: the web caller drives voice-call.json's flow off the wire: the
// relay's `call_ready`, the host's `call_event`s, `error` codes and drops,
// and its own Mute / End call, against a fake relay socket and fake audio.

class FakeSocket implements CallSocket {
  binaryType: BinaryType = `blob`
  readyState = 1
  onopen: CallSocket[`onopen`] = null
  onmessage: CallSocket[`onmessage`] = null
  onclose: CallSocket[`onclose`] = null
  onerror: CallSocket[`onerror`] = null
  sent: (string | Uint8Array)[] = []
  closed = false
  send(data: string | ArrayBufferLike | Uint8Array) {
    this.sent.push(typeof data === `string` ? data : new Uint8Array(data as ArrayBuffer))
  }
  close() {
    this.closed = true
    this.readyState = 3
  }
  text(frame: Record<string, unknown>) {
    this.onmessage?.({ data: JSON.stringify(frame) } as MessageEvent)
  }
  binary(frame: Uint8Array) {
    this.onmessage?.({ data: frame.slice().buffer } as MessageEvent)
  }
  drop() {
    this.readyState = 3
    this.onclose?.({} as CloseEvent)
  }
  controls() {
    return this.sent.filter((frame): frame is string => typeof frame === `string`).map((frame) => JSON.parse(frame))
  }
}

function fakeAudio() {
  const audio = {
    onPacket: null as CallAudio[`onPacket`],
    onLevel: null as CallAudio[`onLevel`],
    onError: null as CallAudio[`onError`],
    muted: false,
    played: [] as number[][],
    flushes: 0,
    closed: false,
    setMuted(muted: boolean) {
      audio.muted = muted
    },
    play(packet: Uint8Array) {
      audio.played.push([...packet])
    },
    flush() {
      audio.flushes += 1
    },
    close() {
      audio.closed = true
    },
  }
  return audio as typeof audio & CallAudio
}

async function dialed(
  options: {
    startCall?: () => Promise<{ url: string; device?: string; fallback?: boolean; seal?: boolean; callId?: string; deviceId?: string }>
    openAudio?: () => Promise<CallAudio>
    keys?: CallKeySource
    agentId?: string
    device?: string
    language?: string
    fallback?: boolean
  } = {}
) {
  const socket = new FakeSocket()
  const audio = fakeAudio()
  const toasts: string[] = []
  const startCall = vi.fn(options.startCall ?? (async () => ({ url: `wss://relay/ws?t=caller` })))
  const rings = { started: [] as number[], stopped: 0 }
  const call = new VoiceCall({
    startCall,
    openSocket: () => socket,
    openAudio: options.openAudio ?? (async () => audio),
    keys: options.keys,
    ring: (ms) => {
      rings.started.push(ms)
      return () => (rings.stopped += 1)
    },
  })
  call.onToast = (message) => toasts.push(message)
  await call.start(options.agentId ?? `agent-1`, options.device ?? `mint`, options.language, options.fallback)
  return { call, socket, audio, toasts, startCall, rings }
}

const view = (call: VoiceCall, platform: CallPlatform = `web`) => callView(call.getSnapshot().state, platform)

/** One fixture event, as it reaches a web caller. */
function deliver(call: VoiceCall, socket: FakeSocket, event: CallEvent) {
  switch (event.t) {
    case `start`:
      return
    case `mute`:
    case `unmute`:
      if (call.getSnapshot().state.muted !== (event.t === `mute`)) call.toggleMute()
      return
    case `end`:
      return call.end()
    case `hungUp`:
      return socket.text({ t: `call_end`, ...(event.reason ? { reason: event.reason } : {}) })
    case `failed`:
      return event.code === `dropped` ? socket.drop() : socket.text({ t: `error`, code: event.code })
    default:
      return socket.text({ t: `call_event`, event })
  }
}

// The phones' `speaker` (the loudspeaker vs the earpiece) is device-local:
// a web caller has none to replay.
const webCases = fixture.flowCases.filter((c) => !c.events.some((event) => event.t === `speaker`))

describe(`a web call replays voice-call.json flowCases off the wire`, () => {
  it.each(webCases.map((c) => [c.name, c] as const))(`%s`, async (_name, c) => {
    const [start, ...events] = c.events as CallEvent[]
    if (start?.t !== `start`) throw new Error(`a flow case starts with start`)
    // #EXP-1941: a failure about this browser's own device is the browser's
    // to find (the microphone, WebCodecs), never the wire's to say.
    const own = events.find((event) => event.t === `failed` && fixture.seal.ownDeviceCodes.includes(event.code))
    const { call, socket, toasts } = await dialed({
      device: start.device,
      language: start.language,
      fallback: start.fallback,
      ...(own?.t === `failed` ? { openAudio: () => Promise.reject(new CallAudioError(own.code as CallAudioError[`code`])) } : {}),
    })
    socket.text({ t: `call_ready` })
    for (const event of events) if (event !== own) deliver(call, socket, event)
    expect(view(call, c.platform as CallPlatform)).toEqual(c.view)
    expect(toasts).toEqual(`toast` in c ? [c.toast] : [])
  })
})

describe(`the web caller's wire`, () => {
  it(`dials the url voice.startCall answers, as web`, async () => {
    const { startCall, socket } = await dialed()
    // No key holder in these deps: a caller that cannot seal makes no claim.
    expect(startCall).toHaveBeenCalledWith(`agent-1`, false)
    expect(socket.binaryType).toBe(`arraybuffer`)
  })

  // #EXP-1935: the server picks the target; its answer names the device
  // that speaks (the phones apply it the same way).
  it(`shows the device voice.startCall picked when it is another one`, async () => {
    const { call } = await dialed({
      device: `mint`,
      startCall: async () => ({ url: `wss://relay/ws?t=caller`, device: `laptop`, fallback: true }),
    })
    expect(call.getSnapshot().state).toMatchObject({ phase: `ringing`, device: `laptop`, fallback: true })
    expect(view(call)?.via).toBe(fixture.copy.via.replace(`{device}`, `laptop`))
  })

  it(`keeps the device it started with when an older server names none`, async () => {
    const { call } = await dialed({ device: `mint`, fallback: true })
    expect(call.getSnapshot().state).toMatchObject({ device: `mint`, fallback: true })
  })

  it(`sends the microphone as up frames only once the pair is up, the sequence counting`, async () => {
    const { socket, audio } = await dialed()
    audio.onPacket?.(new Uint8Array([9]))
    expect(socket.sent).toEqual([])
    socket.text({ t: `call_ready` })
    audio.onPacket?.(new Uint8Array([1, 2]))
    audio.onPacket?.(new Uint8Array([3]))
    const frames = socket.sent.map((frame) => parseAudioFrame(frame as Uint8Array))
    expect(frames).toEqual([
      { kind: KIND_UP, seq: 0, packet: new Uint8Array([1, 2]) },
      { kind: KIND_UP, seq: 1, packet: new Uint8Array([3]) },
    ])
  })

  it(`plays down frames in order, dropping late ones and anything else`, async () => {
    const { socket, audio } = await dialed()
    socket.text({ t: `call_ready` })
    socket.binary(audioFrame(KIND_DOWN, 0xffff, new Uint8Array([1])))
    socket.binary(audioFrame(KIND_DOWN, 0, new Uint8Array([2])))
    socket.binary(audioFrame(KIND_DOWN, 0xffff, new Uint8Array([3])))
    socket.binary(audioFrame(KIND_UP, 1, new Uint8Array([4])))
    socket.binary(new Uint8Array([KIND_DOWN, 0, 2]))
    expect(audio.played).toEqual([[1], [2]])
  })

  it(`speechStarted flushes the reply still queued (barge-in)`, async () => {
    const { socket, audio } = await dialed()
    socket.text({ t: `call_ready` })
    socket.text({ t: `call_event`, event: { t: `replyStarted`, text: `Three.` } })
    socket.text({ t: `call_event`, event: { t: `speechStarted` } })
    expect(audio.flushes).toBe(1)
  })

  it(`#EXP-1936: a host's start, mute, unmute, speaker and a malformed event are not the caller's`, async () => {
    const { call, socket } = await dialed()
    socket.text({ t: `call_ready` })
    for (const event of [{ t: `start`, device: `elsewhere` }, { t: `mute` }, { t: `unmute` }, { t: `speaker`, on: false }, { t: `said` }, { t: `newThing` }]) {
      socket.text({ t: `call_event`, event })
    }
    expect(call.getSnapshot().state).toMatchObject({ phase: `ringing`, device: `mint`, muted: false, speaker: true })
    expect(socket.controls()).toEqual([])
  })

  it(`EXP-1423: rings from the dial until answered (at most maxRingMs); call_ready answers nothing, the host's rang + ready do`, async () => {
    const { call, socket, rings } = await dialed()
    expect(rings).toEqual({ started: [fixture.constants.maxRingMs], stopped: 0 })
    socket.text({ t: `call_ready` })
    expect(view(call)?.status).toBe(fixture.copy.calling)
    socket.text({ t: `call_event`, event: { t: `rang` } })
    expect(view(call)?.status).toBe(fixture.copy.gettingReady)
    expect(rings.stopped).toBe(0)
    socket.text({ t: `call_event`, event: { t: `ready` } })
    expect(view(call)).toMatchObject({ activity: `listening`, caption: ``, buttons: [`mute`, `end`] })
    expect(rings.stopped).toBe(1)
  })

  it(`S-3: Cancel while ringing silences the ring`, async () => {
    const { call, rings } = await dialed()
    call.end()
    expect(view(call)).toBeNull()
    expect(rings.stopped).toBe(1)
  })

  it(`mute closes the microphone; call_mute goes only once paired (voice-call.json caller)`, async () => {
    const { call, socket, audio } = await dialed()
    call.toggleMute()
    expect(audio.muted).toBe(true)
    socket.text({ t: `call_ready` })
    expect(socket.controls()).toEqual([])
    call.toggleMute()
    expect(socket.controls()).toEqual([{ t: `call_mute`, muted: false }])
    expect(audio.muted).toBe(false)
    call.toggleMute()
    expect(socket.controls().at(-1)).toEqual({ t: `call_mute`, muted: true })
    expect(audio.muted).toBe(true)
  })

  it(`End call sends call_end, closes the socket and the audio`, async () => {
    const { call, socket, audio } = await dialed()
    socket.text({ t: `call_ready` })
    call.end()
    expect(socket.controls()).toEqual([{ t: `call_end` }])
    expect(socket.closed).toBe(true)
    expect(audio.closed).toBe(true)
    expect(call.active).toBe(false)
  })

  it(`the host hanging up ends it without a toast`, async () => {
    const { call, socket, toasts } = await dialed()
    socket.text({ t: `call_ready` })
    socket.text({ t: `call_end` })
    expect(call.active).toBe(false)
    expect(toasts).toEqual([])
  })

  it(`a relay refusal ends it with the fixture's toast`, async () => {
    const { socket, toasts } = await dialed()
    socket.text({ t: `error`, code: `device_offline` })
    socket.text({ t: `bye`, outcome: `device_offline` })
    expect(toasts).toEqual([fixture.copy.failed.device_offline.replace(`{device}`, `mint`)])
  })

  it(`the microphone refused or missing ends it with the fixture's toast`, async () => {
    for (const code of [`not_granted`, `no_microphone`] as const) {
      const { call, toasts, startCall } = await dialed({
        openAudio: async () => {
          throw new CallAudioError(code)
        },
      })
      expect(call.active).toBe(false)
      expect(startCall).not.toHaveBeenCalled()
      expect(toasts).toEqual([fixture.copy.failed[code]])
    }
  })

  it(`a browser without WebCodecs Opus toasts unsupported and never dials`, async () => {
    const { call, toasts, startCall } = await dialed({
      openAudio: async () => {
        throw new CallAudioError(`unsupported`)
      },
    })
    expect(call.active).toBe(false)
    expect(startCall).not.toHaveBeenCalled()
    expect(toasts).toEqual([fixture.copy.failed.unsupported])
  })

  it(`the relay's busy refusal toasts the device's other call`, async () => {
    const { socket, toasts } = await dialed()
    socket.text({ t: `error`, code: `busy` })
    expect(toasts).toEqual([fixture.copy.failed.busy.replace(`{device}`, `mint`)])
  })

  it(`a host's failed event (then its call_end) ends it with the fixture's toast`, async () => {
    const cases = [
      [`unavailable`, fixture.copy.failed.unavailable.replace(`{device}`, `mint`)],
      [`meeting`, fixture.copy.failed.meeting.replace(`{device}`, `mint`)],
      [`no_such_code`, fixture.copy.failed.dropped],
    ] as const
    for (const [code, toast] of cases) {
      const { call, socket, audio, toasts } = await dialed()
      socket.text({ t: `call_ready` })
      socket.text({ t: `call_event`, event: { t: `rang` } })
      socket.text({ t: `call_event`, event: { t: `failed`, code } })
      socket.text({ t: `call_end` })
      expect(call.active).toBe(false)
      expect(audio.closed).toBe(true)
      expect(socket.closed).toBe(true)
      expect(toasts).toEqual([toast])
    }
  })

  it(`the relay's idle timeout or max length toasts dropped, muted or not`, async () => {
    for (const reason of [`idle`, `max_length`]) {
      const { call, socket, audio, toasts } = await dialed()
      socket.text({ t: `call_ready` })
      call.toggleMute()
      socket.text({ t: `call_end`, reason })
      expect(call.active).toBe(false)
      expect(audio.closed).toBe(true)
      expect(toasts).toEqual([fixture.copy.failed.dropped])
    }
  })

  it(`muted, the microphone's frames (its silence) keep flowing`, async () => {
    const { call, socket, audio } = await dialed()
    socket.text({ t: `call_ready` })
    call.toggleMute()
    audio.onPacket?.(new Uint8Array([0]))
    expect(socket.sent.filter((frame) => typeof frame !== `string`)).toHaveLength(1)
  })

  it(`a call muted for 40 s outlives the relay's idle check: the frames keep flowing`, async () => {
    vi.useFakeTimers()
    try {
      const { idleTimeoutMs, frameMs } = fixture.constants
      for (const microphone of [`silence`, `nothing`] as const) {
        const { call, socket, audio, toasts } = await dialed()
        socket.text({ t: `call_ready` })
        // The relay's idle check (steer-relay hub.ts): every
        // min(idleTimeoutMs, 5 s), a pair without a frame for idleTimeoutMs
        // ends with `call_end` idle.
        let lastFrameAt = Date.now()
        const send = socket.send.bind(socket)
        socket.send = (data) => {
          if (typeof data !== `string`) lastFrameAt = Date.now()
          send(data)
        }
        const relay = setInterval(() => {
          if (call.active && Date.now() - lastFrameAt >= idleTimeoutMs) socket.text({ t: `call_end`, reason: `idle` })
        }, Math.min(idleTimeoutMs, 5_000))
        // The muted microphone: a disabled track still encodes one frame
        // per frameMs (audio.test: "muted, the track is disabled and the
        // frames keep flowing"); `nothing` = a client that stops sending.
        const capture = setInterval(() => {
          if (microphone === `silence`) audio.onPacket?.(new Uint8Array([0]))
        }, frameMs)
        call.toggleMute()
        vi.advanceTimersByTime(40_000)
        clearInterval(relay)
        clearInterval(capture)
        if (microphone === `silence`) {
          expect(call.active).toBe(true)
          expect(call.getSnapshot().state.muted).toBe(true)
          expect(socket.sent.filter((frame) => typeof frame !== `string`).length).toBeGreaterThanOrEqual(40_000 / frameMs - 1)
          expect(toasts).toEqual([])
          call.end()
        } else {
          expect(call.active).toBe(false)
          expect(toasts).toEqual([fixture.copy.failed.dropped])
        }
      }
    } finally {
      vi.useRealTimers()
    }
  })

  // EXP-1929 (voice-phones-2): once the relay beats, a path that carries
  // nothing for idleTimeoutMs is dead; an older relay never beats.
  it(`a path gone silent after the relay's beat ends dropped; without a beat it never does`, async () => {
    vi.useFakeTimers()
    try {
      const { idleTimeoutMs } = fixture.constants
      const beating = await dialed()
      beating.socket.text({ t: `call_ready` })
      beating.socket.text({ t: `call_alive` })
      vi.advanceTimersByTime(idleTimeoutMs - 1_000)
      expect(beating.call.active).toBe(true)
      beating.socket.binary(audioFrame(KIND_DOWN, 0, new Uint8Array([1])))
      vi.advanceTimersByTime(idleTimeoutMs - 1_000)
      expect(beating.call.active).toBe(true)
      vi.advanceTimersByTime(2_000)
      expect(beating.call.active).toBe(false)
      expect(beating.audio.closed).toBe(true)
      expect(beating.toasts).toEqual([fixture.copy.failed.dropped])
      // #EXP-2017: an own failure: the host hears it from the caller first.
      expect(beating.socket.controls()).toEqual([{ t: `call_end`, reason: `dropped` }])
      expect(beating.socket.closed).toBe(true)
      const older = await dialed()
      older.socket.text({ t: `call_ready` })
      vi.advanceTimersByTime(3 * idleTimeoutMs)
      expect(older.call.active).toBe(true)
      older.call.end()
    } finally {
      vi.useRealTimers()
    }
  })

  it(`a socket that simply goes away closes the audio and toasts dropped`, async () => {
    const { call, socket, audio, toasts } = await dialed()
    socket.text({ t: `call_ready` })
    socket.drop()
    expect(call.active).toBe(false)
    expect(audio.closed).toBe(true)
    expect(toasts).toEqual([fixture.copy.failed.dropped])
  })

  it(`a codec error ends it as dropped, tells the host and closes the microphone`, async () => {
    const { call, socket, audio, toasts } = await dialed()
    socket.text({ t: `call_ready` })
    audio.onError?.()
    expect(call.active).toBe(false)
    expect(audio.closed).toBe(true)
    expect(socket.controls()).toEqual([{ t: `call_end`, reason: `dropped` }])
    expect(socket.closed).toBe(true)
    expect(toasts).toEqual([fixture.copy.failed.dropped])
  })

  it(`End call while the microphone prompt is open never dials and frees the microphone once granted`, async () => {
    let grant: (audio: CallAudio) => void = () => {}
    const audio = fakeAudio()
    const startCall = vi.fn(async () => ({ url: `wss://relay` }))
    const call = new VoiceCall({
      startCall,
      openSocket: () => new FakeSocket(),
      openAudio: () => new Promise<CallAudio>((resolve) => (grant = resolve)),
    })
    const toasts: string[] = []
    call.onToast = (message) => toasts.push(message)
    const started = call.start(`agent-1`, `mint`)
    expect(view(call)?.status).toBe(fixture.copy.calling)
    call.end()
    expect(call.active).toBe(false)
    grant(audio)
    await started
    expect(audio.closed).toBe(true)
    expect(startCall).not.toHaveBeenCalled()
    expect(toasts).toEqual([])
  })

  it(`the level is its own store: it never wakes the call's subscribers`, async () => {
    const { call, socket, audio } = await dialed()
    socket.text({ t: `call_ready` })
    const onState = vi.fn()
    const onLevel = vi.fn()
    call.subscribe(onState)
    call.subscribeLevel(onLevel)
    audio.onLevel?.(0.5)
    expect(call.getLevel()).toBe(0.5)
    expect(onLevel).toHaveBeenCalledOnce()
    expect(onState).not.toHaveBeenCalled()
    call.toggleMute()
    expect(call.getLevel()).toBe(0)
  })

  it(`voice.startCall failing for any other reason toasts dropped, never its raw message`, async () => {
    const { call, audio, toasts } = await dialed({
      startCall: async () => {
        throw new Error(`Failed to fetch`)
      },
    })
    expect(call.active).toBe(false)
    expect(audio.closed).toBe(true)
    expect(toasts).toEqual([fixture.copy.failed.dropped])
  })

  it(`voice.startCall refusing toasts its reason and frees the microphone`, async () => {
    const reason = fixture.copy.reasons.offline.replace(`{device}`, `mint`)
    const { call, audio, toasts } = await dialed({
      startCall: async () => {
        throw new Error(reason)
      },
    })
    expect(call.active).toBe(false)
    expect(audio.closed).toBe(true)
    expect(toasts).toEqual([reason])
  })

  it(`the sequence wraps`, () => {
    expect(seqAfter(0, 0xffff)).toBe(true)
    expect(seqAfter(0xffff, 0)).toBe(false)
    expect(seqAfter(5, 5)).toBe(false)
  })
})

// #EXP-1941: a sealed call, against a scripted host built on the TS reference.
describe(`#EXP-1941 a sealed web call`, () => {
  const seal = fixture.seal
  const i = seal.inputs
  const AGENT = i.agentId
  const secret = Uint8Array.from(Buffer.from(i.secret, `hex`))
  const ROOT: CallKeyTarget = { rootId: i.rootId, epoch: i.epoch }
  const dropped = fixture.copy.failed.dropped
  const settle = async () => {
    for (let n = 0; n < 6; n++) await Promise.resolve()
  }

  /** A key holder that holds the chat's secret, and what was asked of it. */
  function keySource(patch: Partial<CallKeySource> & { targets?: CallKeyTarget[] } = {}) {
    const targets = [...(patch.targets ?? [ROOT])]
    const asked = { targets: 0, secrets: 0 }
    const keys: CallKeySource = {
      ready: async () => true,
      target: async () => {
        asked.targets += 1
        return targets.length > 1 ? targets.shift()! : targets[0]!
      },
      secret: async () => {
        asked.secrets += 1
        return secret
      },
      highestEpoch: () => null,
      ...patch,
    }
    return { keys, asked }
  }

  /** Dials a call the server marked sealed; `host()` answers the caller's hello as the real host would. */
  async function sealedCall(options: { keys?: CallKeySource; hostRoot?: string; hostEpoch?: number; hostSecret?: Uint8Array } = {}) {
    const source = options.keys ? { keys: options.keys, asked: { targets: 0, secrets: 0 } } : keySource()
    const line = await dialed({
      agentId: AGENT,
      keys: source.keys,
      startCall: async () => ({ url: `wss://relay/ws?t=caller`, seal: true, callId: i.callId, deviceId: i.deviceId }),
    })
    const binary = () => line.socket.sent.filter((frame): frame is Uint8Array => typeof frame !== `string`)
    let hostSeal: CallSeal | null = null
    return {
      ...line,
      asked: source.asked,
      binary,
      /** Everything binary but the hello frames (type 1). */
      audioFrames: () => binary().filter((frame) => frame[1] === 0),
      hostSeal: () => hostSeal!,
      /** The host's hello for the caller's, sent down as the `hello` event; answers its bytes. */
      host(tamper: (hello: Uint8Array) => Uint8Array = (hello) => hello) {
        const callerHello = callHelloOfFrame(binary()[0]!)!
        const hosted = CallHandshake.create(`host`, options.hostRoot ?? i.rootId, options.hostEpoch ?? i.epoch).finish({
          secret: options.hostSecret ?? secret,
          callId: i.callId,
          agentId: AGENT,
          deviceId: i.deviceId,
          peerHello: callerHello,
          pad: seal,
        })
        hostSeal = hosted.seal
        line.socket.text({ t: `call_event`, event: { t: `hello`, data: callHelloText(tamper(hosted.hostHello)) } })
        return hosted.hostHello
      },
      sealedEvent(event: Record<string, unknown>) {
        line.socket.text({ t: `call_event`, event: { t: `sealed`, data: hostSeal!.sealControl(JSON.stringify({ t: `call_event`, event })) } })
      },
    }
  }

  it(`claims seal with a key holder; the hello after call_ready, again every helloEveryMs, until the host's is taken`, async () => {
    vi.useFakeTimers()
    try {
      const line = await sealedCall()
      expect(line.startCall).toHaveBeenCalledWith(AGENT, true)
      // Before call_ready nothing leaves, though the microphone runs.
      line.audio.onPacket?.(new Uint8Array([1, 2, 3]))
      expect(line.socket.sent).toEqual([])
      line.socket.text({ t: `call_ready` })
      expect(line.binary()).toHaveLength(1)
      expect([line.binary()[0]![0], line.binary()[0]![1], line.binary()[0]!.length]).toEqual([KIND_UP, 1, 56])
      vi.advanceTimersByTime(seal.helloEveryMs * 2)
      expect(line.binary()).toHaveLength(3)
      expect(line.binary()[2]).toEqual(line.binary()[0])
      // Still no microphone: the pair is up, the host has proven nothing.
      line.audio.onPacket?.(new Uint8Array([1, 2, 3]))
      expect(line.audioFrames()).toEqual([])
      line.socket.text({ t: `call_ready` })
      expect(line.binary()).toHaveLength(3)
    } finally {
      vi.useRealTimers()
    }
  })

  it(`the whole call: the microphone sealed in one frame size with mute inside, events and audio opened, no call_mute`, async () => {
    const line = await sealedCall()
    line.socket.text({ t: `call_ready` })
    line.host()
    await settle()
    expect(line.asked).toEqual({ targets: 1, secrets: 1 })

    line.audio.onPacket?.(new Uint8Array([0xf8, 1, 2]))
    line.call.toggleMute()
    line.audio.onPacket?.(new Uint8Array([0xf8, 3]))
    // A packet above the step leaves as an empty frame with its seq: never a second size.
    line.audio.onPacket?.(new Uint8Array(seal.audioPadStep.up + 1))
    const frames = line.audioFrames()
    expect(frames.map((frame) => frame.length)).toEqual(Array(3).fill(27 + seal.audioPadStep.up))
    const heard = frames.map((frame) => line.hostSeal().openAudio(frame))
    expect(heard.map((frame) => [frame.seq, frame.flags, [...frame.packet], frame.missing])).toEqual([
      [0, 0, [0xf8, 1, 2], 0],
      [1, 1, [0xf8, 3], 0],
      [2, 1, [], 1],
    ])
    expect(line.socket.controls()).toEqual([])

    for (const event of [{ t: `rang` }, { t: `ready` }, { t: `speechStarted` }, { t: `said`, text: `hello there` }]) line.sealedEvent(event)
    expect(view(line.call)?.caption).toBe(`“hello there”`)
    line.socket.binary(line.hostSeal().sealAudio(0, 0, new Uint8Array([9, 9])))
    line.socket.binary(line.hostSeal().sealAudio(1, 0, new Uint8Array(0)))
    expect(line.audio.played).toEqual([[9, 9]])
    // A host's failed inside a sealed event keeps its toast; one about this browser reads dropped.
    line.sealedEvent({ t: `failed`, code: `not_granted` })
    expect(line.toasts).toEqual([dropped])
  })

  it(`a host hello whose tag does not open: no frame leaves, the call drops`, async () => {
    const line = await sealedCall()
    line.socket.text({ t: `call_ready` })
    line.host((hello) => Uint8Array.of(...hello.slice(0, 69), hello[69]! ^ 1))
    await settle()
    line.audio.onPacket?.(new Uint8Array([1, 2, 3]))
    expect(line.audioFrames()).toEqual([])
    expect(line.socket.controls()).toEqual([{ t: `call_end`, reason: seal.endReasons.seal }])
    expect(line.toasts).toEqual([dropped])
    expect(line.call.active).toBe(false)
  })

  it(`a host hello that names another root this keyring holds: no frame leaves, the call drops`, async () => {
    const other = `44444444-4444-4444-8444-444444444444`
    const source = keySource()
    const line = await sealedCall({ keys: source.keys, hostRoot: other })
    line.socket.text({ t: `call_ready` })
    line.host()
    await settle()
    line.audio.onPacket?.(new Uint8Array([1, 2, 3]))
    expect(line.audioFrames()).toEqual([])
    // Asked the server again, once; never the keyring for the hello's root.
    expect(source.asked).toEqual({ targets: 2, secrets: 0 })
    expect(line.socket.controls()).toEqual([{ t: `call_end`, reason: seal.endReasons.noKey }])
    expect(line.toasts).toEqual([dropped])
  })

  it(`a first call: the hello carries zeros; the answer asked again names the root the host created`, async () => {
    const source = keySource({ targets: [{ rootId: null, epoch: null }, ROOT] })
    const line = await sealedCall({ keys: source.keys })
    line.socket.text({ t: `call_ready` })
    expect([...callHelloOfFrame(line.binary()[0]!)!.slice(2, 22)]).toEqual(Array(20).fill(0))
    line.host()
    await settle()
    expect(source.asked.targets).toBe(2)
    line.audio.onPacket?.(new Uint8Array([7]))
    expect(line.hostSeal().openAudio(line.audioFrames()[0]!).packet).toEqual(new Uint8Array([7]))
  })

  it(`rotationDue: asked again once; still due fails the call, cleared goes on`, async () => {
    const due = { ...ROOT, rotationDue: true }
    const failing = await sealedCall({ keys: keySource({ targets: [due, due] }).keys })
    failing.socket.text({ t: `call_ready` })
    failing.host()
    await settle()
    expect(failing.socket.controls()).toEqual([{ t: `call_end`, reason: seal.endReasons.noKey }])
    expect(failing.toasts).toEqual([dropped])

    const fine = await sealedCall({ keys: keySource({ targets: [due, ROOT] }).keys })
    fine.socket.text({ t: `call_ready` })
    fine.host()
    await settle()
    fine.audio.onPacket?.(new Uint8Array([7]))
    expect(fine.audioFrames()).toHaveLength(1)
    expect(fine.toasts).toEqual([])
  })

  it(`an epoch below the highest this install holds is refused`, async () => {
    const line = await sealedCall({ keys: keySource({ highestEpoch: () => i.epoch + 1 }).keys })
    const warn = vi.spyOn(console, `warn`).mockImplementation(() => {})
    line.socket.text({ t: `call_ready` })
    line.host()
    await settle()
    expect(warn.mock.calls[0]![0]).toBe(`[call] epoch_below_held root=${i.rootId} named=${i.epoch} held=${i.epoch + 1}`)
    warn.mockRestore()
    expect(line.toasts).toEqual([dropped])
    expect(line.audioFrames()).toEqual([])
  })

  it(`the secret arrives on a later pull, every pullEveryMs`, async () => {
    vi.useFakeTimers()
    try {
        let pulls = 0
      const line = await sealedCall({ keys: keySource({ secret: async () => (++pulls < 3 ? null : secret) }).keys })
        line.socket.text({ t: `call_ready` })
      line.host()
      await vi.advanceTimersByTimeAsync(seal.pullEveryMs * 2 + 10)
      expect(pulls).toBe(3)
      line.audio.onPacket?.(new Uint8Array([7]))
      expect(line.audioFrames()).toHaveLength(1)
    } finally {
      vi.useRealTimers()
    }
  })

  it(`no hello it can finish within handshakeMs of call_ready: no_key and dropped, though the relay still beats`, async () => {
    vi.useFakeTimers()
    try {
      const line = await sealedCall()
      line.socket.text({ t: `call_ready` })
      for (let at = 0; at < seal.handshakeMs; at += fixture.constants.aliveMs) {
        expect(line.call.active).toBe(true)
        line.socket.text({ t: `call_alive` })
        vi.advanceTimersByTime(fixture.constants.aliveMs)
      }
      expect(line.socket.controls()).toEqual([{ t: `call_end`, reason: seal.endReasons.noKey }])
      expect(line.toasts).toEqual([dropped])
      expect(line.audioFrames()).toEqual([])
    } finally {
      vi.useRealTimers()
    }
  })

  it(`clear content, a sealed event or a binary frame before it is sealed ends the call; clear content after it too`, async () => {
    const early: ((line: Awaited<ReturnType<typeof sealedCall>>) => void)[] = [
      (line) => line.socket.text({ t: `call_event`, event: { t: `said`, text: `forged` } }),
      (line) => line.socket.text({ t: `call_event`, event: { t: `sealed`, data: `AAAA` } }),
      (line) => line.socket.binary(audioFrame(KIND_DOWN, 0, new Uint8Array([1, 2, 3]))),
    ]
    for (const inject of early) {
      const line = await sealedCall()
      line.socket.text({ t: `call_ready` })
      inject(line)
      expect(line.call.active).toBe(false)
      expect(line.audio.played).toEqual([])
      expect(line.socket.controls()).toEqual([{ t: `call_end`, reason: seal.endReasons.seal }])
      expect(line.toasts).toEqual([dropped])
    }
    const line = await sealedCall()
    line.socket.text({ t: `call_ready` })
    line.host()
    await settle()
    // A clear frame of today's wire does not open: dropped, never played.
    line.socket.binary(audioFrame(KIND_DOWN, 0, new Uint8Array([1, 2, 3])))
    expect(line.audio.played).toEqual([])
    expect(line.call.active).toBe(true)
    line.socket.text({ t: `call_event`, event: { t: `replyStarted`, text: `forged` } })
    expect(line.socket.controls()).toEqual([{ t: `call_end`, reason: seal.endReasons.seal }])
    expect(line.toasts).toEqual([dropped])
  })

  it(`a clear failed still ends a sealed call with its toast, before any key`, async () => {
    const line = await sealedCall()
    line.socket.text({ t: `call_ready` })
    line.socket.text({ t: `call_event`, event: { t: `failed`, code: `busy` } })
    expect(line.toasts).toEqual([fixture.copy.failed.busy.replace(`{device}`, `mint`)])
  })

  it(`a key holder that is not there in time: no claim, today's wire; told sealed without the claim: no call`, async () => {
    const none = await dialed({ keys: keySource({ ready: async () => false }).keys })
    expect(none.startCall).toHaveBeenCalledWith(`agent-1`, false)
    none.socket.text({ t: `call_ready` })
    none.audio.onPacket?.(new Uint8Array([1, 2, 3]))
    expect(parseAudioFrame(none.socket.sent[0] as Uint8Array)).toMatchObject({ kind: KIND_UP, seq: 0 })
    // The server did not mark the call sealed (no cap, or sealing is off): today's wire, though it claimed.
    const off = await dialed({ keys: keySource().keys })
    expect(off.startCall).toHaveBeenCalledWith(`agent-1`, true)
    off.socket.text({ t: `call_ready` })
    off.call.toggleMute()
    expect(off.socket.controls()).toEqual([{ t: `call_mute`, muted: true }])
    const forced = await dialed({
      keys: keySource({ ready: async () => false }).keys,
      startCall: async () => ({ url: `wss://relay/ws?t=caller`, seal: true, callId: i.callId, deviceId: i.deviceId }),
    })
    expect(forced.call.active).toBe(false)
    expect(forced.toasts).toEqual([dropped])
  })

  it(`a wire code about this browser's own device reads dropped, in any call`, async () => {
    for (const frame of [{ t: `error`, code: `not_granted` }, { t: `bye`, outcome: `unsupported` }, { t: `call_event`, event: { t: `failed`, code: `no_microphone` } }]) {
      const { socket, toasts } = await dialed()
      socket.text({ t: `call_ready` })
      socket.text(frame)
      expect(toasts).toEqual([dropped])
    }
  })
})

// #EXP-2017: the three ways the web caller differed from voice-call.json
// `caller` (a host's end / hungUp, an own failure's call_end, the ring's
// bound), against a fake relay socket and a fake clock.
describe(`#EXP-2017 the web caller ends a call as caller.cases do`, () => {
  const dropped = fixture.copy.failed.dropped
  const unavailable = fixture.copy.failed.unavailable.replace(`{device}`, `mint`)
  const { maxRingMs } = fixture.constants
  const answer = (socket: FakeSocket) => {
    socket.text({ t: `call_ready` })
    socket.text({ t: `call_event`, event: { t: `rang` } })
    socket.text({ t: `call_event`, event: { t: `ready` } })
  }

  it(`#EXP-1936: a host's end event ends the call as a reason-less call_end would: silently, nothing goes back, everything is let go`, async () => {
    const { call, socket, audio, rings, toasts } = await dialed()
    answer(socket)
    expect(view(call)?.activity).toBe(`listening`)
    socket.text({ t: `call_event`, event: { t: `end` } })
    expect(call.active).toBe(false)
    expect(view(call)).toBeNull()
    expect(socket.controls()).toEqual([])
    expect([socket.closed, audio.closed, rings.stopped]).toEqual([true, true, 1])
    expect(toasts).toEqual([])
    // Every later frame of that call is ignored.
    socket.text({ t: `call_event`, event: { t: `said`, text: `late` } })
    expect(call.active).toBe(false)
  })

  it(`#EXP-1936: a host's hungUp with a reason fails it dropped; without one it ends silently, also while it still rings`, async () => {
    const listening = await dialed()
    answer(listening.socket)
    listening.socket.text({ t: `call_event`, event: { t: `hungUp`, reason: `idle` } })
    expect(listening.call.active).toBe(false)
    expect(listening.toasts).toEqual([dropped])
    expect(listening.socket.controls()).toEqual([])
    expect([listening.socket.closed, listening.audio.closed]).toEqual([true, true])

    const ringing = await dialed()
    ringing.socket.text({ t: `call_ready` })
    ringing.socket.text({ t: `call_event`, event: { t: `hungUp` } })
    expect(ringing.call.active).toBe(false)
    expect(ringing.toasts).toEqual([])
    expect(ringing.socket.controls()).toEqual([])
    expect(ringing.rings.stopped).toBe(1)
  })

  it(`an own failure once dialled tells the host call_end {reason: dropped} before it lets go; while the socket still connects nothing is sent`, async () => {
    // Dialled, not paired yet: the codec failed.
    const dialledOnly = await dialed()
    dialledOnly.audio.onError?.()
    expect(dialledOnly.socket.controls()).toEqual([{ t: `call_end`, reason: `dropped` }])
    expect(dialledOnly.socket.closed).toBe(true)
    expect(dialledOnly.toasts).toEqual([dropped])

    const connecting = await dialed()
    connecting.socket.readyState = 0
    connecting.audio.onError?.()
    expect(connecting.socket.sent).toEqual([])
    expect(connecting.socket.closed).toBe(true)
    expect(connecting.call.active).toBe(false)
  })

  it(`an own failure after the far side ended is no second end: nothing more is sent`, async () => {
    const { call, socket, audio, toasts } = await dialed()
    answer(socket)
    socket.text({ t: `call_end` })
    audio.onError?.()
    expect(call.active).toBe(false)
    expect(socket.controls()).toEqual([])
    expect(toasts).toEqual([])
  })

  it(`before the dial there is nobody to tell: a failed startCall or a refused microphone sends nothing, dials nothing`, async () => {
    const opened: string[] = []
    for (const deps of [
      { startCall: async () => Promise.reject(new Error(`Failed to fetch`)) },
      { openAudio: async () => Promise.reject(new CallAudioError(`not_granted`)) },
    ]) {
      const call = new VoiceCall({
        startCall: async () => ({ url: `wss://relay` }),
        openAudio: async () => fakeAudio(),
        ...deps,
        openSocket: (url) => {
          opened.push(url)
          return new FakeSocket()
        },
      })
      await call.start(`agent-1`, `mint`)
      expect(call.active).toBe(false)
    }
    expect(opened).toEqual([])
  })

  it(`the ring's bound: still ringing at maxRingMs fails unavailable and tells the host`, async () => {
    vi.useFakeTimers()
    try {
      const { call, socket, audio, rings, toasts } = await dialed()
      socket.text({ t: `call_ready` })
      socket.text({ t: `call_event`, event: { t: `rang` } })
      vi.advanceTimersByTime(maxRingMs - 1)
      expect(call.active).toBe(true)
      expect(view(call)?.status).toBe(fixture.copy.gettingReady)
      expect(socket.controls()).toEqual([])
      vi.advanceTimersByTime(1)
      expect(call.active).toBe(false)
      expect(socket.controls()).toEqual([{ t: `call_end`, reason: `dropped` }])
      expect(toasts).toEqual([unavailable])
      expect([socket.closed, audio.closed, rings.stopped]).toEqual([true, true, 1])
    } finally {
      vi.useRealTimers()
    }
  })

  it(`the ring's bound passes an answered call by`, async () => {
    vi.useFakeTimers()
    try {
      const { call, socket, toasts } = await dialed()
      answer(socket)
      vi.advanceTimersByTime(2 * maxRingMs)
      expect(call.active).toBe(true)
      expect(view(call)?.activity).toBe(`listening`)
      expect(socket.controls()).toEqual([])
      expect(toasts).toEqual([])
      call.end()
    } finally {
      vi.useRealTimers()
    }
  })

  it(`the ring's bound before the dial (startCall never answers): unavailable, nothing dialled, nobody to tell`, async () => {
    vi.useFakeTimers()
    try {
      const audio = fakeAudio()
      const openSocket = vi.fn(() => new FakeSocket())
      const toasts: string[] = []
      const call = new VoiceCall({ startCall: () => new Promise(() => {}), openSocket, openAudio: async () => audio })
      call.onToast = (message) => toasts.push(message)
      void call.start(`agent-1`, `mint`)
      await vi.advanceTimersByTimeAsync(maxRingMs)
      expect(call.active).toBe(false)
      expect(toasts).toEqual([unavailable])
      expect(audio.closed).toBe(true)
      expect(openSocket).not.toHaveBeenCalled()
    } finally {
      vi.useRealTimers()
    }
  })
})

// #EXP-2017: every clear `caller.cases` row through the REAL web caller with
// a fake relay socket, microphone, ring and clock (Android: the same rows
// through `VoiceCallDriver`): each fixture event becomes what the page would
// see, and what the call then does is asserted against the row's effects.
// Not replayed here: a hold's refusal or an End during it (a browser holds
// nothing: its hold answers `held ok` at once, iOS's way), the loudspeaker
// (a browser has no earpiece) and the sealed rows (the sealed suite above
// runs the real line against a scripted host).
describe(`#EXP-2017 the web caller replays caller.cases`, () => {
  type Row = (typeof fixture.caller.cases)[number]
  type Event = Row[`events`][number] & Record<string, unknown>
  const holdsOnly = (row: Row) => {
    const events = row.events as Event[]
    const start = events.findIndex((event) => event.t === `start`)
    const held = events.findIndex((event) => event.t === `held`)
    return events.some((event) => event.t === `held` && event.ok === false) || (held > start + 1 && start >= 0)
  }
  const skipped = (row: Row) =>
    holdsOnly(row) || (row.events as Event[]).some((event) => event.t === `speaker` || (event.t === `ticket` && event.sealed === true))
  const rows = fixture.caller.cases.filter((row) => !skipped(row))

  it(`replays every row but the hold's, the loudspeaker's and the sealed ones`, () => {
    expect(fixture.caller.cases.filter(skipped).map((row) => row.name).length).toBe(6)
    expect(rows.length).toBe(fixture.caller.cases.length - 6)
  })

  /** What a row's effects look like from outside a browser's call. */
  function expected(row: Row) {
    const seen: string[] = []
    let releases = 0
    let flowing = false
    for (const effects of row.effects as Record<string, unknown>[][]) {
      for (const effect of effects) {
        switch (effect.do) {
          case `ring`:
            seen.push(`ring ${effect.ms}`)
            break
          case `ticket`:
            seen.push(`openAudio`)
            break
          case `dial`:
            seen.push(`openSocket`)
            break
          case `startAudio`:
            flowing = true
            break
          case `stopRing`:
            seen.push(`ring.stop`)
            break
          case `flush`:
            seen.push(`flush`)
            break
          case `mute`:
            seen.push(`setMuted ${effect.muted}`)
            break
          case `send`:
            seen.push(`send ${JSON.stringify(effect.frame)}`)
            break
          case `release`:
            releases += 1
            flowing = false
            break
          case `toast`:
            seen.push(`toast ${effect.text}`)
            break
        }
      }
      // A probe packet after every event: it goes up while the microphone's frames flow.
      if (flowing) seen.push(`up`)
    }
    return { seen, releases }
  }

  it.each(rows.map((row) => [row.name, row] as const))(`%s`, async (_name, row) => {
    vi.useFakeTimers()
    try {
      const seen: string[] = []
      // What the call was handed, as it hands it (closures fill it in).
      const fake: {
        socket: FakeSocket | null
        audio: ReturnType<typeof fakeAudio> | null
        grant: { resolve: (audio: CallAudio) => void; reject: (error: unknown) => void } | null
        answer: { resolve: (ticket: { url: string; device?: string; fallback?: boolean }) => void; reject: (error: unknown) => void } | null
      } = { socket: null, audio: null, grant: null, answer: null }
      let ringing = false
      const call: VoiceCall = new VoiceCall({
        startCall: () => new Promise((resolve, reject) => (fake.answer = { resolve, reject })),
        openAudio: () => {
          seen.push(`openAudio`)
          return new Promise((resolve, reject) => (fake.grant = { resolve, reject }))
        },
        openSocket: () => {
          seen.push(`openSocket`)
          const next = new FakeSocket()
          const send = next.send.bind(next)
          next.send = (data) => {
            if (typeof data === `string`) seen.push(`send ${data}`)
            send(data)
          }
          return (fake.socket = next)
        },
        ring: (ms) => {
          seen.push(`ring ${ms}`)
          ringing = true
          return () => {
            ringing = false
            // A stop by `release` is the release's; only an answered ring's is `stopRing`.
            if (call.active) seen.push(`ring.stop`)
          }
        },
      })
      call.onToast = (text) => seen.push(`toast ${text}`)
      const settle = async () => {
        for (let n = 0; n < 10; n++) await Promise.resolve()
      }
      let granted = false
      const microphone = () => {
        const next = fakeAudio()
        const setMuted = next.setMuted
        next.setMuted = (muted) => {
          if (muted !== next.muted) seen.push(`setMuted ${muted}`)
          setMuted(muted)
        }
        next.flush = () => seen.push(`flush`)
        return (fake.audio = next)
      }
      let releases = 0
      for (const event of row.events as Event[]) {
        const before = call.active
        switch (event.t) {
          case `start`:
            void call.start(`agent-1`, event.device as string, event.language as string | undefined, (event.fallback as boolean | undefined) ?? false)
            break
          case `held`:
            break
          case `ticket`:
          case `refused`: {
            // The microphone opens once; a late answer finds the call gone.
            if (fake.grant && !granted) {
              granted = true
              fake.grant.resolve(microphone())
            }
            await settle()
            if (event.t === `refused`) fake.answer?.reject(new Error(event.message as string))
            else
              fake.answer?.resolve({
                url: `wss://relay/ws?t=caller`,
                ...(event.device !== undefined ? { device: event.device as string } : {}),
                ...(event.fallback !== undefined ? { fallback: event.fallback as boolean } : {}),
              })
            break
          }
          case `frame`:
            fake.socket?.text(event.frame as Record<string, unknown>)
            break
          case `closed`:
            fake.socket?.drop()
            break
          case `failed`:
            // Before the ticket's microphone opened: the browser refused it;
            // once dialled: the codec failed.
            if (fake.socket) fake.audio?.onError?.()
            else if (fake.grant && !granted) {
              granted = true
              fake.grant.reject(new CallAudioError(event.code as CallAudioError[`code`]))
            }
            break
          case `ringTimeout`:
            await vi.advanceTimersByTimeAsync(fixture.constants.maxRingMs)
            break
          case `mute`:
            call.toggleMute()
            break
          case `end`:
            call.end()
            break
          default:
            throw new Error(`no stimulus for ${event.t}`)
        }
        await settle()
        if (before && !call.active) releases += 1
        const { socket, audio } = fake
        if (call.active && audio && socket) {
          const sent = socket.sent.length
          audio.onPacket?.(new Uint8Array([7]))
          if (socket.sent.length > sent) seen.push(`up`)
        }
      }
      const want = expected(row)
      expect(seen).toEqual(want.seen)
      expect(releases).toBe(want.releases)
      const state = call.getSnapshot().state
      const end = row as Row & { device?: string; fallback?: boolean; muted?: boolean }
      expect(state.phase).toBe(end.phase)
      expect(call.active).toBe(end.stage !== `idle`)
      if (end.device !== undefined) expect(state.device).toBe(end.device)
      if (end.fallback !== undefined) expect(state.fallback).toBe(end.fallback)
      if (end.muted !== undefined) expect(state.muted).toBe(end.muted)
      if (end.stage === `idle`) {
        // Everything the call held went: the ring, the socket, the microphone.
        expect(ringing).toBe(false)
        if (fake.socket) expect(fake.socket.closed).toBe(true)
        if (fake.audio) expect(fake.audio.closed).toBe(true)
      }
      call.end()
    } finally {
      vi.useRealTimers()
    }
  })
})
