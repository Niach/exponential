import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { CallAudioError, openCallAudio } from "./audio"

// NEW-70 review: the web call's audio against stub WebAudio + WebCodecs:
// the microphone's track is stopped on close (and on a failed open), mute
// keeps encoding (the disabled track's silence keeps frames flowing, so the
// relay's idle timeout never ends a muted call), a codec error ends the call.

class FakeTrack {
  enabled = true
  stopped = false
  stop() {
    this.stopped = true
  }
}

class FakeContext {
  static failWorklet = false
  audioWorklet = {
    addModule: async () => {
      if (FakeContext.failWorklet) throw new Error(`no worklet`)
    },
  }
  destination = {}
  closed = false
  constructor(_options?: unknown) {}
  createMediaStreamSource() {
    return { connect() {}, disconnect() {} }
  }
  async resume() {}
  async close() {
    this.closed = true
  }
}

const nodes: FakeNode[] = []
class FakeNode {
  port = { onmessage: null as ((event: MessageEvent) => void) | null, postMessage() {} }
  constructor(_context: unknown, readonly name: string) {
    nodes.push(this)
  }
  connect() {}
}

const encoders: FakeEncoder[] = []
class FakeEncoder {
  static isConfigSupported = async () => ({ supported: true })
  state = `unconfigured`
  encoded = 0
  constructor(readonly init: { output: (chunk: unknown) => void; error: (error: unknown) => void }) {
    encoders.push(this)
  }
  configure() {
    this.state = `configured`
  }
  encode() {
    this.encoded += 1
    this.init.output({ byteLength: 1, copyTo: (into: Uint8Array) => into.set([7]) })
  }
  close() {
    this.state = `closed`
  }
}

const decoders: FakeEncoder[] = []
class FakeDecoder extends FakeEncoder {
  constructor(init: FakeEncoder[`init`]) {
    super(init)
    encoders.pop()
    decoders.push(this)
  }
}

let track: FakeTrack

beforeEach(() => {
  track = new FakeTrack()
  nodes.length = 0
  encoders.length = 0
  decoders.length = 0
  FakeContext.failWorklet = false
  const stream = { getTracks: () => [track], getAudioTracks: () => [track] }
  vi.stubGlobal(`navigator`, { mediaDevices: { getUserMedia: async () => stream } })
  vi.stubGlobal(`AudioContext`, FakeContext)
  vi.stubGlobal(`AudioWorkletNode`, FakeNode)
  vi.stubGlobal(`AudioEncoder`, FakeEncoder)
  vi.stubGlobal(`AudioDecoder`, FakeDecoder)
  vi.stubGlobal(`AudioData`, class {
    close() {}
  })
  vi.stubGlobal(`EncodedAudioChunk`, class {})
  vi.spyOn(URL, `createObjectURL`).mockReturnValue(`blob:worklet`)
  vi.spyOn(URL, `revokeObjectURL`).mockImplementation(() => {})
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

/** One captured frame of the microphone, as the capture worklet posts it. */
function capture(samples = new Float32Array(320).fill(0.1)) {
  const tap = nodes.find((node) => node.name === `call-capture`)!
  tap.port.onmessage?.({ data: samples } as MessageEvent)
}

describe(`openCallAudio`, () => {
  it(`close stops the microphone's track`, async () => {
    const audio = await openCallAudio()
    expect(track.stopped).toBe(false)
    audio.close()
    expect(track.stopped).toBe(true)
    expect(encoders[0]!.state).toBe(`closed`)
  })

  it(`a failed open stops the track and says unsupported`, async () => {
    FakeContext.failWorklet = true
    await expect(openCallAudio()).rejects.toEqual(new CallAudioError(`unsupported`))
    expect(track.stopped).toBe(true)
  })

  it(`muted, the track is disabled and the frames keep flowing`, async () => {
    const audio = await openCallAudio()
    const packets: number[][] = []
    audio.onPacket = (packet) => packets.push([...packet])
    audio.setMuted(true)
    expect(track.enabled).toBe(false)
    capture(new Float32Array(320))
    expect(encoders[0]!.encoded).toBe(1)
    expect(packets).toEqual([[7]])
    audio.setMuted(false)
    expect(track.enabled).toBe(true)
  })

  it(`an encoder or decoder error ends the call`, async () => {
    const audio = await openCallAudio()
    const onError = vi.fn()
    audio.onError = onError
    encoders[0]!.init.error(new Error(`encode`))
    decoders[0]!.init.error(new Error(`decode`))
    expect(onError).toHaveBeenCalledTimes(2)
  })
})
