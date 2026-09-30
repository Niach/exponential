import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { WaveGraph, waveGraphSize, type WaveGraphNode } from "./wave-graph"

// SLOP-16: the blocks mini-graph draws the grid VERTICAL (waves = rows, lanes
// = columns); the workflow graph keeps the horizontal default.

const metrics = { nodeWidth: 100, nodeHeight: 20, waveGap: 24, laneGap: 12 }
const nodes: WaveGraphNode[] = [
  { id: `a`, wave: 0, lane: 0 },
  { id: `b`, wave: 1, lane: 1 },
]

const box = (container: HTMLElement, id: string) =>
  container.querySelector(`[data-testid="g-node-${id}"]`) as HTMLElement

describe(`waveGraphSize`, () => {
  it(`is horizontal by default: waves are columns`, () => {
    expect(waveGraphSize(nodes, metrics)).toEqual({ width: 224, height: 52 })
  })
  it(`vertical: waves are rows, lanes columns`, () => {
    expect(waveGraphSize(nodes, metrics, `vertical`)).toEqual({ width: 212, height: 64 })
  })
  it(`is empty with no node`, () => {
    expect(waveGraphSize([], metrics, `vertical`)).toEqual({ width: 0, height: 0 })
  })
})

describe(`WaveGraph orientation`, () => {
  const draw = (orientation?: `horizontal` | `vertical`) =>
    render(
      <WaveGraph
        nodes={nodes}
        edges={[{ from: `a`, to: `b`, style: `plain` }]}
        {...metrics}
        orientation={orientation}
        idPrefix="g"
        renderNode={(id) => id}
      />
    ).container

  it(`places a horizontal node by wave → x`, () => {
    const container = draw()
    expect(box(container, `b`).style.left).toBe(`124px`)
    expect(box(container, `b`).style.top).toBe(`32px`)
    const path = container.querySelector(`[data-testid="g-edge"]`)!
    expect(path.getAttribute(`d`)!.startsWith(`M 100 10`)).toBe(true)
  })

  it(`places a vertical node by lane → x, wave → y, sized to the grid`, () => {
    const container = draw(`vertical`)
    const root = container.firstElementChild as HTMLElement
    expect(root.dataset.orientation).toBe(`vertical`)
    expect(root.style.width).toBe(`212px`)
    expect(root.style.height).toBe(`64px`)
    expect(box(container, `b`).style.left).toBe(`112px`)
    expect(box(container, `b`).style.top).toBe(`44px`)
  })

  it(`draws a vertical edge bottom-middle → top-middle, bent inside the gap`, () => {
    const container = draw(`vertical`)
    const path = container.querySelector(`[data-testid="g-edge"]`)!
    expect(path.getAttribute(`d`)).toBe(`M 50 20 C 50 32, 162 32, 162 44`)
  })

  it(`bows a backward vertical edge by max(waveGap / 2, |dy| / 2)`, () => {
    const container = render(
      <WaveGraph
        nodes={nodes}
        edges={[{ from: `b`, to: `a`, style: `cycle` }]}
        {...metrics}
        orientation="vertical"
        idPrefix="g"
        renderNode={(id) => id}
      />
    ).container
    const path = container.querySelector(`[data-testid="g-cycle-edge"]`)!
    // start (162, 64), end (50, 0): dy = -64 → bend 32.
    expect(path.getAttribute(`d`)).toBe(`M 162 64 C 162 96, 50 -32, 50 0`)
  })
})
