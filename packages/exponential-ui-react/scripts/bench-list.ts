#!/usr/bin/env bun
// Round 2 (docs/round-2-contract.md §5, fixtures/bench-list.json): the
// 100,000-row List bench in headless Chromium — `bun run bench:list`.
// Opens the harness's `bench` view (the fixture's surface, 390 × 800,
// neutral), then reports firstPaintMs (reduce + bind + layout + paint of the
// first window, to the frame after the commit), scrollStepMs (mean of 100
// one-viewport steps: the scroll, the window's render flushed synchronously
// and the layout read back — the main-thread work, not the frame wait),
// scrollToIndexMs (index 50000, align start, measured the same way) and
// renderedItems (items alive after the jump). The jsdom half: `XUI_BENCH=1 vitest run src/bench.test.tsx`.

import { chromium } from "playwright"
import bench from "@exponential-at/ui/fixtures/bench-list.json"
import { bundleHarness } from "../harness/serve"

const js = await bundleHarness()
const PAGE = `<!doctype html><html lang="en"><head><meta charset="utf-8"></head><body><div id="app"></div><script type="module" src="/dist/main.js"></script></body></html>`
const server = Bun.serve({
  port: 0,
  fetch(req) {
    const url = new URL(req.url)
    if (url.pathname === `/dist/main.js`) return new Response(js, { headers: { "content-type": `text/javascript` } })
    return new Response(PAGE, { headers: { "content-type": `text/html` } })
  },
})
const browser = await chromium.launch()
try {
  const page = await browser.newPage({ viewport: { width: bench.viewport.width, height: bench.viewport.height } })
  await page.goto(`http://localhost:${server.port}/?view=bench&theme=${bench.theme}&mode=light`)
  await page.waitForFunction(() => window.__xuiBench?.firstPaintMs !== undefined, undefined, { timeout: 120_000 })
  const result = await page.evaluate(
    async ({ steps, index }) => {
      const frame = () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))
      const b = window.__xuiBench!
      const list = document.querySelector<HTMLElement>(`[data-xui-id="root"]`)!
      // One step = the scroll, the window's render flushed, the layout read.
      const settle = () => {
        b.flush(() => list.dispatchEvent(new Event(`scroll`)))
        void list.querySelector(`.xui-list-window`)!.getBoundingClientRect()
      }
      const times: number[] = []
      for (let i = 0; i < steps; i++) {
        const t = performance.now()
        list.scrollTop += list.clientHeight
        settle()
        times.push(performance.now() - t)
        await frame()
      }
      list.scrollTop = 0
      settle()
      await frame()
      const t = performance.now()
      b.flush(() => b.handle()!.run({ scrollToIndex: { id: `root`, index, align: `start` } }))
      settle()
      const jump = performance.now() - t
      await frame()
      const target = document.querySelector(`[data-xui-id="bench-row.r${index}"]`)
      return {
        firstPaintMs: window.__xuiBench!.firstPaintMs!,
        scrollStepMs: times.reduce((a, b) => a + b, 0) / times.length,
        scrollToIndexMs: jump,
        renderedItems: list.querySelectorAll(`.xui-list-item`).length,
        targetRendered: target !== null,
      }
    },
    { steps: bench.steps.scroll.count, index: bench.steps.scrollToIndex.index }
  )
  const round = (v: number) => Math.round(v * 10) / 10
  console.log(JSON.stringify({ renderer: `react-chromium`, rows: bench.rows.count, firstPaintMs: round(result.firstPaintMs), scrollStepMs: round(result.scrollStepMs), scrollToIndexMs: round(result.scrollToIndexMs), renderedItems: result.renderedItems, targetRendered: result.targetRendered }))
} finally {
  await browser.close()
  server.stop()
}
