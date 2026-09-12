// EXP-854 — what an image attachment costs an agent's context.
//
// `exponential_attachments_get` inlines images as base64 `image` content
// (EXP-511: an agent that only gets a download URL never looks at the
// picture). A phone screenshot or a retina capture is 4-12 MP, and base64 of
// its original bytes is tens of thousands of tokens in a window the run also
// needs for its work — one such fetch has emptied a context before the agent
// read the issue. So the inline copy is BOUNDED here: full resolution is what
// the signed `downloadUrl` is for, the inline copy only has to be readable.
//
// Pure and side-effect free apart from the lazy `sharp` import, so the whole
// policy is unit-testable without an object store.

/** Anything at or under this stays exactly as it is (EXP-704's text twin). */
const MAX_INLINE_IMAGE_BYTES = 300 * 1024
/** Text in a UI screenshot is still legible at this longest edge. */
const MAX_INLINE_IMAGE_EDGE = 1280
/** Tried in order until the encode fits the byte cap; the last one wins. */
const WEBP_QUALITY_LADDER = [80, 65, 50] as const

export interface InlineImage {
  /** The bytes to base64 into the MCP `image` content block. */
  data: Uint8Array
  /** `image/webp` when re-encoded, else the attachment's own content type. */
  mimeType: string
  width?: number
  height?: number
  /** False when the original passed through untouched. */
  downscaled: boolean
}

// eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
type SharpFactory = typeof import("sharp")

let sharpLoad: Promise<SharpFactory | null> | undefined
let loggedLoadFailure = false

/** Lazy so a deployment without a usable libvips still serves every other
 *  tool: the handler falls back to the ORIGINAL bytes when this returns null
 *  (never dropping the inline image, EXP-511). */
async function loadSharp(): Promise<SharpFactory | null> {
  const pending =
    sharpLoad ??
    (sharpLoad = (async () => {
      try {
        // sharp is CommonJS (`export =`), so the ESM namespace carries the
        // factory on `default` while the type is the callable itself.
        const loaded = (await import(`sharp`)) as unknown as SharpFactory & {
          default?: SharpFactory
        }
        return loaded.default ?? loaded
      } catch (error) {
        if (!loggedLoadFailure) {
          loggedLoadFailure = true
          console.warn(
            `[mcp] sharp unavailable, inlining images unbounded`,
            error
          )
        }
        return null
      }
    })())
  return pending
}

/**
 * Bound one image for inline MCP delivery.
 *
 * Returns the original untouched when it is already small (<= 300 KB) AND
 * small enough to read (longest edge <= 1280 px); otherwise a WebP re-encode
 * that fits inside 1280 px with EXIF orientation applied (both the Claude
 * and the MCP image content types accept `image/webp`). An animated GIF
 * contributes its first frame. Returns null when the bytes are not a decodable
 * image or sharp is unavailable — the caller then inlines the original.
 */
export async function inlineImageForContext(
  bytes: Uint8Array,
  contentType: string
): Promise<InlineImage | null> {
  const sharp = await loadSharp()
  if (!sharp) return null

  const input = Buffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  try {
    // `animated: false` (the default) reads the FIRST frame of a GIF/WebP
    // sequence, which is also what the re-encode below emits.
    const metadata = await sharp(input).metadata()
    const width = metadata.width
    const height = metadata.height
    const longestEdge = Math.max(width ?? 0, height ?? 0)

    if (
      longestEdge > 0 &&
      longestEdge <= MAX_INLINE_IMAGE_EDGE &&
      bytes.byteLength <= MAX_INLINE_IMAGE_BYTES
    ) {
      return {
        data: bytes,
        mimeType: contentType,
        width,
        height,
        downscaled: false,
      }
    }

    let best: InlineImage | null = null
    for (const quality of WEBP_QUALITY_LADDER) {
      const encoded = await sharp(input)
        // EXIF orientation applied, so a portrait phone shot does not reach
        // the agent on its side (and the resize below sees the real edges).
        .rotate()
        .resize({
          width: MAX_INLINE_IMAGE_EDGE,
          height: MAX_INLINE_IMAGE_EDGE,
          fit: `inside`,
          withoutEnlargement: true,
        })
        .webp({ quality })
        .toBuffer({ resolveWithObject: true })
      best = {
        data: new Uint8Array(
          encoded.data.buffer,
          encoded.data.byteOffset,
          encoded.data.byteLength
        ),
        mimeType: `image/webp`,
        width: encoded.info.width,
        height: encoded.info.height,
        downscaled: true,
      }
      if (encoded.data.byteLength <= MAX_INLINE_IMAGE_BYTES) return best
    }
    // Every rung tried; the smallest one is still the right answer.
    return best
  } catch (error) {
    console.warn(`[mcp] could not bound an inline image`, error)
    return null
  }
}
