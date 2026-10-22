#!/usr/bin/env bun
// VAPP-105: the React runner. Every scenario of scenarios/manifest.json in
// headless Chromium through the React harness's `bench-suite` view
// (packages/exponential-ui-react/harness/main.tsx, `window.__xuiBenchSuite`).
// A pass = the change applied inside flushSync and the layout read back on
// the main thread (as scripts/bench-list.ts); a frame is awaited BETWEEN
// passes, never inside a measurement. Each repeat is a fresh page; the
// fragment holds the MEDIAN per metric over the repeats.
//
//   bun run --filter @exponential-at/ui-bench bench:react [--scenario <id>]… [--repeat <n>]
//
// Writes `{ renderer: "react-chromium", platform: "web", … }` to
// $EXPONENTIAL_UI_BENCH_OUT (default .bench/react-chromium.json).

import { chromium } from "playwright"
import { bundleHarness } from "../../exponential-ui-react/harness/serve"
import manifest from "../scenarios/manifest.json"
import { generateItems, SCENARIOS } from "../src/scenarios"
import { round, writeFragment, type Metrics } from "../src/results"

/** The harness view's driver surface (packages/exponential-ui-react/harness/main.tsx `BenchSuiteApi`). */
interface BenchSuiteApi {
  ready: boolean
  firstPaintMs?: number
  coldStartMs?: number
  flush: (fn: () => void) => void
  apply: (message: unknown) => void
  setData: (path: string, value: unknown) => void
  setTheme: (id: string) => void
  setMode: (mode: `light` | `dark`) => void
  setWidth: (px: number) => void
  scrollToIndex: (id: string, index: number, align?: `start` | `center` | `end` | `nearest`) => boolean
  scroll: (id: string, by: number) => number
  read: () => number
  nodes: () => number
  depth: () => number
  renderedItems: (selector: string) => number
  has: (selector: string) => boolean
}
declare global {
  interface Window {
    __xuiBenchSuite?: BenchSuiteApi
  }
}

const args = process.argv.slice(2)
const only = args.flatMap((a, i) => (a === `--scenario` ? [args[i + 1]] : []))
const repeatAt = args.indexOf(`--repeat`)
const repeats = repeatAt >= 0 ? Number(args[repeatAt + 1]) : 3
const ids = manifest.scenarios.map((s) => s.id).filter((id) => only.length === 0 || only.includes(id))

const median = (v: number[]) => {
  const s = [...v].sort((a, b) => a - b)
  return s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2
}

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
const browser = await chromium.launch({ args: [`--enable-precise-memory-info`] })

/** Drive one scenario on a fresh page; the page-side script measures. */
async function runOnce(id: string): Promise<Metrics> {
  const def = SCENARIOS.find((s) => s.id === id)!
  const page = await browser.newPage({ viewport: { width: def.viewport.width, height: def.viewport.height } })
  try {
    await page.goto(`http://localhost:${server.port}/?view=bench-suite&scenario=${id}`)
    await page.waitForFunction(() => window.__xuiBenchSuite?.ready === true, undefined, { timeout: 180_000 })
    const steps = def.steps as Record<string, unknown>
    const rebind = id === `template-10k` ? generateItems(steps.rebind as Parameters<typeof generateItems>[0]) : null
    const metrics = await page.evaluate(
      async ({ id, steps, rebind }) => {
        const b = window.__xuiBenchSuite!
        const frame = () => new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r())))
        const pass = (fn: () => void) => {
          const t = performance.now()
          b.flush(fn)
          b.read()
          return performance.now() - t
        }
        const med = (v: number[]) => {
          const s = [...v].sort((a, b) => a - b)
          return s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2
        }
        const out: Record<string, number> = { firstPaintMs: b.firstPaintMs!, nodes: b.nodes() }
        const widthPasses = async (widths: number[], passes: number) => {
          const times: number[] = []
          for (let i = 0; i < passes; i++) {
            const w = widths[i % widths.length]
            times.push(pass(() => b.setWidth(w)))
            await frame()
          }
          return med(times)
        }
        switch (id) {
          case `cold-start`: {
            out.coldStartMs = b.coldStartMs!
            break
          }
          case `card-grid`:
          case `deep-nesting`: {
            out.warmPassMs = await widthPasses(steps.widths as number[], steps.passes as number)
            if (id === `deep-nesting`) out.depth = b.depth()
            break
          }
          case `resize`: {
            out.resizeMs = await widthPasses(steps.widths as number[], steps.passes as number)
            break
          }
          case `list-100k`: {
            const scroll = steps.scroll as { count: number }
            const jump = steps.scrollToIndex as { index: number; align: `start` }
            const times: number[] = []
            const vh = document.querySelector<HTMLElement>(`[data-xui-id="root"]`)!.clientHeight
            for (let i = 0; i < scroll.count; i++) {
              const t = performance.now()
              b.scroll(`root`, vh)
              times.push(performance.now() - t)
              await frame()
            }
            b.scroll(`root`, -scroll.count * vh)
            await frame()
            const t = performance.now()
            b.flush(() => b.scrollToIndex(`root`, jump.index, jump.align))
            b.scroll(`root`, 0)
            out.scrollToIndexMs = performance.now() - t
            await frame()
            out.scrollStepMs = times.reduce((a, c) => a + c, 0) / times.length
            out.renderedItems = b.renderedItems(`.xui-list-item`)
            out.targetRendered = b.has(`[data-xui-id="bench-row.r${jump.index}"]`) ? 1 : 0
            break
          }
          case `streaming`: {
            const messages = steps.messages as unknown[]
            const times: number[] = []
            const t0 = performance.now()
            for (const m of messages) {
              times.push(pass(() => b.apply(m)))
              await frame()
            }
            const sorted = [...times].sort((a, c) => a - c)
            out.applyMs = times.reduce((a, c) => a + c, 0) / times.length
            out.p95ApplyMs = sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * 0.95))]
            out.totalMs = times.reduce((a, c) => a + c, 0)
            out.wallMs = performance.now() - t0
            out.nodes = b.nodes()
            break
          }
          case `theme-switch`: {
            const themes = steps.themes as string[]
            const modes = steps.modes as (`light` | `dark`)[]
            const t: number[] = []
            for (let c = 0; c < (steps.cycles as number); c++) {
              for (const theme of themes) {
                t.push(pass(() => b.setTheme(theme)))
                await frame()
              }
            }
            const m: number[] = []
            for (let c = 0; c < (steps.modeCycles as number); c++) {
              for (const mode of modes) {
                m.push(pass(() => b.setMode(mode)))
                await frame()
              }
            }
            out.themeSwitchMs = med(t)
            out.modeSwitchMs = med(m)
            break
          }
          case `template-10k`: {
            const spec = steps.rebind as { path: string }
            out.rebindMs = pass(() => b.setData(spec.path, rebind))
            await frame()
            out.nodes = b.nodes()
            break
          }
          case `overlay`: {
            const path = steps.path as string
            const opens: number[] = []
            const closes: number[] = []
            let opened = 0
            for (let i = 0; i < (steps.toggles as number); i++) {
              opens.push(pass(() => b.setData(path, true)))
              await frame()
              if (b.has(`[data-xui-overlay="Dialog"]`)) opened++
              closes.push(pass(() => b.setData(path, false)))
              await frame()
            }
            out.openMs = med(opens)
            out.closeMs = med(closes)
            out.opened = opened
            break
          }
          case `table-1k`: {
            const scroll = steps.scroll as { count: number }
            const sort = steps.sort as { updateComponents: unknown }
            const times: number[] = []
            const vh = document.querySelector<HTMLElement>(`[data-xui-id="table"]`)!.clientHeight
            for (let i = 0; i < scroll.count; i++) {
              const t = performance.now()
              b.scroll(`table`, vh)
              times.push(performance.now() - t)
              await frame()
            }
            out.scrollStepMs = times.reduce((a, c) => a + c, 0) / times.length
            out.renderedRows = b.renderedItems(`[data-xui-id="table"] [role="row"]`) - 1
            out.sortMs = pass(() => b.apply(sort))
            await frame()
            break
          }
        }
        const mem = (performance as unknown as { memory?: { usedJSHeapSize: number } }).memory
        if (mem) out.memoryMb = mem.usedJSHeapSize / 2 ** 20
        return out
      },
      { id, steps, rebind }
    )
    return metrics
  } finally {
    await page.close()
  }
}

const scenarios: Record<string, Metrics> = {}
try {
  for (const id of ids) {
    const runs: Metrics[] = []
    for (let r = 0; r < repeats; r++) runs.push(await runOnce(id))
    const def = SCENARIOS.find((s) => s.id === id)!
    const merged: Metrics = {}
    for (const key of Object.keys(runs[0])) merged[key] = round(median(runs.map((m) => m[key])))
    // Invariants the scenario names.
    if (id === `list-100k`) {
      if (merged.targetRendered !== 1) throw new Error(`list-100k: row ${(def.steps.scrollToIndex as { index: number }).index} not rendered after the jump`)
      delete merged.targetRendered
    }
    if (id === `deep-nesting` && merged.depth !== 48) throw new Error(`deep-nesting: depth ${merged.depth}, expected 48`)
    if (id === `template-10k` && merged.nodes < 10_000) throw new Error(`template-10k: ${merged.nodes} nodes, expected ≥ 10,000`)
    if (id === `overlay`) {
      if (merged.opened !== (def.steps.toggles as number)) throw new Error(`overlay: the dialog opened ${merged.opened} of ${def.steps.toggles} times`)
      delete merged.opened
    }
    if (id === `streaming`) delete merged.wallMs
    // Only the manifest's metrics go into the fragment.
    const allowed = new Set(def.metrics.map((m) => m.id))
    scenarios[id] = Object.fromEntries(Object.entries(merged).filter(([k]) => allowed.has(k)))
    console.error(`${id}: ${JSON.stringify(scenarios[id])}  (runs: ${runs.map((m) => Object.values(m).map((v) => round(v, 1)).join(`/`)).join(` | `)})`)
  }
} finally {
  await browser.close()
  server.stop()
}

const fragment = { renderer: `react-chromium`, platform: `web`, date: new Date().toISOString(), scenarios }
const path = writeFragment(fragment)
console.log(JSON.stringify(fragment))
console.error(`wrote ${path}`)
