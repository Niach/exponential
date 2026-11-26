import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/data-viz.json" with { type: "json" }
import { ringArcs, seekFraction, waveformBars, WAVEFORM, type RingInput } from "./data-viz"

describe(`Group D data viz`, () => {
  test(`fixtures/data-viz.json replays`, () => {
    expect(fixture.waveform).toEqual({ ...WAVEFORM })
    for (const c of fixture.rings) expect(ringArcs(c.input as RingInput), c.name).toEqual(c.expected as never)
    for (const c of fixture.bars) expect(waveformBars(c.input as never), c.name).toEqual(c.expected as never)
    for (const c of fixture.seek) expect(seekFraction(c.x, c.width)).toBe(c.expected)
  })
})
