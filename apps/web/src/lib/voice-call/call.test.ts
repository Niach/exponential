import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/voice-call.json"
import { callView, type CallEvent } from "@exp/domain-contract/voice-call"

import { CallAudioError, type CallAudio } from "./audio"
import { VoiceCall, type CallSocket } from "./call"
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

async function dialed(options: { startCall?: () => Promise<{ url: string }>; openAudio?: () => Promise<CallAudio> } = {}) {
  const socket = new FakeSocket()
  const audio = fakeAudio()
  const toasts: string[] = []
  const startCall = vi.fn(options.startCall ?? (async () => ({ url: `wss://relay/ws?t=caller` })))
  const call = new VoiceCall({
    startCall,
    openSocket: () => socket,
    openAudio: options.openAudio ?? (async () => audio),
  })
  call.onToast = (message) => toasts.push(message)
  await call.start(`agent-1`, `mint`)
  return { call, socket, audio, toasts, startCall }
}

const view = (call: VoiceCall) => callView(call.getSnapshot().state)

/** One fixture event, as it reaches a web caller. */
function deliver(call: VoiceCall, socket: FakeSocket, event: CallEvent) {
  switch (event.t) {
    case `start`:
      return
    case `connected`:
      return socket.text({ t: `call_ready` })
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

describe(`a web call replays voice-call.json flowCases off the wire`, () => {
  it.each(fixture.flowCases.map((c) => [c.name, c] as const))(`%s`, async (_name, c) => {
    const { call, socket, toasts } = await dialed()
    for (const event of c.events as CallEvent[]) deliver(call, socket, event)
    expect(view(call)).toEqual(c.view)
    expect(toasts).toEqual(`toast` in c ? [c.toast] : [])
  })
})

describe(`the web caller's wire`, () => {
  it(`dials the url voice.startCall answers, as web`, async () => {
    const { startCall, socket } = await dialed()
    expect(startCall).toHaveBeenCalledWith(`agent-1`)
    expect(socket.binaryType).toBe(`arraybuffer`)
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

  it(`ignores a host event outside the flow`, async () => {
    const { call, socket } = await dialed()
    socket.text({ t: `call_ready` })
    socket.text({ t: `call_event`, event: { t: `end` } })
    socket.text({ t: `call_event`, event: { t: `said` } })
    expect(view(call)?.state).toBe(fixture.copy.listening)
  })

  it(`mute closes the microphone and tells the host; muted before ready is told on ready`, async () => {
    const { call, socket, audio } = await dialed()
    call.toggleMute()
    expect(audio.muted).toBe(true)
    expect(socket.controls()).toEqual([])
    socket.text({ t: `call_ready` })
    expect(socket.controls()).toEqual([{ t: `call_mute`, muted: true }])
    call.toggleMute()
    expect(socket.controls().at(-1)).toEqual({ t: `call_mute`, muted: false })
    expect(audio.muted).toBe(false)
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

  it(`a socket that simply goes away closes the audio and toasts dropped`, async () => {
    const { call, socket, audio, toasts } = await dialed()
    socket.text({ t: `call_ready` })
    socket.drop()
    expect(call.active).toBe(false)
    expect(audio.closed).toBe(true)
    expect(toasts).toEqual([fixture.copy.failed.dropped])
  })

  it(`a codec error ends it as dropped and closes the microphone`, async () => {
    const { call, socket, audio, toasts } = await dialed()
    socket.text({ t: `call_ready` })
    audio.onError?.()
    expect(call.active).toBe(false)
    expect(audio.closed).toBe(true)
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
    expect(view(call)?.state).toBe(fixture.copy.connecting)
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
