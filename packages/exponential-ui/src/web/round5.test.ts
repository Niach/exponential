// VAPP-162: the shared web halves of the round-5 natives. The Button hold
// machine replays fixtures/interactions-round1.json `buttonHold` without a
// DOM (the React and Solid painters replay it again through their DOM).

import { describe, expect, test } from "bun:test"
import { qrMatrix } from "../qr-code"
import fixture from "../../fixtures/interactions-round1.json" with { type: "json" }
import { ButtonHold, qrCornerPath, qrFeedColour, qrPaint, qrScanReporter, qrScanFormats, segmentedBadge, withinSlop, type HoldEvent } from "./round5"

describe(`qrPaint`, () => {
  test(`one path of every dark module, inside the quiet zone`, () => {
    const p = qrPaint(`https://exponential.at`, `M`, `#ffffff`, `#000000`)
    const m = qrMatrix(`https://exponential.at`, `M`)!
    expect(p.modules).toBe(m.size + 8)
    const dark = m.rows.join(``).split(``).filter((c) => c === `1`).length
    const runs = [...p.path.matchAll(/h(\d+)v1/g)].reduce((s, r) => s + Number(r[1]), 0)
    expect(runs).toBe(dark)
    expect(p.path.startsWith(`M4 4h`)).toBe(true)
  })
  test(`the lighter colour is always the plate`, () => {
    const dark = qrPaint(`x`, `L`, `#0a0a0a`, `#fafafa`)
    expect(dark.plate).toBe(`#fafafa`)
    expect(dark.module).toBe(`#0a0a0a`)
    const light = qrPaint(`x`, `L`, `#ffffff`, `#111111`)
    expect(light.plate).toBe(`#ffffff`)
  })
  test(`no matrix = the empty plate`, () => {
    expect(qrPaint(``, `M`, `#fff`, `#000`)).toMatchObject({ matrix: null, path: ``, modules: 0 })
  })
})

describe(`QrScanner helpers`, () => {
  test(`a value reports once until it changes`, () => {
    const seen: string[] = []
    const r = qrScanReporter((v) => seen.push(v))
    for (const v of [`a`, `a`, `b`, `b`, `a`, ``]) r(v)
    expect(seen).toEqual([`a`, `b`, `a`])
  })
  test(`formats default to qr`, () => {
    expect(qrScanFormats(undefined)).toEqual([`qr`])
    expect(qrScanFormats([`qr`, `ean13`])).toEqual([`qr`, `ean13`])
  })
  test(`corner brackets stay in their 24 px square`, () => {
    expect(qrCornerPath(`tl`, 0)).toBe(`M1.5 24V1.5A0 0 0 0 1 1.5 1.5H24`)
    expect(qrCornerPath(`br`, 13.5)).toBe(`M22.5 0V10.5A12 12 0 0 1 10.5 22.5H0`)
  })
  test(`the empty feed is the colour at 6 %`, () => {
    expect(qrFeedColour(`#000000`)).toBe(`rgba(0, 0, 0, 0.06)`)
  })
})

test(`segmentedBadge: hidden at 0, 99+ above 99`, () => {
  expect([0, -1, undefined, `x`, 1, 3, 99, 100, `7`].map(segmentedBadge)).toEqual([null, null, null, null, `1`, `3`, `99`, `99+`, `7`])
})

interface Step {
  do: string
  x?: number
  y?: number
  key?: string
  repeat?: boolean
  events: string[]
  pressed: boolean
  label: string
}
interface HoldCase {
  name: string
  props?: Record<string, unknown>
  steps: Step[]
}

describe(`fixtures/interactions-round1.json buttonHold (the machine)`, () => {
  const rect = { left: 0, top: 0, right: 120, bottom: 40 }
  for (const c of fixture.buttonHold as HoldCase[]) {
    // `hold: false` is the painter's plain press, not the machine's.
    if (c.props?.hold === false) continue
    test(c.name, () => {
      let fired: HoldEvent[] = []
      let pressed = false
      const disabled = c.props?.disabled === true
      const m = new ButtonHold((e) => fired.push(e), (p) => (pressed = p), () => disabled)
      const holding = c.props && `holdingLabel` in c.props ? (c.props.holdingLabel as string | null) : `Holding…`
      for (const s of c.steps) {
        fired = []
        if (s.do === `pointerdown`) m.pointerDown(1)
        else if (s.do === `pointermove`) m.pointerMove(rect, s.x!, s.y!, 1)
        else if (s.do === `pointerup`) m.pointerUp(rect, s.x!, s.y!, 1)
        else if (s.do === `pointercancel`) m.pointerCancel(1)
        else if (s.do === `keydown`) m.keyDown(s.key!, s.repeat === true)
        else if (s.do === `keyup`) m.keyUp(s.key!)
        else if (s.do === `blur`) m.blur()
        expect(fired).toEqual(s.events as HoldEvent[])
        expect(pressed).toBe(s.pressed)
        expect(pressed && holding ? holding : `Hold to talk`).toBe(s.label)
      }
    })
  }
  test(`the slop grows the box on every side`, () => {
    expect(withinSlop({ left: 0, top: 0, right: 10, bottom: 10 }, -24, 34)).toBe(true)
    expect(withinSlop({ left: 0, top: 0, right: 10, bottom: 10 }, -25, 5)).toBe(false)
  })
})
