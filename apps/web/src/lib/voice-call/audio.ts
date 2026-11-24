// NEW-70 (voice V6): a web call's audio (`fixtures/voice-call.json`
// `constants`). Capture = getUserMedia with the browser's echo canceller and
// noise suppression on, mono `upRate` Hz, cut into `frameMs` frames by a
// capture worklet and coded as Opus (~`upKbps` kbps, in-band FEC) by
// WebCodecs' AudioEncoder. Playback = the `downRate` Hz Opus downlink
// decoded by AudioDecoder into a playback worklet behind a `jitterMs` jitter
// buffer, flushed on barge-in. The host sends a reply at real time, at most
// 200 ms ahead (#EXP-1929), and the buffer plays what arrives in time.
// Mute disables the track and keeps encoding: its silence keeps the frames
// flowing, so the relay's idle timeout never ends a muted call.

import { VOICE_CALL_CONSTANTS } from "@exp/etalk/voice-call"
import captureWorkletUrl from "./worklets/call-capture.js?url&no-inline"
import playbackWorkletUrl from "./worklets/call-playback.js?url&no-inline"

/** The two worklet modules. `no-inline`: a small asset would become a
 *  `data:` URL, which the CSP refuses like a blob. */
export const CALL_WORKLET_URLS = { capture: captureWorkletUrl, playback: playbackWorkletUrl }

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

// The capture worklet (worklets/call-capture.js): the context's input
// resampled (linearly, when the context could not open at `upRate`) to
// `upRate`, posted in whole frames. The playback worklet
// (worklets/call-playback.js): decoded chunks (any rate) resampled to the
// output rate and queued; it starts (and restarts after an underrun) once
// `jitterMs` are buffered; `flush` drops the queue.

// #EXP-2102: by URL, from this origin. The CSP has no `blob:` in script-src
// and must not get one.
const addWorklet = (context: AudioContext, url: string): Promise<void> => context.audioWorklet.addModule(url)

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
    await addWorklet(capture.context, CALL_WORKLET_URLS.capture)
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
    await addWorklet(playback, CALL_WORKLET_URLS.playback)
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

// S-3 (voice-call.json): the caller's ring while the voice device warms up,
// EXP-1423: the shared ringback (`ringLowHz` + `ringHighHz`, `ringToneMs`
// on, `ringGapMs` off: `callRingToneOn`), quiet under whatever the page plays.
const RING_TONES = [VOICE_CALL_CONSTANTS.ringLowHz, VOICE_CALL_CONSTANTS.ringHighHz]
const RING_ON = VOICE_CALL_CONSTANTS.ringToneMs / 1000
const RING_PERIOD = (VOICE_CALL_CONSTANTS.ringToneMs + VOICE_CALL_CONSTANTS.ringGapMs) / 1000
const RING_GAIN = 0.08
const RING_RAMP = 0.02

/** Play the ring tone, looping, at most `ms` (called inside the click that
 *  dials, so the context may start); answers a stop that silences it at once. */
export function playRingTone(ms: number): () => void {
  let context: AudioContext
  try {
    context = new AudioContext({ latencyHint: `interactive` })
  } catch {
    return () => {}
  }
  const seconds = ms / 1000
  const start = context.currentTime + 0.05
  const gain = context.createGain()
  gain.gain.setValueAtTime(0, start)
  for (let period = 0; period < seconds; period += RING_PERIOD) {
    const from = start + period
    const to = Math.min(from + RING_ON, start + seconds)
    if (to <= from) continue
    gain.gain.setValueAtTime(0, from)
    gain.gain.linearRampToValueAtTime(RING_GAIN, from + RING_RAMP)
    gain.gain.setValueAtTime(RING_GAIN, to - RING_RAMP)
    gain.gain.linearRampToValueAtTime(0, to)
  }
  gain.connect(context.destination)
  const oscillators = RING_TONES.map((frequency) => {
    const oscillator = context.createOscillator()
    oscillator.frequency.value = frequency
    oscillator.connect(gain)
    oscillator.start(start)
    oscillator.stop(start + seconds)
    return oscillator
  })
  void context.resume().catch(() => {})
  let stopped = false
  const stop = () => {
    if (stopped) return
    stopped = true
    for (const oscillator of oscillators) {
      try {
        oscillator.stop()
      } catch {}
    }
    void context.close().catch(() => {})
  }
  oscillators[0]!.onended = stop
  return stop
}
