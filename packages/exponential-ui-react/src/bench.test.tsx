// Round 2 (docs/round-2-contract.md §5, fixtures/bench-list.json): the
// 100,000-row List bench in jsdom — `XUI_BENCH=1 bunx vitest run
// src/bench.test.tsx` prints the numbers (skipped otherwise; the Chromium
// half is scripts/bench-list.ts). jsdom has no layout: the list windows
// against the window viewport, and a scroll step moves the list's reported
// box and fires `scroll` (the work measured is the React side: window, bind,
// render, commit).

import { describe, expect, it } from "vitest"
import { act, render } from "@testing-library/react"
import { createRef } from "react"
import { reduceSurface } from "@exponential-at/ui"
import type { FlatComponent } from "@exponential-at/ui"
import bench from "@exponential-at/ui/fixtures/bench-list.json"
import { ExponentialSurface, type SurfaceHandle } from "./surface"

const run = process.env.XUI_BENCH === `1`

describe.skipIf(!run)(`bench: 100,000 rows (jsdom)`, () => {
  it(`firstPaintMs, scrollStepMs, scrollToIndexMs, renderedItems`, () => {
    Object.defineProperty(window, `innerHeight`, { configurable: true, value: bench.viewport.height })
    let scroll = 0
    const prevScrollBy = window.scrollBy
    window.scrollBy = ((o: ScrollToOptions) => void (scroll += o.top ?? 0)) as typeof window.scrollBy
    const rect = () => ({ x: 0, y: -scroll, left: 0, top: -scroll, right: bench.viewport.width, bottom: 0, width: bench.viewport.width, height: 0, toJSON() {} }) as DOMRect
    const prevRect = Element.prototype.getBoundingClientRect
    Element.prototype.getBoundingClientRect = function (this: Element) {
      return this.classList.contains(`xui-list-window`) ? rect() : prevRect.call(this)
    }
    try {
      const t0 = performance.now()
      const rows = Array.from({ length: bench.rows.count }, (_, i) => ({ id: `r${i}`, title: `Row ${i}`, meta: String(i % 97) }))
      const r = reduceSurface(bench.components as unknown as FlatComponent[], { catalogId: bench.catalogId })
      const handle = createRef<SurfaceHandle>()
      const { container } = render(<ExponentialSurface root={r.root} templates={r.templates} data={{ rows }} theme={bench.theme} mode="light" width={bench.viewport.width} handleRef={handle} />)
      const firstPaintMs = performance.now() - t0
      const steps: number[] = []
      for (let i = 0; i < bench.steps.scroll.count; i++) {
        const t = performance.now()
        scroll += bench.viewport.height
        act(() => void window.dispatchEvent(new Event(`scroll`)))
        steps.push(performance.now() - t)
      }
      scroll = 0
      act(() => void window.dispatchEvent(new Event(`scroll`)))
      const t = performance.now()
      act(() => handle.current!.run({ scrollToIndex: { id: `root`, index: bench.steps.scrollToIndex.index, align: `start` } }))
      act(() => void window.dispatchEvent(new Event(`scroll`)))
      const scrollToIndexMs = performance.now() - t
      const renderedItems = container.querySelectorAll(`.xui-list-item`).length
      const round = (v: number) => Math.round(v * 10) / 10
      console.log(JSON.stringify({ renderer: `react-jsdom`, rows: bench.rows.count, firstPaintMs: round(firstPaintMs), scrollStepMs: round(steps.reduce((a, b) => a + b, 0) / steps.length), scrollToIndexMs: round(scrollToIndexMs), renderedItems }))
      expect(container.querySelector(`[data-xui-id="bench-row.r${bench.steps.scrollToIndex.index}"]`)).not.toBeNull()
      expect(renderedItems).toBeLessThan(60)
    } finally {
      window.scrollBy = prevScrollBy
      Element.prototype.getBoundingClientRect = prevRect
    }
  })
})
