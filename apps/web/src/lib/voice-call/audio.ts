// NEW-70 (voice V6): a web call's audio (`fixtures/voice-call.json`
// `constants`). Capture = getUserMedia with the browser's echo canceller and
// noise suppression on, mono `upRate` Hz, cut into `frameMs` frames by a
// capture worklet and coded as Opus (~`upKbps` kbps, in-band FEC) by
// WebCodecs' AudioEncoder. Playback = the `downRate` Hz Opus downlink
// decoded by AudioDecoder into a playback worklet behind a `jitterMs` jitter
// buffer, flushed on barge-in. The host sends a reply as fast as it is
// synthesised, so the buffer queues a whole clause and plays it in time.
// Mute disables the track and keeps encoding: its silence keeps the frames
// flowing, so the relay's idle timeout never ends a muted call.

import { VOICE_CALL_CONSTANTS } from "@exp/domain-contract/voice-call"

const { upRate, downRate, upKbps, frameMs, jitterMs } = VOICE_CALL_CONSTANTS
const UP_FRAME = (upRate * frameMs) / 1000

/** Why a call could not open its audio (voice-call.json `copy.failed`). */
export class CallAudioError extends Error {
  constructor(readonly code: `no_microphone` | `not_granted` | `unsupported`) {
    super(code)
  }
}

export interface CallAudio {
  /** One Opus packet of the microphone per `frameMs`; `level` = its 0..1 loudness. */
  onPacket: ((packet: Uint8Array) => void) | null
  onLevel: ((level: number) => void) | null
  /** The encoder or decoder failed: the call is over. */
  onError: (() => void) | null
  setMuted(muted: boolean): void
  /** One downlink Opus packet, in order. */
  play(packet: Uint8Array): void
  /** Barge-in: drop whatever is still queued to play. */
  flush(): void
  close(): void
}

const ENCODER_CONFIG = {
  codec: `opus`,
  sampleRate: upRate,
  numberOfChannels: 1,
  bitrate: upKbps * 1000,
  opus: { frameDuration: frameMs * 1000, useinbandfec: true, packetlossperc: 10 },
} as AudioEncoderConfig
const DECODER_CONFIG: AudioDecoderConfig = { codec: `opus`, sampleRate: downRate, numberOfChannels: 1 }

/** Whether this browser codes Opus through WebCodecs both ways. */
export async function opusCallSupported(): Promise<boolean> {
  if (typeof AudioEncoder === `undefined` || typeof AudioDecoder === `undefined`) return false
  try {
    const [up, down] = await Promise.all([
      AudioEncoder.isConfigSupported(ENCODER_CONFIG),
      AudioDecoder.isConfigSupported(DECODER_CONFIG),
    ])
    return up.supported === true && down.supported === true
  } catch {
    return false
  }
}

// The capture worklet: the context's input resampled (linearly, when the
// context could not open at `upRate`) to `upRate`, posted in whole frames.
const CAPTURE_WORKLET = `
class CallCapture extends AudioWorkletProcessor {
  constructor(options) {
    super()
    this.target = options.processorOptions.rate
    this.size = options.processorOptions.frame
    this.step = sampleRate / this.target
    this.pos = 0
    this.last = 0
    this.frame = new Float32Array(this.size)
    this.fill = 0
  }
  emit(sample) {
    this.frame[this.fill++] = sample
    if (this.fill === this.size) {
      this.port.postMessage(this.frame, [this.frame.buffer])
      this.frame = new Float32Array(this.size)
      this.fill = 0
    }
  }
  process(inputs) {
    const input = inputs[0] && inputs[0][0]
    if (!input) return true
    if (this.step === 1) {
      for (let i = 0; i < input.length; i++) this.emit(input[i])
      return true
    }
    while (this.pos < input.length) {
      const i = Math.floor(this.pos)
      const frac = this.pos - i
      const a = i === 0 ? this.last : input[i - 1]
      const b = input[i]
      this.emit(a + (b - a) * frac)
      this.pos += this.step
    }
    this.pos -= input.length
    this.last = input[input.length - 1]
    return true
  }
}
registerProcessor('call-capture', CallCapture)
`

// The playback worklet: decoded chunks (any rate) resampled to the output
// rate and queued; it starts (and restarts after an underrun) once
// `jitterMs` are buffered; `flush` drops the queue.
const PLAYBACK_WORKLET = `
class CallPlayback extends AudioWorkletProcessor {
  constructor(options) {
    super()
    this.prime = Math.round(sampleRate * options.processorOptions.jitterMs / 1000)
    this.queue = []
    this.head = 0
    this.buffered = 0
    this.playing = false
    this.port.onmessage = (event) => {
      const data = event.data
      if (data.t === 'flush') {
        this.queue = []
        this.head = 0
        this.buffered = 0
        this.playing = false
        return
      }
      const chunk = this.resample(data.samples, data.rate)
      this.queue.push(chunk)
      this.buffered += chunk.length
    }
  }
  resample(samples, rate) {
    if (rate === sampleRate) return samples
    const length = Math.floor(samples.length * sampleRate / rate)
    const out = new Float32Array(length)
    const step = rate / sampleRate
    for (let j = 0; j < length; j++) {
      const pos = j * step
      const i = Math.floor(pos)
      const a = samples[i]
      const b = i + 1 < samples.length ? samples[i + 1] : a
      out[j] = a + (b - a) * (pos - i)
    }
    return out
  }
  process(_inputs, outputs) {
    const out = outputs[0][0]
    if (!this.playing && this.buffered >= this.prime) this.playing = true
    if (!this.playing) return true
    let written = 0
    while (written < out.length && this.queue.length > 0) {
      const chunk = this.queue[0]
      const take = Math.min(out.length - written, chunk.length - this.head)
      out.set(chunk.subarray(this.head, this.head + take), written)
      written += take
      this.head += take
      this.buffered -= take
      if (this.head === chunk.length) {
        this.queue.shift()
        this.head = 0
      }
    }
    if (written < out.length) this.playing = false
    return true
  }
}
registerProcessor('call-playback', CallPlayback)
`

async function addWorklet(context: AudioContext, source: string): Promise<void> {
  const url = URL.createObjectURL(new Blob([source], { type: `text/javascript` }))
  try {
    await context.audioWorklet.addModule(url)
  } finally {
    URL.revokeObjectURL(url)
  }
}

async function openMicrophone(): Promise<MediaStream> {
  if (!navigator.mediaDevices?.getUserMedia) throw new CallAudioError(`no_microphone`)
  try {
    return await navigator.mediaDevices.getUserMedia({
      audio: {
        echoCancellation: true,
        noiseSuppression: true,
        autoGainControl: true,
        channelCount: 1,
        sampleRate: upRate,
      },
    })
  } catch (error) {
    const name = error instanceof DOMException ? error.name : ``
    if (name === `NotAllowedError` || name === `SecurityError`) throw new CallAudioError(`not_granted`)
    throw new CallAudioError(`no_microphone`)
  }
}

/** A capture context at `upRate` where the browser resamples the microphone
 *  itself, else the default one (the worklet resamples). */
async function captureContext(stream: MediaStream): Promise<{ context: AudioContext; source: MediaStreamAudioSourceNode }> {
  try {
    const context = new AudioContext({ sampleRate: upRate, latencyHint: `interactive` })
    try {
      return { context, source: context.createMediaStreamSource(stream) }
    } catch (error) {
      void context.close()
      throw error
    }
  } catch {
    const context = new AudioContext({ latencyHint: `interactive` })
    return { context, source: context.createMediaStreamSource(stream) }
  }
}

/** Open the microphone and the speaker for one call. */
export async function openCallAudio(): Promise<CallAudio> {
  if (!(await opusCallSupported())) throw new CallAudioError(`unsupported`)
  const stream = await openMicrophone()
  const closers: (() => void)[] = [() => stream.getTracks().forEach((track) => track.stop())]
  const closeAll = () => {
    for (const close of closers.splice(0).reverse()) {
      try {
        close()
      } catch {}
    }
  }
  try {
    let timestamp = 0
    const audio: CallAudio = {
      onPacket: null,
      onLevel: null,
      onError: null,
      setMuted(next) {
        for (const track of stream.getAudioTracks()) track.enabled = !next
        if (next) audio.onLevel?.(0)
      },
      play: () => {},
      flush: () => {},
      close: closeAll,
    }

    // Uplink: microphone → capture worklet → AudioEncoder → onPacket.
    const capture = await captureContext(stream)
    closers.push(() => void capture.context.close())
    await addWorklet(capture.context, CAPTURE_WORKLET)
    const encoder = new AudioEncoder({
      output: (chunk) => {
        const packet = new Uint8Array(chunk.byteLength)
        chunk.copyTo(packet)
        audio.onPacket?.(packet)
      },
      error: () => audio.onError?.(),
    })
    encoder.configure(ENCODER_CONFIG)
    closers.push(() => encoder.state !== `closed` && encoder.close())
    const tap = new AudioWorkletNode(capture.context, `call-capture`, {
      numberOfInputs: 1,
      numberOfOutputs: 0,
      processorOptions: { rate: upRate, frame: UP_FRAME },
    })
    tap.port.onmessage = (event: MessageEvent<Float32Array<ArrayBuffer>>) => {
      const samples = event.data
      timestamp += frameMs * 1000
      if (encoder.state !== `configured`) return
      let sum = 0
      for (const sample of samples) sum += sample * sample
      audio.onLevel?.(Math.min(1, Math.sqrt(sum / samples.length) * 4))
      const data = new AudioData({
        format: `f32`,
        sampleRate: upRate,
        numberOfFrames: samples.length,
        numberOfChannels: 1,
        timestamp,
        data: samples,
      })
      encoder.encode(data)
      data.close()
    }
    capture.source.connect(tap)
    closers.push(() => capture.source.disconnect())

    // Downlink: AudioDecoder → playback worklet → speaker.
    const playback = new AudioContext({ sampleRate: downRate, latencyHint: `interactive` })
    closers.push(() => void playback.close())
    await addWorklet(playback, PLAYBACK_WORKLET)
    const speaker = new AudioWorkletNode(playback, `call-playback`, {
      numberOfInputs: 0,
      outputChannelCount: [1],
      processorOptions: { jitterMs },
    })
    speaker.connect(playback.destination)
    const decoder = new AudioDecoder({
      output: (data) => {
        const samples = new Float32Array(data.numberOfFrames)
        data.copyTo(samples, { planeIndex: 0, format: `f32-planar` })
        speaker.port.postMessage({ samples, rate: data.sampleRate }, [samples.buffer])
        data.close()
      },
      error: () => audio.onError?.(),
    })
    decoder.configure(DECODER_CONFIG)
    closers.push(() => decoder.state !== `closed` && decoder.close())
    let downTime = 0
    audio.play = (packet) => {
      if (decoder.state !== `configured`) return
      decoder.decode(new EncodedAudioChunk({ type: `key`, timestamp: downTime, data: packet }))
      downTime += frameMs * 1000
    }
    audio.flush = () => speaker.port.postMessage({ t: `flush` })
    // A click started the call: both contexts may run.
    await Promise.all([capture.context.resume(), playback.resume()])
    return audio
  } catch (error) {
    closeAll()
    if (error instanceof CallAudioError) throw error
    throw new CallAudioError(`unsupported`)
  }
}
