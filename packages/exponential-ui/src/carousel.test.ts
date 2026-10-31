import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/carousel.json" with { type: "json" }
import { carouselSpec } from "./carousel"

describe(`carousel spec (EXP-1493)`, () => {
  test(`fixtures/carousel.json replays`, () => {
    for (const c of fixture.cases) expect(carouselSpec({ theme: c.theme, mode: c.mode as `light` | `dark`, pages: c.pages, ...c.props }), c.name).toEqual(c.expected as never)
  })
  test(`the current dot differs from the others; the counter is never a white-on-black pill`, () => {
    const s = carouselSpec({ theme: `neutral`, mode: `dark`, pages: 3, page: 1, counter: true })
    expect(s.dots[1]!.color).not.toBe(s.dots[0]!.color)
    expect(s.order[0]).toBe(`counter`)
    expect(s.counter!.color).not.toBe(`#ffffff`)
  })
})
