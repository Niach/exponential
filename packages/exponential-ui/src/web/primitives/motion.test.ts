// EXP-1604: the web Motion helpers shared by React, Solid and the element
// (the Canvas2D painter itself is gated in Chromium: conformance/motion-parity.ts).
import { describe, expect, it } from "bun:test"
import { deflateRawSync } from "node:zlib"
import { builtinTheme } from "../../themes"
import { motionA11y, motionBoxStyle, motionDeclaredSize, motionFit, motionFitMatrix, motionFromDotLottie, motionThemeSlots } from "./motion"

describe(`motion box (contract §4)`, () => {
  it(`unstyled: the composition's px width, shrinking to the width, aspect kept`, () =>
    expect(motionBoxStyle([200, 100], undefined)).toEqual({ width: `200px`, maxWidth: `100%`, aspectRatio: `200 / 100` }))
  it(`a style width or height wins; the aspect fills the other edge`, () => {
    expect(motionBoxStyle([200, 100], { width: 64 })).toEqual({ aspectRatio: `200 / 100` })
    expect(motionBoxStyle([200, 100], { height: 10 })).toEqual({ aspectRatio: `200 / 100` })
    expect(motionBoxStyle([200, 100], { width: 64, height: 10 })).toEqual({})
  })
  it(`the declared size: dialect size, inline Lottie w/h, none for src`, () => {
    expect(motionDeclaredSize({ layers: [], size: [48, 32] })).toEqual([48, 32])
    expect(motionDeclaredSize({ lottie: { w: 10, h: 20 } })).toEqual([10, 20])
    expect(motionDeclaredSize({ src: `/api/attachments/a` })).toBeNull()
  })
})

describe(`fit`, () => {
  it(`contain centres, cover overflows, fill stretches`, () => {
    expect(motionFitMatrix(100, 50, 160, 160, `contain`)).toEqual([1.6, 0, 0, 1.6, 0, 40])
    expect(motionFitMatrix(100, 50, 160, 160, `cover`)).toEqual([3.2, 0, 0, 3.2, -80, 0])
    expect(motionFitMatrix(100, 50, 160, 160, `fill`)).toEqual([1.6, 0, 0, 3.2, 0, 0])
    expect(motionFit(`nope`)).toBe(`contain`)
  })
})

describe(`theme slots (contract §2)`, () => {
  const theme = builtinTheme(`exponential`)
  it(`every $color slot takes the mode's colour; dark adds $mode; the node's slots last`, () => {
    const light = motionThemeSlots([`$color.primary`, `other`], theme, `light`, undefined)
    expect(Object.keys(light)).toEqual([`$color.primary`])
    const dark = motionThemeSlots([`$color.primary`], theme, `dark`, { "$color.primary": { p: { a: 0, k: [1, 0, 0, 1] } } })
    expect(dark.$mode).toBe(`dark`)
    expect(dark[`$color.primary`]).toEqual({ p: { a: 0, k: [1, 0, 0, 1] } })
    const k = (light[`$color.primary`] as { p: { k: number[] } }).p.k
    expect(k).toHaveLength(4)
    k.forEach((v) => expect(v >= 0 && v <= 1).toBe(true))
  })
})

describe(`a11y (contract §6)`, () => {
  it(`img named by the label; hidden without one`, () => {
    expect(motionA11y(`Loading`)).toEqual({ role: `img`, "aria-label": `Loading` })
    expect(motionA11y(``)).toEqual({ "aria-hidden": `true` })
  })
})

/** A minimal zip (deflate entries) for the dotLottie reader. */
function zip(files: Record<string, Uint8Array>): Uint8Array {
  const enc = new TextEncoder()
  const local: Uint8Array[] = []
  const central: Uint8Array[] = []
  let offset = 0
  for (const [name, data] of Object.entries(files)) {
    const n = enc.encode(name)
    const body = new Uint8Array(deflateRawSync(data))
    const h = new DataView(new ArrayBuffer(30))
    h.setUint32(0, 0x04034b50, true)
    h.setUint16(8, 8, true)
    h.setUint32(18, body.length, true)
    h.setUint32(22, data.length, true)
    h.setUint16(26, n.length, true)
    local.push(new Uint8Array(h.buffer), n, body)
    const c = new DataView(new ArrayBuffer(46))
    c.setUint32(0, 0x02014b50, true)
    c.setUint16(10, 8, true)
    c.setUint32(20, body.length, true)
    c.setUint32(24, data.length, true)
    c.setUint16(28, n.length, true)
    c.setUint32(42, offset, true)
    central.push(new Uint8Array(c.buffer), n)
    offset += 30 + n.length + body.length
  }
  const cdSize = central.reduce((a, b) => a + b.length, 0)
  const end = new DataView(new ArrayBuffer(22))
  end.setUint32(0, 0x06054b50, true)
  end.setUint16(8, Object.keys(files).length, true)
  end.setUint16(10, Object.keys(files).length, true)
  end.setUint32(12, cdSize, true)
  end.setUint32(16, offset, true)
  const parts = [...local, ...central, new Uint8Array(end.buffer)]
  const out = new Uint8Array(parts.reduce((a, b) => a + b.length, 0))
  let p = 0
  for (const part of parts) {
    out.set(part, p)
    p += part.length
  }
  return out
}

describe(`dotLottie`, () => {
  it(`reads the manifest's first animation and inlines its images`, async () => {
    const enc = new TextEncoder()
    const anim = { v: `5.7.0`, w: 10, h: 10, assets: [{ id: `i`, u: `/images/`, p: `a.png`, w: 1, h: 1 }], layers: [] }
    const bytes = zip({ "manifest.json": enc.encode(JSON.stringify({ animations: [{ id: `wave` }] })), "animations/wave.json": enc.encode(JSON.stringify(anim)), "images/a.png": new Uint8Array([1, 2, 3]) })
    const lottie = JSON.parse(await motionFromDotLottie(bytes)) as { assets: { p: string; e: number }[] }
    expect(lottie.assets[0]!.p).toBe(`data:image/png;base64,AQID`)
    expect(lottie.assets[0]!.e).toBe(1)
  })
})
