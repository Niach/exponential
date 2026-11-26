// genUI round 4: fixtures/box-backgrounds.json on the TS reference (the
// validator, the shared geometry and the web node sheet's layers). The Rust
// core (eui-core tests/box_backgrounds.rs), Swift and Kotlin replay it too.
import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/box-backgrounds.json" with { type: "json" }
import { pictureRect, radialRadius, radialT } from "./background"
import { validateStyle } from "./style"
import { declarations } from "./web/theme-css"

describe(`box backgrounds`, () => {
  for (const c of fixture.validate) {
    test(`validate: ${c.name}`, () => {
      expect(validateStyle(c.style, { path: `style` })).toEqual(c.issues)
    })
  }
  for (const c of fixture.radial) {
    test(`radial: ${c.name}`, () => {
      const [w, h] = c.size
      const [cx, cy] = c.centre
      expect(radialRadius(w!, h!, cx!, cy!)).toBeCloseTo(c.radius, 4)
      for (const [px, py, t] of c.samples) expect(radialT(w!, h!, cx!, cy!, px!, py!)).toBeCloseTo(t!, 4)
    })
  }
  for (const c of fixture.picture) {
    test(`picture: ${c.name}`, () => {
      const got = pictureRect(c.fit, c.size as [number, number], c.image as [number, number], c.focal as [number, number])
      got.forEach((v, i) => expect(v).toBeCloseTo(c.rect[i]!, 4))
    })
  }
  for (const c of fixture.css) {
    test(`css: ${c.name}`, () => {
      const style = c.style as Record<string, unknown>
      expect(declarations(style, {}, style)).toEqual(c.declarations as [string, string][])
    })
  }
  test(`a block that changes only the gradient keeps the picture layers`, () => {
    const base = { backgroundImage: { src: `a.png` } }
    const block = { backgroundGradient: { kind: `radial`, stops: [{ color: `$color.primary`, offset: 0 }, { color: `$color.card`, offset: 1 }] } }
    const out = Object.fromEntries(declarations(block, {}, base))
    expect(out[`background-image`]).toContain(`var(--xui-bgi`)
    expect(out[`background-image`]!.startsWith(`radial-gradient(circle farthest-corner at 50% 50%`)).toBe(true)
  })
})
