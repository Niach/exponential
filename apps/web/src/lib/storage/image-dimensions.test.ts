import { describe, expect, it } from "vitest"
import { getImageDimensions } from "@/lib/storage/image-dimensions"

// Minimal PNG header: 8-byte signature + IHDR length/type + width/height.
function pngHeader(width: number, height: number): Uint8Array {
  const bytes = new Uint8Array(24)
  bytes.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a], 0)
  const view = new DataView(bytes.buffer)
  view.setUint32(8, 13) // IHDR length
  bytes.set([0x49, 0x48, 0x44, 0x52], 12) // "IHDR"
  view.setUint32(16, width)
  view.setUint32(20, height)
  return bytes
}

describe(`getImageDimensions`, () => {
  it(`probes a plausible PNG header`, () => {
    expect(getImageDimensions(pngHeader(1280, 720))).toEqual({
      width: 1280,
      height: 720,
    })
  })

  it(`returns null for unrecognized bytes`, () => {
    expect(getImageDimensions(new Uint8Array([1, 2, 3, 4]))).toBeNull()
  })

  // REV-26: a crafted IHDR declaring 0xFFFFFFFF used to flow into the int4
  // attachments.width column and abort the widget submit transaction with a
  // Postgres out-of-range error. Absurd headers degrade to null instead.
  it(`rejects header dimensions that overflow any real image`, () => {
    expect(getImageDimensions(pngHeader(0xffffffff, 720))).toBeNull()
    expect(getImageDimensions(pngHeader(1280, 0xffffffff))).toBeNull()
    expect(getImageDimensions(pngHeader(65536, 65536))).toBeNull()
  })

  it(`rejects zero dimensions`, () => {
    expect(getImageDimensions(pngHeader(0, 720))).toBeNull()
    expect(getImageDimensions(pngHeader(1280, 0))).toBeNull()
  })

  it(`accepts the 65535 boundary`, () => {
    expect(getImageDimensions(pngHeader(65535, 1))).toEqual({
      width: 65535,
      height: 1,
    })
  })
})
