// VAPP-4 spike: CSS geometry (fixed fake measure) vs the taffy core's frames.
// Prints every node beyond ±1px and a summary line per case.
import { readFileSync } from "node:fs"
import { resolve } from "node:path"
import { launch, openSignedIn, openSurface } from "./common"

type Frame = { id: string; x: number; y: number; w: number; h: number }
const SPIKE = resolve(import.meta.dir, `../../../../spikes/vapp-layout`)
const TOL = 1

const cases = [
  { name: `900`, file: `frames-900.json`, query: `?geometry=1&width=900` },
  { name: `390`, file: `frames-390.json`, query: `?geometry=1&width=390` },
  { name: `900-rtl`, file: `frames-900-rtl.json`, query: `?geometry=1&width=900&rtl=1` },
  { name: `390-rtl`, file: `frames-390-rtl.json`, query: `?geometry=1&width=390&rtl=1` },
]

const fmt = (n: number) => (Math.round(n * 100) / 100).toString()

const browser = await launch()
const summary: Array<string> = []
try {
  const page = await openSignedIn(browser, { width: 1200, height: 900 })
  for (const c of cases) {
    const taffy = JSON.parse(readFileSync(resolve(SPIKE, c.file), `utf8`)) as {
      nodes: Array<Frame>
    }
    await openSurface(page, c.query)
    // ABLATE_CSS = extra CSS injected after the surface stylesheet, to measure
    // what each taffy-alignment rule in style-to-css.ts is worth.
    if (process.env.ABLATE_CSS) await page.addStyleTag({ content: process.env.ABLATE_CSS })
    const browserFrames = (await page.evaluate(() => window.__vappFrames!())) as Array<Frame>
    const byId = new Map(browserFrames.map((f) => [f.id, f]))
    const rows: Array<string> = []
    let within = 0
    const all: Array<[number, string]> = []
    for (const t of taffy.nodes) {
      const b = byId.get(t.id)
      if (!b) {
        rows.push(`| ${t.id} | (missing) | | | |`)
        continue
      }
      let ok = true
      for (const axis of [`x`, `y`, `w`, `h`] as const) {
        const d = b[axis] - t[axis]
        all.push([Math.abs(d), `${t.id}.${axis} taffy=${fmt(t[axis])} browser=${fmt(b[axis])} delta=${d.toExponential(2)}`])
        if (Math.abs(d) > TOL) {
          ok = false
          rows.push(`| ${t.id} | ${axis} | ${fmt(t[axis])} | ${fmt(b[axis])} | ${fmt(d)} |`)
        }
      }
      if (ok) within++
    }
    console.log(`\n### width ${c.name}\n`)
    if (rows.length) {
      console.log(`| id | axis | taffy | browser | delta |\n|---|---|---|---|---|`)
      console.log(rows.join(`\n`))
    }
    all.sort((a, b) => b[0] - a[0])
    console.log(`top 5 |delta|:\n  ${all.slice(0, 5).map(([, l]) => l).join(`\n  `)}`)
    const line = `${c.name}: ${within}/${taffy.nodes.length} nodes within ±${TOL}px (browser frames: ${browserFrames.length}, max |delta| ${all[0]?.[0].toExponential(2)})`
    console.log(`\n${line}`)
    summary.push(line)
  }
} finally {
  await browser.close()
}
console.log(`\n${summary.join(`\n`)}`)
