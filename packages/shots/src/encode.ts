/**
 * The ONE encode pipeline every stored screenshot goes through (EXP-566).
 *
 * Captures arrive at wildly different native resolutions — a @2x browser shot is
 * 2880×1920, an iPhone 17 Pro Max is 1320×2868, an emulator 1080×2400. The store
 * normalises all of them to an 1800px LONG EDGE (see `PLATFORM_FRAME`) and one
 * lossy webp setting, for two reasons:
 *
 *   1. The store is COMMITTED. Raw PNGs at native resolution would add tens of
 *      megabytes per capture run to git history, forever.
 *   2. The diff-skip in `store.ts` compares the ENCODED images, so encoding has
 *      to be deterministic. Same bytes in, same bytes out — anything that varies
 *      per run (a timestamp, an adaptive quality heuristic) would make every
 *      view look "changed" on every run and defeat the whole mechanism.
 *
 * `withoutEnlargement` matters: a view captured below 1800px (a small window, a
 * platform that shrinks) is stored at its real size rather than upscaled into
 * fake sharpness.
 */
import sharp from "sharp"

/** Long edge of every stored image, in pixels. */
export const MAX_LONG_EDGE = 1800

/**
 * Fixed webp settings. `quality: 78` is the knee for UI screenshots — text stays
 * crisp, flat dark panels stop banding; `effort: 5` is sharp's default-ish
 * middle, and `smartSubsample` keeps coloured text edges (status pills, syntax
 * highlighting) from bleeding.
 */
export const WEBP_OPTIONS = {
  quality: 78,
  effort: 5,
  smartSubsample: true,
} as const

export interface EncodedShot {
  buf: Buffer
  width: number
  height: number
}

/** Decode a capture, downscale it to the store frame, encode webp. */
export async function encodeShot(png: Uint8Array): Promise<EncodedShot> {
  const { data, info } = await sharp(Buffer.from(png))
    .resize({
      width: MAX_LONG_EDGE,
      height: MAX_LONG_EDGE,
      fit: `inside`,
      kernel: `lanczos3`,
      withoutEnlargement: true,
    })
    .webp(WEBP_OPTIONS)
    .toBuffer({ resolveWithObject: true })
  return { buf: data, width: info.width, height: info.height }
}

export interface RawImage {
  data: Uint8Array
  width: number
  height: number
}

/**
 * Decode any encoded image to straight RGBA, which is the only thing pixelmatch
 * understands. `ensureAlpha` is not optional: webp drops the alpha channel when
 * a capture happens to be fully opaque, and comparing a 3-channel buffer against
 * a 4-channel one silently reads garbage.
 */
export async function toRawRgba(encoded: Uint8Array): Promise<RawImage> {
  const { data, info } = await sharp(Buffer.from(encoded))
    .ensureAlpha()
    .raw()
    .toBuffer({ resolveWithObject: true })
  return { data, width: info.width, height: info.height }
}

/** Pixel dimensions of an encoded image, without decoding the pixels. */
export async function imageSize(
  encoded: Uint8Array
): Promise<{ width: number; height: number }> {
  const meta = await sharp(Buffer.from(encoded)).metadata()
  return { width: meta.width ?? 0, height: meta.height ?? 0 }
}

/**
 * Mean per-channel standard deviation — the desktop capturer's "did I photograph
 * an empty window?" gate. A window caught before its first paint is a flat dark
 * rectangle: near-zero variance. Real UI, even a mostly-empty settings pane,
 * sits well above 10.
 */
export async function luminanceVariance(png: Uint8Array): Promise<number> {
  const stats = await sharp(Buffer.from(png)).stats()
  const channels = stats.channels.slice(0, 3)
  if (channels.length === 0) return 0
  return channels.reduce((sum, channel) => sum + channel.stdev, 0) / channels.length
}
