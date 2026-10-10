// VAPP-103 rfix: the pixel cap on a whole image (the Rust core's
// `media_image_within_limits` tests, byte for byte).
import { describe, expect, test } from "bun:test"
import { imageDimensions, imageFrames, mediaImageWithinLimits, svgDimensions } from "./policy"

const enc = (s: string) => new TextEncoder().encode(s)
const bytes = (...parts: (number[] | Uint8Array)[]) => {
  const out: number[] = []
  for (const p of parts) out.push(...p)
  return new Uint8Array(out)
}
const le16 = (v: number) => [v & 0xff, (v >> 8) & 0xff]
const be32 = (v: number) => [(v >>> 24) & 0xff, (v >>> 16) & 0xff, (v >>> 8) & 0xff, v & 0xff]
const le32 = (v: number) => [v & 0xff, (v >>> 8) & 0xff, (v >>> 16) & 0xff, (v >>> 24) & 0xff]

describe(`media pixel cap`, () => {
  test(`SVGs have a size`, () => {
    expect(svgDimensions(enc(`<svg xmlns="http://www.w3.org/2000/svg" width="20000" height="20000"/>`))).toEqual([20000, 20000])
    expect(mediaImageWithinLimits(enc(`<svg width="20000" height="20000"></svg>`))).toContain(`20000×20000`)
    expect(svgDimensions(enc(`<?xml version="1.0"?><svg viewBox="0 0 300 150"/>`))).toEqual([300, 150])
    expect(svgDimensions(enc(`<svg width="100000" viewBox="0 0 1 1">`))).toEqual([100000, 100000])
    expect(svgDimensions(enc(`<svg width="2in" height="10em">`))).toEqual([192, 160])
    expect(svgDimensions(enc(`<svg width="50%">`))).toEqual([100, 100])
    expect(svgDimensions(enc(`<svg>`))).toEqual([100, 100])
    expect(imageDimensions(enc(`<svg/>`))).toEqual([100, 100])
    expect(mediaImageWithinLimits(enc(`<svg width="24" height="24"/>`))).toBeNull()
    expect(mediaImageWithinLimits(enc(`<?xml version='1.0'?><note/>`))).toContain(`unreadable`)
  })

  test(`frames multiply the pixels`, () => {
    const frame = [0x21, 0xf9, 4, 0, 0, 0, 0, 0, 0x2c, 0, 0, 0, 0, 0xe8, 3, 0xe8, 3, 0, 2, 2, 0x4c, 1, 0]
    const gif = bytes(enc(`GIF89a`), le16(1000), le16(1000), [0, 0, 0], ...Array.from({ length: 40 }, () => frame), [0x3b])
    expect(imageFrames(gif)).toBe(40)
    expect(mediaImageWithinLimits(gif)).toContain(`40 frames`)
    expect(imageFrames(gif.subarray(0, gif.length / 2))).toBe(20)
    const png = bytes([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13], enc(`IHDR`), be32(1000), be32(1000), [8, 6, 0, 0, 0, 0, 0, 0, 0], [0, 0, 0, 8], enc(`acTL`), be32(100), [0, 0, 0, 0, 0, 0, 0, 0])
    expect(imageFrames(png)).toBe(100)
    expect(mediaImageWithinLimits(png)).not.toBeNull()
    const webp = bytes(enc(`RIFF`), [0, 0, 0, 0], enc(`WEBPVP8X`), [0x0a, 0, 0, 0, 0x02, 0, 0, 0], le32(1999).slice(0, 3), le32(1999).slice(0, 3), ...Array.from({ length: 10 }, () => bytes(enc(`ANMF`), [2, 0, 0, 0], enc(`ab`))))
    expect(imageDimensions(webp)).toEqual([2000, 2000])
    expect(imageFrames(webp)).toBe(10)
    expect(mediaImageWithinLimits(webp)).toContain(`10 frames`)
  })

  test(`an unreadable raster is refused`, () => {
    expect(mediaImageWithinLimits(bytes([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0]))).toContain(`unreadable`)
    expect(mediaImageWithinLimits(bytes([0xff, 0xd8, 0xff, 0xe0]))).toContain(`unreadable`)
    expect(mediaImageWithinLimits(enc(`{"not": "an image"}`))).toBeNull()
    expect(mediaImageWithinLimits(bytes(enc(`GIF89a`), [0x10, 0, 0x20, 0]))).toBeNull()
  })
})
