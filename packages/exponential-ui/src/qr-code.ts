// VAPP-162: the QrCode native's MATRIX (catalog/style.json `qrCode`). A
// faithful TypeScript port of Project Nayuki's QR Code generator library
// (https://www.nayuki.io/page/qr-code-generator-library, MIT licence,
// Copyright (c) Project Nayuki; the notice is in docs/third-party-licences.md),
// narrowed to what the catalog needs: ONE byte-mode segment over the value's
// UTF-8 bytes, the smallest version 1–40 that fits, the given error
// correction level kept exact (Nayuki's `boostEcl` OFF), the mask with the
// lowest penalty. Pure; the Rust core ports it and fixtures/qr-code.json
// locks every matrix.

import styleJson from "../catalog/style.json" with { type: "json" }

export const QR_CODE = styleJson.qrCode

export const QR_LEVELS = [`L`, `M`, `Q`, `H`] as const
export type QrLevel = (typeof QR_LEVELS)[number]

export interface QrMatrix {
  /** Modules per side (21 + 4 × (version − 1)). */
  size: number
  version: number
  /** The mask pattern chosen (0..7). */
  mask: number
  /** `size` rows of `size` characters: '1' = dark, '0' = light; no quiet zone. */
  rows: string[]
}

/** The level as the catalog names it; anything else is `M` (the default). */
export function qrLevel(value: unknown): QrLevel {
  return (QR_LEVELS as readonly unknown[]).includes(value) ? (value as QrLevel) : `M`
}

// Nayuki: ECC_CODEWORDS_PER_BLOCK / NUM_ERROR_CORRECTION_BLOCKS, indexed
// [level ordinal L M Q H][version]; index 0 is padding.
const ECC_CODEWORDS_PER_BLOCK: readonly (readonly number[])[] = [
  [-1, 7, 10, 15, 20, 26, 18, 20, 24, 30, 18, 20, 24, 26, 30, 22, 24, 28, 30, 28, 28, 28, 28, 30, 30, 26, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
  [-1, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28],
  [-1, 13, 22, 18, 26, 18, 24, 18, 22, 20, 24, 28, 26, 24, 20, 30, 24, 28, 28, 26, 30, 28, 30, 30, 30, 30, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
  [-1, 17, 28, 22, 16, 22, 28, 26, 26, 24, 28, 24, 28, 22, 24, 24, 30, 28, 28, 26, 28, 30, 24, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
]
const NUM_ERROR_CORRECTION_BLOCKS: readonly (readonly number[])[] = [
  [-1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 4, 4, 6, 6, 6, 6, 7, 8, 8, 9, 9, 10, 12, 12, 12, 13, 14, 15, 16, 17, 18, 19, 19, 20, 21, 22, 24, 25],
  [-1, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21, 23, 25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49],
  [-1, 1, 1, 2, 2, 4, 4, 6, 6, 8, 8, 8, 10, 12, 16, 12, 17, 16, 18, 21, 20, 23, 23, 25, 27, 29, 34, 34, 35, 38, 40, 43, 45, 48, 51, 53, 56, 59, 62, 65, 68],
  [-1, 1, 1, 2, 4, 4, 4, 5, 6, 8, 8, 11, 11, 16, 16, 18, 16, 19, 21, 25, 25, 25, 34, 30, 32, 35, 37, 40, 42, 45, 48, 51, 54, 57, 60, 63, 66, 70, 74, 77, 81],
]
/** The two format bits the spec gives each level (L 1, M 0, Q 3, H 2). */
const FORMAT_BITS: Record<QrLevel, number> = { L: 1, M: 0, Q: 3, H: 2 }
const ORDINAL: Record<QrLevel, number> = { L: 0, M: 1, Q: 2, H: 3 }

const PENALTY_N1 = 3
const PENALTY_N2 = 3
const PENALTY_N3 = 40
const PENALTY_N4 = 10

function utf8(value: string): number[] {
  return [...new TextEncoder().encode(value)]
}

/** Raw data modules of a version (all modules minus the function patterns). */
function numRawDataModules(ver: number): number {
  let result = (16 * ver + 128) * ver + 64
  if (ver >= 2) {
    const numAlign = Math.floor(ver / 7) + 2
    result -= (25 * numAlign - 10) * numAlign - 55
    if (ver >= 7) result -= 36
  }
  return result
}

function numDataCodewords(ver: number, level: QrLevel): number {
  const o = ORDINAL[level]
  return Math.floor(numRawDataModules(ver) / 8) - ECC_CODEWORDS_PER_BLOCK[o]![ver]! * NUM_ERROR_CORRECTION_BLOCKS[o]![ver]!
}

/** Byte mode's character-count field width per version band. */
const byteCountBits = (ver: number) => (ver < 10 ? 8 : 16)

// --- Reed–Solomon over GF(2^8 / 0x11D) --------------------------------------

function rsMultiply(x: number, y: number): number {
  let z = 0
  for (let i = 7; i >= 0; i--) {
    z = (z << 1) ^ ((z >>> 7) * 0x11d)
    z ^= ((y >>> i) & 1) * x
  }
  return z & 0xff
}

function rsDivisor(degree: number): number[] {
  const result: number[] = new Array(degree - 1).fill(0)
  result.push(1)
  let root = 1
  for (let i = 0; i < degree; i++) {
    for (let j = 0; j < result.length; j++) {
      result[j] = rsMultiply(result[j]!, root)
      if (j + 1 < result.length) result[j] = result[j]! ^ result[j + 1]!
    }
    root = rsMultiply(root, 0x02)
  }
  return result
}

function rsRemainder(data: readonly number[], divisor: readonly number[]): number[] {
  const result = divisor.map(() => 0)
  for (const b of data) {
    const factor = b ^ result.shift()!
    result.push(0)
    divisor.forEach((coef, i) => (result[i] = result[i]! ^ rsMultiply(coef, factor)))
  }
  return result
}

// --- The symbol ---------------------------------------------------------------

class QrSymbol {
  readonly size: number
  readonly modules: boolean[][]
  readonly isFunction: boolean[][]

  constructor(
    readonly version: number,
    readonly level: QrLevel
  ) {
    this.size = version * 4 + 17
    this.modules = Array.from({ length: this.size }, () => new Array<boolean>(this.size).fill(false))
    this.isFunction = Array.from({ length: this.size }, () => new Array<boolean>(this.size).fill(false))
  }

  setFunction(x: number, y: number, dark: boolean): void {
    this.modules[y]![x] = dark
    this.isFunction[y]![x] = true
  }

  drawFunctionPatterns(): void {
    for (let i = 0; i < this.size; i++) {
      this.setFunction(6, i, i % 2 === 0)
      this.setFunction(i, 6, i % 2 === 0)
    }
    this.drawFinder(3, 3)
    this.drawFinder(this.size - 4, 3)
    this.drawFinder(3, this.size - 4)
    const align = this.alignmentPositions()
    const n = align.length
    for (let i = 0; i < n; i++) {
      for (let j = 0; j < n; j++) {
        if (!((i === 0 && j === 0) || (i === 0 && j === n - 1) || (i === n - 1 && j === 0))) this.drawAlignment(align[i]!, align[j]!)
      }
    }
    this.drawFormatBits(0) // dummy, overwritten after the mask is chosen
    this.drawVersion()
  }

  drawFormatBits(mask: number): void {
    const data = (FORMAT_BITS[this.level] << 3) | mask
    let rem = data
    for (let i = 0; i < 10; i++) rem = (rem << 1) ^ ((rem >>> 9) * 0x537)
    const bits = ((data << 10) | rem) ^ 0x5412
    const bit = (i: number) => ((bits >>> i) & 1) !== 0
    for (let i = 0; i <= 5; i++) this.setFunction(8, i, bit(i))
    this.setFunction(8, 7, bit(6))
    this.setFunction(8, 8, bit(7))
    this.setFunction(7, 8, bit(8))
    for (let i = 9; i < 15; i++) this.setFunction(14 - i, 8, bit(i))
    for (let i = 0; i < 8; i++) this.setFunction(this.size - 1 - i, 8, bit(i))
    for (let i = 8; i < 15; i++) this.setFunction(8, this.size - 15 + i, bit(i))
    this.setFunction(8, this.size - 8, true) // always dark
  }

  drawVersion(): void {
    if (this.version < 7) return
    let rem = this.version
    for (let i = 0; i < 12; i++) rem = (rem << 1) ^ ((rem >>> 11) * 0x1f25)
    const bits = (this.version << 12) | rem
    for (let i = 0; i < 18; i++) {
      const dark = ((bits >>> i) & 1) !== 0
      const a = this.size - 11 + (i % 3)
      const b = Math.floor(i / 3)
      this.setFunction(a, b, dark)
      this.setFunction(b, a, dark)
    }
  }

  drawFinder(x: number, y: number): void {
    for (let dy = -4; dy <= 4; dy++) {
      for (let dx = -4; dx <= 4; dx++) {
        const dist = Math.max(Math.abs(dx), Math.abs(dy))
        const xx = x + dx
        const yy = y + dy
        if (xx >= 0 && xx < this.size && yy >= 0 && yy < this.size) this.setFunction(xx, yy, dist !== 2 && dist !== 4)
      }
    }
  }

  drawAlignment(x: number, y: number): void {
    for (let dy = -2; dy <= 2; dy++) for (let dx = -2; dx <= 2; dx++) this.setFunction(x + dx, y + dy, Math.max(Math.abs(dx), Math.abs(dy)) !== 1)
  }

  alignmentPositions(): number[] {
    if (this.version === 1) return []
    const numAlign = Math.floor(this.version / 7) + 2
    const step = Math.floor((this.version * 8 + numAlign * 3 + 5) / (numAlign * 4 - 4)) * 2
    const result = [6]
    for (let pos = this.size - 7; result.length < numAlign; pos -= step) result.splice(1, 0, pos)
    return result
  }

  addEccAndInterleave(data: readonly number[]): number[] {
    const o = ORDINAL[this.level]
    const numBlocks = NUM_ERROR_CORRECTION_BLOCKS[o]![this.version]!
    const blockEccLen = ECC_CODEWORDS_PER_BLOCK[o]![this.version]!
    const rawCodewords = Math.floor(numRawDataModules(this.version) / 8)
    const numShortBlocks = numBlocks - (rawCodewords % numBlocks)
    const shortBlockLen = Math.floor(rawCodewords / numBlocks)
    const blocks: number[][] = []
    const divisor = rsDivisor(blockEccLen)
    for (let i = 0, k = 0; i < numBlocks; i++) {
      const dat = data.slice(k, k + shortBlockLen - blockEccLen + (i < numShortBlocks ? 0 : 1))
      k += dat.length
      const ecc = rsRemainder(dat, divisor)
      if (i < numShortBlocks) dat.push(0)
      blocks.push(dat.concat(ecc))
    }
    const result: number[] = []
    for (let i = 0; i < blocks[0]!.length; i++) {
      blocks.forEach((block, j) => {
        if (i !== shortBlockLen - blockEccLen || j >= numShortBlocks) result.push(block[i]!)
      })
    }
    return result
  }

  drawCodewords(data: readonly number[]): void {
    let i = 0
    for (let right = this.size - 1; right >= 1; right -= 2) {
      if (right === 6) right = 5
      for (let vert = 0; vert < this.size; vert++) {
        for (let j = 0; j < 2; j++) {
          const x = right - j
          const upward = ((right + 1) & 2) === 0
          const y = upward ? this.size - 1 - vert : vert
          if (!this.isFunction[y]![x] && i < data.length * 8) {
            this.modules[y]![x] = ((data[i >>> 3]! >>> (7 - (i & 7))) & 1) !== 0
            i++
          }
        }
      }
    }
  }

  applyMask(mask: number): void {
    for (let y = 0; y < this.size; y++) {
      for (let x = 0; x < this.size; x++) {
        let invert: boolean
        switch (mask) {
          case 0: invert = (x + y) % 2 === 0; break
          case 1: invert = y % 2 === 0; break
          case 2: invert = x % 3 === 0; break
          case 3: invert = (x + y) % 3 === 0; break
          case 4: invert = (Math.floor(x / 3) + Math.floor(y / 2)) % 2 === 0; break
          case 5: invert = ((x * y) % 2) + ((x * y) % 3) === 0; break
          case 6: invert = (((x * y) % 2) + ((x * y) % 3)) % 2 === 0; break
          default: invert = (((x + y) % 2) + ((x * y) % 3)) % 2 === 0; break
        }
        if (!this.isFunction[y]![x] && invert) this.modules[y]![x] = !this.modules[y]![x]
      }
    }
  }

  penaltyScore(): number {
    let result = 0
    const size = this.size
    for (let y = 0; y < size; y++) {
      let runColor = false
      let runX = 0
      const runHistory = [0, 0, 0, 0, 0, 0, 0]
      for (let x = 0; x < size; x++) {
        if (this.modules[y]![x] === runColor) {
          runX++
          if (runX === 5) result += PENALTY_N1
          else if (runX > 5) result++
        } else {
          this.finderPenaltyAddHistory(runX, runHistory)
          if (!runColor) result += this.finderPenaltyCountPatterns(runHistory) * PENALTY_N3
          runColor = this.modules[y]![x]!
          runX = 1
        }
      }
      result += this.finderPenaltyTerminateAndCount(runColor, runX, runHistory) * PENALTY_N3
    }
    for (let x = 0; x < size; x++) {
      let runColor = false
      let runY = 0
      const runHistory = [0, 0, 0, 0, 0, 0, 0]
      for (let y = 0; y < size; y++) {
        if (this.modules[y]![x] === runColor) {
          runY++
          if (runY === 5) result += PENALTY_N1
          else if (runY > 5) result++
        } else {
          this.finderPenaltyAddHistory(runY, runHistory)
          if (!runColor) result += this.finderPenaltyCountPatterns(runHistory) * PENALTY_N3
          runColor = this.modules[y]![x]!
          runY = 1
        }
      }
      result += this.finderPenaltyTerminateAndCount(runColor, runY, runHistory) * PENALTY_N3
    }
    for (let y = 0; y < size - 1; y++) {
      for (let x = 0; x < size - 1; x++) {
        const color = this.modules[y]![x]
        if (color === this.modules[y]![x + 1] && color === this.modules[y + 1]![x] && color === this.modules[y + 1]![x + 1]) result += PENALTY_N2
      }
    }
    let dark = 0
    for (const row of this.modules) dark = row.reduce((sum, c) => sum + (c ? 1 : 0), dark)
    const total = size * size
    const k = Math.ceil(Math.abs(dark * 20 - total * 10) / total) - 1
    result += k * PENALTY_N4
    return result
  }

  finderPenaltyCountPatterns(runHistory: readonly number[]): number {
    const n = runHistory[1]!
    const core = n > 0 && runHistory[2] === n && runHistory[3] === n * 3 && runHistory[4] === n && runHistory[5] === n
    return (core && runHistory[0]! >= n * 4 && runHistory[6]! >= n ? 1 : 0) + (core && runHistory[6]! >= n * 4 && runHistory[0]! >= n ? 1 : 0)
  }

  finderPenaltyTerminateAndCount(currentRunColor: boolean, currentRunLength: number, runHistory: number[]): number {
    if (currentRunColor) {
      this.finderPenaltyAddHistory(currentRunLength, runHistory)
      currentRunLength = 0
    }
    currentRunLength += this.size
    this.finderPenaltyAddHistory(currentRunLength, runHistory)
    return this.finderPenaltyCountPatterns(runHistory)
  }

  finderPenaltyAddHistory(currentRunLength: number, runHistory: number[]): void {
    if (runHistory[0] === 0) currentRunLength += this.size
    runHistory.pop()
    runHistory.unshift(currentRunLength)
  }
}

/** The data codewords of one byte segment at `version` (header, data,
 *  terminator, byte padding, the 0xEC/0x11 pad bytes). */
function dataCodewords(bytes: readonly number[], version: number, level: QrLevel): number[] {
  const bits: number[] = []
  const append = (val: number, len: number) => {
    for (let i = len - 1; i >= 0; i--) bits.push((val >>> i) & 1)
  }
  append(0x4, 4)
  append(bytes.length, byteCountBits(version))
  for (const b of bytes) append(b, 8)
  const capacityBits = numDataCodewords(version, level) * 8
  append(0, Math.min(4, capacityBits - bits.length))
  append(0, (8 - (bits.length % 8)) % 8)
  for (let pad = 0xec; bits.length < capacityBits; pad ^= 0xec ^ 0x11) append(pad, 8)
  const out: number[] = new Array(bits.length / 8).fill(0)
  bits.forEach((b, i) => (out[i >>> 3] = out[i >>> 3]! | (b << (7 - (i & 7)))))
  return out
}

/** The smallest version whose data capacity holds `bytes` at `level`, or null. */
export function qrVersion(byteLength: number, level: QrLevel): number | null {
  for (let version = 1; version <= 40; version++) {
    const used = 4 + byteCountBits(version) + byteLength * 8
    if (byteLength < 1 << byteCountBits(version) && used <= numDataCodewords(version, level) * 8) return version
  }
  return null
}

/** The QR matrix of `value` (UTF-8, byte mode) at `level`; null for an empty
 *  value or one too long for version 40 at that level. */
export function qrMatrix(value: string, rawLevel: unknown = `M`): QrMatrix | null {
  const level = qrLevel(rawLevel)
  if (typeof value !== `string` || value.length === 0) return null
  const bytes = utf8(value)
  const version = qrVersion(bytes.length, level)
  if (version === null) return null
  const sym = new QrSymbol(version, level)
  sym.drawFunctionPatterns()
  sym.drawCodewords(sym.addEccAndInterleave(dataCodewords(bytes, version, level)))
  let mask = 0
  let minPenalty = Infinity
  for (let i = 0; i < 8; i++) {
    sym.applyMask(i)
    sym.drawFormatBits(i)
    const penalty = sym.penaltyScore()
    if (penalty < minPenalty) {
      mask = i
      minPenalty = penalty
    }
    sym.applyMask(i) // XOR undoes it
  }
  sym.applyMask(mask)
  sym.drawFormatBits(mask)
  return { size: sym.size, version, mask, rows: sym.modules.map((row) => row.map((d) => (d ? `1` : `0`)).join(``)) }
}

/** The painted geometry of a QrCode of edge `size` px (style.json `qrCode`):
 *  the module edge and the quiet zone's inset, both in px. */
export function qrLayout(size: number, modules: number): { module: number; inset: number } {
  const module = size / (modules + 2 * QR_CODE.quietZone)
  return { module, inset: module * QR_CODE.quietZone }
}
