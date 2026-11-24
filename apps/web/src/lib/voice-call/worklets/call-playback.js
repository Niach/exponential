// #EXP-2102: an AudioWorklet module of a web call, served as a same-origin
// hashed asset (audio.ts imports its URL). Never a blob: URL: the app's CSP
// (lib/security-headers.ts, `script-src 'self'`) refuses one, and a refused
// module read as "This browser can't make calls". Plain JS: it runs in the
// AudioWorkletGlobalScope and is copied to the build as it is.
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
