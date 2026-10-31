// EXP-854 — the inline image budget. Real sharp, real bytes: the point of the
// policy is what an agent's context actually receives, which a mocked encoder
// cannot tell us.

import { describe, expect, it } from "vitest"
import sharp from "sharp"
import { inlineImageForContext } from "./inline-image"

const MAX_BYTES = 300 * 1024
const MAX_EDGE = 1280

/** Noise, not flat colour: a flat image compresses to a few hundred bytes and
 *  would pass the byte cap for the wrong reason. */
async function noisyPng(width: number, height: number): Promise<Uint8Array> {
  const channels = 3
  const raw = Buffer.alloc(width * height * channels)
  let seed = 42
  for (let i = 0; i < raw.length; i++) {
    seed = (seed * 1103515245 + 12345) % 2147483648
    raw[i] = seed % 256
  }
  const png = await sharp(raw, { raw: { width, height, channels } })
    .png({ compressionLevel: 0 })
    .toBuffer()
  return new Uint8Array(png)
}

describe(`inlineImageForContext`, () => {
  it(`downscales a 4000x3000 screenshot to a bounded webp`, { timeout: 30_000 }, async () => {
    const original = await noisyPng(4000, 3000)

    const inlined = await inlineImageForContext(original, `image/png`)

    expect(inlined).not.toBeNull()
    expect(inlined!.mimeType).toBe(`image/webp`)
    expect(inlined!.downscaled).toBe(true)
    expect(Math.max(inlined!.width ?? 0, inlined!.height ?? 0)).toBe(MAX_EDGE)
    // 4:3 stays 4:3 — a bound, never a crop.
    expect(inlined!.width).toBe(1280)
    expect(inlined!.height).toBe(960)
    expect(inlined!.data.byteLength).toBeLessThanOrEqual(MAX_BYTES)
    expect(inlined!.data.byteLength).toBeLessThan(original.byteLength)
    // Really a WebP container (RIFF....WEBP).
    const header = Buffer.from(inlined!.data.slice(0, 12)).toString(`latin1`)
    expect(header.startsWith(`RIFF`)).toBe(true)
    expect(header.endsWith(`WEBP`)).toBe(true)
  })

  it(`passes a small image through untouched`, async () => {
    const original = await noisyPng(200, 120)
    expect(original.byteLength).toBeLessThanOrEqual(MAX_BYTES)

    const inlined = await inlineImageForContext(original, `image/png`)

    expect(inlined).not.toBeNull()
    expect(inlined!.downscaled).toBe(false)
    expect(inlined!.mimeType).toBe(`image/png`)
    expect(inlined!.width).toBe(200)
    expect(inlined!.height).toBe(120)
    expect(Buffer.from(inlined!.data).equals(Buffer.from(original))).toBe(true)
  })

  it(`re-encodes a heavy image that is already small enough to read`, { timeout: 30_000 }, async () => {
    // Under the edge cap, far over the byte cap: the bytes are the cost, so
    // this one is re-encoded too.
    const original = await noisyPng(1200, 1000)
    expect(original.byteLength).toBeGreaterThan(MAX_BYTES)

    const inlined = await inlineImageForContext(original, `image/png`)

    expect(inlined!.downscaled).toBe(true)
    expect(inlined!.mimeType).toBe(`image/webp`)
    // Never upscaled, and never enlarged to the cap either.
    expect(inlined!.width).toBe(1200)
    expect(inlined!.height).toBe(1000)
  })

  it(`inlines the first frame of an animated gif`, async () => {
    const frames = await sharp(
      Buffer.from(await noisyPng(2000, 2000)),
    )
      .gif()
      .toBuffer()

    const inlined = await inlineImageForContext(
      new Uint8Array(frames),
      `image/gif`
    )

    expect(inlined!.downscaled).toBe(true)
    expect(inlined!.width).toBe(MAX_EDGE)
    expect(inlined!.height).toBe(MAX_EDGE)
  })

  it(`returns null for bytes that are not an image`, async () => {
    const corrupt = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 1, 2, 3, 4, 5, 6])

    expect(await inlineImageForContext(corrupt, `image/png`)).toBeNull()
  })
})
