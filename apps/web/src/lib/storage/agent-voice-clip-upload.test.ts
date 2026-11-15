import { describe, expect, it } from "vitest"
import { isWav } from "./agent-voice-clip-upload"

// #EXP-1847: a voice clip is a WAV by its bytes; a declared type must be a
// WAV one, a missing or generic one is judged by the bytes alone.
describe(`isWav`, () => {
  const wav = new Uint8Array([...`RIFF`].map((c) => c.charCodeAt(0)).concat([0, 0, 0, 0], [...`WAVE`].map((c) => c.charCodeAt(0))))
  it(`takes RIFF/WAVE bytes under a WAV, generic or missing type`, () => {
    expect(isWav(`audio/wav`, wav)).toBe(true)
    expect(isWav(`audio/x-wav`, wav)).toBe(true)
    expect(isWav(`application/octet-stream`, wav)).toBe(true)
    expect(isWav(``, wav)).toBe(true)
  })
  it(`refuses other bytes or another declared type`, () => {
    expect(isWav(`audio/mpeg`, wav)).toBe(false)
    expect(isWav(`audio/wav`, new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]))).toBe(false)
  })
})
