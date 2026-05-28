// Dependency-free intrinsic-dimension probing for the image formats we accept
// (png/jpeg/gif/webp). AVIF and anything unrecognized returns null — callers
// must treat dimensions as best-effort and never block an upload on them.

export interface ImageDimensions {
  width: number
  height: number
}

export function getImageDimensions(bytes: Uint8Array): ImageDimensions | null {
  return (
    pngDimensions(bytes) ??
    gifDimensions(bytes) ??
    webpDimensions(bytes) ??
    jpegDimensions(bytes)
  )
}

function pngDimensions(b: Uint8Array): ImageDimensions | null {
  // 8-byte signature + IHDR chunk: width @16 (BE u32), height @20 (BE u32).
  if (b.length < 24) return null
  const sig = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]
  if (!sig.every((v, i) => b[i] === v)) return null
  const view = new DataView(b.buffer, b.byteOffset, b.byteLength)
  return { width: view.getUint32(16), height: view.getUint32(20) }
}

function gifDimensions(b: Uint8Array): ImageDimensions | null {
  // "GIF87a"/"GIF89a"; logical screen width @6 (LE u16), height @8 (LE u16).
  if (b.length < 10) return null
  if (b[0] !== 0x47 || b[1] !== 0x49 || b[2] !== 0x46) return null
  const view = new DataView(b.buffer, b.byteOffset, b.byteLength)
  return { width: view.getUint16(6, true), height: view.getUint16(8, true) }
}

function webpDimensions(b: Uint8Array): ImageDimensions | null {
  // RIFF container: "RIFF"...."WEBP" then a VP8/VP8L/VP8X chunk.
  if (b.length < 30) return null
  if (b[0] !== 0x52 || b[1] !== 0x49 || b[2] !== 0x46 || b[3] !== 0x46) return null // RIFF
  if (b[8] !== 0x57 || b[9] !== 0x45 || b[10] !== 0x42 || b[11] !== 0x50) return null // WEBP
  const view = new DataView(b.buffer, b.byteOffset, b.byteLength)
  const fourCC = String.fromCharCode(b[12], b[13], b[14], b[15])

  if (fourCC === `VP8 `) {
    // Lossy: 16-bit width/height (14 bits used) at offset 26/28.
    const width = view.getUint16(26, true) & 0x3fff
    const height = view.getUint16(28, true) & 0x3fff
    return { width, height }
  }
  if (fourCC === `VP8L`) {
    // Lossless: dimensions packed into 4 bytes after the 0x2f signature @20.
    if (b[20] !== 0x2f) return null
    const bits = view.getUint32(21, true)
    const width = (bits & 0x3fff) + 1
    const height = ((bits >> 14) & 0x3fff) + 1
    return { width, height }
  }
  if (fourCC === `VP8X`) {
    // Extended: 24-bit canvas width/height (minus one) at offset 24/27.
    const width = (b[24] | (b[25] << 8) | (b[26] << 16)) + 1
    const height = (b[27] | (b[28] << 8) | (b[29] << 16)) + 1
    return { width, height }
  }
  return null
}

function jpegDimensions(b: Uint8Array): ImageDimensions | null {
  // Scan markers for a Start-Of-Frame (SOF0-SOF15, excluding DHT/JPG/DAC).
  if (b.length < 4 || b[0] !== 0xff || b[1] !== 0xd8) return null
  const view = new DataView(b.buffer, b.byteOffset, b.byteLength)
  let offset = 2
  while (offset + 9 < b.length) {
    if (b[offset] !== 0xff) {
      offset += 1
      continue
    }
    const marker = b[offset + 1]
    // SOF markers carry frame dimensions; C4 (DHT), C8 (JPG), CC (DAC) do not.
    const isSOF =
      marker >= 0xc0 &&
      marker <= 0xcf &&
      marker !== 0xc4 &&
      marker !== 0xc8 &&
      marker !== 0xcc
    if (isSOF) {
      const height = view.getUint16(offset + 5)
      const width = view.getUint16(offset + 7)
      return { width, height }
    }
    // Skip this segment using its length field.
    const segmentLength = view.getUint16(offset + 2)
    if (segmentLength < 2) return null
    offset += 2 + segmentLength
  }
  return null
}
