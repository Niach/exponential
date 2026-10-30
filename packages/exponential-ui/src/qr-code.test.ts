import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/qr-code.json" with { type: "json" }
import { QR_CODE, qrLayout, qrMatrix, qrVersion } from "./qr-code"

describe(`VAPP-162 QR code`, () => {
  test(`fixtures/qr-code.json replays`, () => {
    for (const c of fixture.cases) expect(qrMatrix(c.value, c.level), c.name).toEqual(c.expected as never)
  })

  test(`byte capacities match the spec's table (versions 1, 10, 40 × L M Q H)`, () => {
    const cap = (v: number, level: `L` | `M` | `Q` | `H`) => {
      let n = 0
      while ((qrVersion(n + 1, level) ?? 99) <= v) n++
      return n
    }
    expect([`L`, `M`, `Q`, `H`].map((l) => cap(1, l as never))).toEqual([17, 14, 11, 7])
    expect([`L`, `M`, `Q`, `H`].map((l) => cap(10, l as never))).toEqual([271, 213, 151, 119])
    expect([`L`, `M`, `Q`, `H`].map((l) => cap(40, l as never))).toEqual([2953, 2331, 1663, 1273])
    expect(QR_CODE.maxValueBytes).toBe(2953)
  })

  test(`the matrix is square with the three finder patterns`, () => {
    const m = qrMatrix(`HELLO`, `M`)!
    expect(m.rows).toHaveLength(m.size)
    for (const row of m.rows) expect(row).toHaveLength(m.size)
    const finder = [`1111111`, `1000001`, `1011101`, `1011101`, `1011101`, `1000001`, `1111111`]
    for (let i = 0; i < 7; i++) {
      expect(m.rows[i]!.slice(0, 7)).toBe(finder[i]!)
      expect(m.rows[i]!.slice(m.size - 7)).toBe(finder[i]!)
      expect(m.rows[m.size - 7 + i]!.slice(0, 7)).toBe(finder[i]!)
    }
  })

  test(`empty and oversized values have no matrix; the layout keeps a 4-module quiet zone`, () => {
    expect(qrMatrix(``)).toBeNull()
    expect(qrMatrix(`x`.repeat(2954), `L`)).toBeNull()
    expect(qrMatrix(`x`.repeat(2953), `L`)!.version).toBe(40)
    expect(qrLayout(160, 21)).toEqual({ module: 160 / 29, inset: (160 / 29) * 4 })
  })
})
