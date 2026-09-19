import { describe, expect, it } from "vitest"
import { edgeKey, layoutWorkflow, type LayoutEdge } from "./workflow-layout"

const nodes = (...keys: string[]) => keys.map((key) => ({ id: key.toLowerCase(), key }))
const place = (keys: string[], edges: LayoutEdge[]) => {
  const layout = layoutWorkflow(nodes(...keys), edges)
  return {
    layout,
    at: Object.fromEntries(
      [...layout.placements].map(([id, p]) => [id, `${p.wave}:${p.lane}${p.cycle ? `!` : ``}`])
    ),
  }
}

describe(`layoutWorkflow`, () => {
  it(`lays an empty workflow out as nothing`, () => {
    const { layout } = place([], [])
    expect(layout.metrics).toEqual({ nodes: 0, edges: 0, depth: 0, width: 0, cycles: [] })
  })

  it(`puts unrelated nodes in one wave, by identifier`, () => {
    const { at, layout } = place([`C`, `A`, `B`], [])
    expect(at).toEqual({ a: `0:0`, b: `0:1`, c: `0:2` })
    expect(layout.metrics).toMatchObject({ depth: 1, width: 3 })
  })

  it(`layers by the LONGEST path: contract, fan-out, integration`, () => {
    const { at, layout } = place(
      [`K`, `L1`, `L2`, `L3`, `Z`],
      [
        [`k`, `l1`],
        [`k`, `l2`],
        [`k`, `l3`],
        [`l1`, `z`],
        [`l2`, `z`],
        [`l3`, `z`],
        [`k`, `z`],
      ]
    )
    expect(at).toEqual({ k: `0:0`, l1: `1:0`, l2: `1:1`, l3: `1:2`, z: `2:0` })
    expect(layout.metrics).toEqual({ nodes: 5, edges: 7, depth: 3, width: 3, cycles: [] })
  })

  it(`orders a wave by its neighbours, not by identifier, to uncross edges`, () => {
    // a → y, b → x: identifier order (x above y) would cross the two edges.
    const { at } = place(
      [`A`, `B`, `X`, `Y`],
      [
        [`a`, `y`],
        [`b`, `x`],
      ]
    )
    expect(at).toEqual({ a: `0:0`, b: `0:1`, y: `1:0`, x: `1:1` })
  })

  it(`flags a cycle, keeps it out of the layering and names it`, () => {
    const { at, layout } = place(
      [`A`, `B`, `C`, `D`],
      [
        [`a`, `b`],
        [`b`, `c`],
        [`c`, `b`],
        [`c`, `d`],
      ]
    )
    expect(at).toEqual({ a: `0:0`, c: `0:1!`, b: `1:0!`, d: `1:1` })
    expect([...layout.cycleEdges].sort()).toEqual([edgeKey(`b`, `c`), edgeKey(`c`, `b`)])
    expect(layout.metrics.cycles).toEqual([[`B`, `C`]])
  })

  it(`treats a self edge as a cycle and ignores unknown ends and duplicates`, () => {
    const { at, layout } = place(
      [`A`, `B`],
      [
        [`a`, `a`],
        [`a`, `b`],
        [`a`, `b`],
        [`ghost`, `b`],
      ]
    )
    expect(at).toEqual({ a: `0:0!`, b: `1:0` })
    expect(layout.metrics.edges).toBe(2)
    expect(layout.metrics.cycles).toEqual([[`A`]])
  })

  it(`survives a chain far deeper than the call stack would`, () => {
    const keys = Array.from({ length: 20000 }, (_, i) => `N${String(i).padStart(5, `0`)}`)
    const edges = keys.slice(1).map((key, i) => [keys[i]!.toLowerCase(), key.toLowerCase()] as const)
    const { layout } = place(keys, edges)
    expect(layout.metrics.depth).toBe(20000)
  })
})
