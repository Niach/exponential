import { describe, expect, it } from "vitest"
import { reviewPoints, reviewWaveGate } from "./workflow-waves"

describe(`reviewPoints`, () => {
  it(`is one wave at the end up to three layers, and every third layer after the contract beyond`, () => {
    expect(reviewPoints(0)).toEqual([])
    expect(reviewPoints(1)).toEqual([0])
    expect(reviewPoints(3)).toEqual([2])
    expect(reviewPoints(4)).toEqual([0, 3])
    expect(reviewPoints(5)).toEqual([0, 3, 4])
    expect(reviewPoints(7)).toEqual([0, 3, 6])
    expect(reviewPoints(8)).toEqual([0, 3, 6, 7])
  })
})

describe(`reviewWaveGate`, () => {
  const node = (wave: number, state: string, approved = false) => ({
    wave,
    state,
    approvedAt: approved ? new Date() : null,
  })

  it(`never gates a shallow graph: its one wave follows the last layer`, () => {
    const nodes = [node(0, `landed`), node(1, `in_review`), node(2, `blocked`)]
    expect(reviewWaveGate(nodes, 1)).toBeNull()
    expect(reviewWaveGate(nodes, 2)).toBeNull()
  })

  it(`gates the layers behind an uncleared mid wave of a deep graph`, () => {
    const deep = [node(0, `landed`), node(1, `in_review`), node(2, `blocked`), node(3, `blocked`)]
    expect(reviewWaveGate(deep, 0)).toBeNull()
    expect(reviewWaveGate(deep, 1)).toBe(0)
    expect(reviewWaveGate(deep, 3)).toBe(0)
    const cleared = [node(0, `landed`, true), ...deep.slice(1)]
    expect(reviewWaveGate(cleared, 1)).toBeNull()
    // A skipped node clears vacuously; an unfinished one holds the wave.
    expect(reviewWaveGate([node(0, `skipped`), ...deep.slice(1)], 1)).toBeNull()
    expect(reviewWaveGate([node(0, `running`), ...deep.slice(1)], 1)).toBe(0)
  })
})
