// #EXP-2102: an AudioWorklet module of a web call, served as a same-origin
// hashed asset (audio.ts imports its URL). Never a blob: URL: the app's CSP
// (lib/security-headers.ts, `script-src 'self'`) refuses one, and a refused
// module read as "This browser can't make calls". Plain JS: it runs in the
// AudioWorkletGlobalScope and is copied to the build as it is.
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
