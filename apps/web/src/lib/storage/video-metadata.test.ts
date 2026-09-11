import { describe, expect, it } from "vitest"
import {
  formatDuration,
  getVideoMetadata,
  isProbeableVideoContentType,
} from "@/lib/storage/video-metadata"

// ISO-BMFF builders: the fixtures are assembled box by box so each case pins
// exactly one header property (moov placement, rotation, version, codec).

function ascii(text: string) {
  return new Uint8Array([...text].map((char) => char.charCodeAt(0)))
}

function u32(value: number) {
  const bytes = new Uint8Array(4)
  new DataView(bytes.buffer).setUint32(0, value >>> 0)
  return bytes
}

function i32(value: number) {
  const bytes = new Uint8Array(4)
  new DataView(bytes.buffer).setInt32(0, value)
  return bytes
}

function u64(value: bigint) {
  const bytes = new Uint8Array(8)
  new DataView(bytes.buffer).setBigUint64(0, value)
  return bytes
}

function concat(...parts: Uint8Array[]) {
  const length = parts.reduce((sum, part) => sum + part.length, 0)
  const out = new Uint8Array(length)
  let offset = 0
  for (const part of parts) {
    out.set(part, offset)
    offset += part.length
  }
  return out
}

function box(type: string, ...payload: Uint8Array[]) {
  const body = concat(...payload)
  return concat(u32(8 + body.length), ascii(type), body)
}

function fullBox(type: string, version: number, ...payload: Uint8Array[]) {
  return box(type, new Uint8Array([version, 0, 0, 0]), ...payload)
}

const FIXED_ONE = 0x00010000

function mvhd(timescale: number, duration: number, version = 0) {
  if (version === 1) {
    return fullBox(
      `mvhd`,
      1,
      u64(0n),
      u64(0n),
      u32(timescale),
      u64(BigInt(duration)),
      new Uint8Array(80)
    )
  }
  return fullBox(
    `mvhd`,
    0,
    u32(0),
    u32(0),
    u32(timescale),
    u32(duration),
    new Uint8Array(80)
  )
}

type Rotation = 0 | 90 | 180 | 270

function matrix(rotation: Rotation) {
  const [a, b, c, d] = {
    0: [FIXED_ONE, 0, 0, FIXED_ONE],
    90: [0, FIXED_ONE, -FIXED_ONE, 0],
    180: [-FIXED_ONE, 0, 0, -FIXED_ONE],
    270: [0, -FIXED_ONE, FIXED_ONE, 0],
  }[rotation]
  return concat(
    i32(a),
    i32(b),
    i32(0),
    i32(c),
    i32(d),
    i32(0),
    i32(0),
    i32(0),
    i32(0x40000000)
  )
}

function tkhd(
  width: number,
  height: number,
  options: { rotation?: Rotation; version?: number } = {}
) {
  const head =
    options.version === 1
      ? concat(u64(0n), u64(0n), u32(1), u32(0), u64(0n))
      : concat(u32(0), u32(0), u32(1), u32(0), u32(0))
  return fullBox(
    `tkhd`,
    options.version ?? 0,
    head,
    new Uint8Array(8), // reserved
    new Uint8Array(2), // layer
    new Uint8Array(2), // alternate_group
    new Uint8Array(2), // volume
    new Uint8Array(2), // reserved
    matrix(options.rotation ?? 0),
    u32(width << 16),
    u32(height << 16)
  )
}

function hdlr(handler: string) {
  return fullBox(`hdlr`, 0, u32(0), ascii(handler), new Uint8Array(12), ascii(`\0`))
}

function stsd(sampleEntryType: string) {
  return fullBox(`stsd`, 0, u32(1), box(sampleEntryType, new Uint8Array(16)))
}

function trak(args: {
  handler?: string
  codec: string
  width?: number
  height?: number
  rotation?: Rotation
  tkhdVersion?: number
}) {
  const mdiaChildren = [
    ...(args.handler ? [hdlr(args.handler)] : []),
    box(`minf`, box(`stbl`, stsd(args.codec))),
  ]
  return box(
    `trak`,
    tkhd(args.width ?? 0, args.height ?? 0, {
      rotation: args.rotation,
      version: args.tkhdVersion,
    }),
    box(`mdia`, ...mdiaChildren)
  )
}

function ftyp(brand: string) {
  return box(`ftyp`, ascii(brand), u32(0), ascii(brand))
}

function mdat(bytes = 64) {
  return box(`mdat`, new Uint8Array(bytes))
}

const h264Tracks = [
  trak({ handler: `vide`, codec: `avc1`, width: 1920, height: 1080 }),
  trak({ handler: `soun`, codec: `mp4a` }),
]

describe(`getVideoMetadata`, () => {
  it(`probes an H.264/AAC faststart mp4`, () => {
    const file = concat(
      ftyp(`isom`),
      box(`moov`, mvhd(1000, 7250), ...h264Tracks),
      mdat()
    )
    expect(getVideoMetadata(file)).toEqual({
      width: 1920,
      height: 1080,
      durationMs: 7250,
      videoCodec: `avc1`,
      audioCodec: `mp4a`,
    })
  })

  // A camera recording writes mdat first and appends moov when recording
  // stops — the probe must walk past the payload to find it.
  it(`finds a moov box at the end of an HEVC QuickTime recording`, () => {
    const file = concat(
      ftyp(`qt  `),
      box(`wide`),
      mdat(4096),
      box(
        `moov`,
        mvhd(600, 1800),
        trak({ handler: `vide`, codec: `hvc1`, width: 3840, height: 2160 }),
        trak({ handler: `soun`, codec: `mp4a` })
      )
    )
    expect(getVideoMetadata(file)).toEqual({
      width: 3840,
      height: 2160,
      durationMs: 3000,
      videoCodec: `hvc1`,
      audioCodec: `mp4a`,
    })
  })

  it(`applies the rotation matrix so a portrait phone clip reports portrait`, () => {
    for (const rotation of [90, 270] as const) {
      const file = concat(
        ftyp(`mp42`),
        box(
          `moov`,
          mvhd(1000, 5000),
          trak({
            handler: `vide`,
            codec: `avc1`,
            width: 1920,
            height: 1080,
            rotation,
          })
        )
      )
      expect(getVideoMetadata(file)).toMatchObject({
        width: 1080,
        height: 1920,
      })
    }
    const upsideDown = concat(
      ftyp(`mp42`),
      box(
        `moov`,
        mvhd(1000, 5000),
        trak({
          handler: `vide`,
          codec: `avc1`,
          width: 1920,
          height: 1080,
          rotation: 180,
        })
      )
    )
    expect(getVideoMetadata(upsideDown)).toMatchObject({
      width: 1920,
      height: 1080,
    })
  })

  it(`reads version-1 (64-bit) movie and track headers`, () => {
    const file = concat(
      ftyp(`isom`),
      box(
        `moov`,
        mvhd(90000, 90000 * 61, 1),
        trak({
          handler: `vide`,
          codec: `av01`,
          width: 1280,
          height: 720,
          tkhdVersion: 1,
        })
      )
    )
    expect(getVideoMetadata(file)).toEqual({
      width: 1280,
      height: 720,
      durationMs: 61000,
      videoCodec: `av01`,
      audioCodec: null,
    })
  })

  it(`treats an hdlr-less track with a size as the video track`, () => {
    const file = concat(
      ftyp(`qt  `),
      box(`moov`, mvhd(1000, 1000), trak({ codec: `avc1`, width: 640, height: 480 }))
    )
    expect(getVideoMetadata(file)).toMatchObject({
      width: 640,
      height: 480,
      videoCodec: `avc1`,
    })
  })

  it(`returns null for bytes that are not a container`, () => {
    expect(getVideoMetadata(new Uint8Array([1, 2, 3, 4]))).toBeNull()
    // PNG signature.
    const png = new Uint8Array(64)
    png.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a], 0)
    expect(getVideoMetadata(png)).toBeNull()
    // Matroska/WebM EBML header.
    const webm = new Uint8Array(64)
    webm.set([0x1a, 0x45, 0xdf, 0xa3], 0)
    expect(getVideoMetadata(webm)).toBeNull()
  })

  it(`returns null when the file is truncated before moov`, () => {
    const complete = concat(
      ftyp(`isom`),
      mdat(256),
      box(`moov`, mvhd(1000, 1000), ...h264Tracks)
    )
    // Cut inside mdat: the mdat header claims more bytes than remain, so the
    // walk stops and moov is never reached.
    expect(getVideoMetadata(complete.slice(0, 100))).toBeNull()
  })

  it(`degrades a truncated moov to the fields that survive`, () => {
    const complete = concat(
      ftyp(`isom`),
      box(`moov`, mvhd(1000, 4200), ...h264Tracks)
    )
    // moov's own size header now overruns the buffer, so the walker refuses
    // it entirely rather than reading garbage.
    expect(getVideoMetadata(complete.slice(0, complete.length - 10))).toBeNull()
  })

  it(`bounds attacker-controlled header values`, () => {
    const absurdSize = concat(
      ftyp(`isom`),
      box(
        `moov`,
        mvhd(1000, 1000),
        trak({ handler: `vide`, codec: `avc1`, width: 0xffff + 1, height: 720 })
      )
    )
    expect(getVideoMetadata(absurdSize)).toMatchObject({
      width: null,
      height: null,
      durationMs: 1000,
    })

    const unknownDuration = concat(
      ftyp(`isom`),
      box(`moov`, mvhd(1000, 0xffffffff), ...h264Tracks)
    )
    expect(getVideoMetadata(unknownDuration)).toMatchObject({ durationMs: null })

    const weekLong = concat(
      ftyp(`isom`),
      box(`moov`, mvhd(1, 7 * 24 * 3600), ...h264Tracks)
    )
    expect(getVideoMetadata(weekLong)).toMatchObject({ durationMs: null })

    const zeroTimescale = concat(
      ftyp(`isom`),
      box(`moov`, mvhd(0, 1000), ...h264Tracks)
    )
    expect(getVideoMetadata(zeroTimescale)).toMatchObject({ durationMs: null })
  })

  it(`accepts a 64-bit largesize mdat before moov`, () => {
    const payload = new Uint8Array(32)
    const largeMdat = concat(u32(1), ascii(`mdat`), u64(BigInt(16 + payload.length)), payload)
    const file = concat(ftyp(`isom`), largeMdat, box(`moov`, mvhd(1000, 500), ...h264Tracks))
    expect(getVideoMetadata(file)).toMatchObject({ durationMs: 500, width: 1920 })
  })
})

describe(`isProbeableVideoContentType`, () => {
  it(`covers the BMFF family only`, () => {
    expect(isProbeableVideoContentType(`video/mp4`)).toBe(true)
    expect(isProbeableVideoContentType(`video/quicktime`)).toBe(true)
    expect(isProbeableVideoContentType(`audio/mp4`)).toBe(true)
    expect(isProbeableVideoContentType(`video/webm`)).toBe(false)
    expect(isProbeableVideoContentType(`audio/mpeg`)).toBe(false)
  })
})

describe(`formatDuration`, () => {
  it(`formats m:ss and h:mm:ss`, () => {
    expect(formatDuration(0)).toBe(`0:00`)
    expect(formatDuration(7250)).toBe(`0:07`)
    expect(formatDuration(154_000)).toBe(`2:34`)
    expect(formatDuration(3_723_000)).toBe(`1:02:03`)
  })
})
