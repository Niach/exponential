/* A webp's pixel size from its header (VP8 / VP8L / VP8X), so every
   picture renders with width + height and reserves its box (no layout
   shift when a lazy image arrives). */
import { closeSync, openSync, readSync } from "node:fs"

export function webpSize(file: string): { width: number; height: number } | null {
  const fd = openSync(file, `r`)
  const b = Buffer.alloc(30)
  readSync(fd, b, 0, 30, 0)
  closeSync(fd)
  if (b.toString(`ascii`, 0, 4) !== `RIFF` || b.toString(`ascii`, 8, 12) !== `WEBP`) return null
  const chunk = b.toString(`ascii`, 12, 16)
  if (chunk === `VP8X`) return { width: 1 + b.readUIntLE(24, 3), height: 1 + b.readUIntLE(27, 3) }
  if (chunk === `VP8 `) return { width: b.readUInt16LE(26) & 0x3fff, height: b.readUInt16LE(28) & 0x3fff }
  if (chunk === `VP8L`) {
    const bits = b.readUInt32LE(21)
    return { width: (bits & 0x3fff) + 1, height: ((bits >> 14) & 0x3fff) + 1 }
  }
  return null
}
