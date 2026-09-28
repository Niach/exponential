// VAPP-3 spike: drive the bench page in Playwright (chromium headless shell + webkit).
//   bun spike/vapp-3/web/run.ts --runs 20 --policy all|relay --browsers chromium,webkit --scenario same-machine
//   extra: --tamper sdp|sig|control|answer  --daemon-policy all|relay  --notes "..."
// Needs `bun spike/vapp-3/web/serve.ts` running and a daemon in the room.
import { chromium, webkit } from "/Users/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-3/node_modules/playwright/index.mjs"

const args = process.argv.slice(2)
const arg = (k: string, d: string) => { const i = args.indexOf(`--${k}`); return i >= 0 ? args[i + 1] : d }
const runs = arg("runs", "20")
const policy = arg("policy", "all")
const scenario = arg("scenario", "same-machine")
const browsers = arg("browsers", "chromium,webkit").split(",")
const tamper = arg("tamper", "")
const daemonPolicy = arg("daemon-policy", "all")
const notes = arg("notes", "")
const base = arg("url", "http://localhost:8787/")

for (const name of browsers) {
  const source = name === "chromium" ? "web-chromium" : "web-webkit"
  const q = new URLSearchParams({ policy, runs, source, scenario, auto: "1", daemonPolicy, notes })
  if (tamper) q.set("tamper", tamper)
  const url = `${base}?${q}`
  const type = name === "chromium" ? chromium : webkit
  const browser = await type.launch({ headless: true })
  const page = await browser.newPage()
  page.on("console", (m) => { if (process.env.VERBOSE) console.log(`[${name}] ${m.text()}`) })
  page.on("pageerror", (e) => console.log(`[${name}] pageerror ${e.message}`))
  const t0 = Date.now()
  await page.goto(url)
  await page.waitForFunction(() => document.title === "done", undefined, { timeout: 10 * 60_000, polling: 500 })
  const res = await page.evaluate(() => (window as any).__vapp3)
  await browser.close()
  const rows = res.rows as any[]
  const bench = rows.filter((r) => r.kind === "bench")
  const ok = bench.filter((r) => r.ok)
  const pct = (xs: number[], p: number) => { const s = [...xs].sort((a, b) => a - b); return s[Math.floor(p * (s.length - 1) + 0.5)] }
  const f = (x: number) => (x == null || Number.isNaN(x) ? "–" : x.toFixed(2))
  console.log(`\n${source} policy=${policy} daemon=${daemonPolicy} ${tamper ? `tamper=${tamper} ` : ""}(${((Date.now() - t0) / 1000).toFixed(0)} s) errors=${JSON.stringify(res.errors)}`)
  if (bench.length) {
    console.log(`  ok ${ok.length}/${bench.length}  connect p50 ${f(pct(ok.map((r) => r.connectMs), 0.5))} p95 ${f(pct(ok.map((r) => r.connectMs), 0.95))}  rtt p50 ${f(pct(ok.map((r) => r.rttP50Ms), 0.5))}  up ${f(pct(ok.map((r) => r.upMbps), 0.5))}  down ${f(pct(ok.map((r) => r.downMbps), 0.5))} Mbps`)
    console.log(`  paths: ${[...new Set(ok.map((r) => `${r.localPath}↔${r.remotePath}`))].join(", ")}  daemon view: ${[...new Set(ok.map((r) => `${r.daemonView?.localPath}↔${r.daemonView?.remotePath}`))].join(", ")}`)
    for (const r of bench.filter((r) => !r.ok)) console.log(`  FAIL ${r.error} ${r.notes}`)
  }
  for (const r of rows.filter((r) => r.kind === "tamper")) console.log(`  tamper ${r.variant}: rejected=${r.rejected} ${r.reason}`)
}
console.log(`\nSafari (manual, once): open ${base}?${new URLSearchParams({ policy, runs, source: "web-safari", scenario, auto: "1" })}`)
