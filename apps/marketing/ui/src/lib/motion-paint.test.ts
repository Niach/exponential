/* Motion paints on the BUILT site under its real CSP (public/serve.json).
   The web Motion painter instantiates the engine as WebAssembly, which a
   CSP without 'wasm-unsafe-eval' refuses: every Motion on exponential.at
   then stayed blank (an empty canvas, data-state="error") while dev, which
   sends no CSP, looked fine. This serves dist/ with the built serve.json
   headers, opens the Motion pages in Chromium and asserts every player
   reached `ready` and painted pixels. Skipped until `bun run build` wrote
   dist/ (release-suite's marketing job builds first). */
import { expect, test } from "bun:test"
import { existsSync } from "node:fs"
import { join } from "node:path"

const CSP = `Content-Security-Policy`
const read = (rel: string) => Bun.file(new URL(rel, import.meta.url)).text()
const cspOf = (serve: { headers: { headers: { key: string; value: string }[] }[] }) => serve.headers.flatMap((h) => h.headers).find((h) => h.key === CSP)!.value

test(`the CSP lets the Motion engine (WebAssembly) compile`, async () => {
  const csp = cspOf(JSON.parse(await read(`../../../public/serve.json`)))
  const scriptSrc = csp.split(`;`).map((d) => d.trim()).find((d) => d.startsWith(`script-src `))!
  expect(scriptSrc).toContain(`'wasm-unsafe-eval'`)
  // Never the broad escape hatch.
  expect(scriptSrc).not.toContain(`'unsafe-eval'`)
})

const dist = new URL(`../../../dist/`, import.meta.url).pathname
const built = existsSync(join(dist, `ui/guides/motion/index.html`))

test.skipIf(!built)(`every Motion on the built Motion pages paints under the CSP`, async () => {
  const csp = cspOf(JSON.parse(await Bun.file(join(dist, `serve.json`)).text()))
  const server = Bun.serve({
    port: 0,
    async fetch(req) {
      const path = decodeURIComponent(new URL(req.url).pathname)
      const file = Bun.file(join(dist, path.endsWith(`/`) ? `${path}index.html` : path))
      if (!(await file.exists())) return new Response(`not found`, { status: 404 })
      return new Response(file, { headers: { [CSP]: csp } })
    },
  })
  const { chromium } = await import(`playwright`)
  const browser = await chromium.launch()
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } })
    const errors: string[] = []
    page.on(`console`, (m) => {
      if (m.type() === `error` && /Content Security Policy|WebAssembly/i.test(m.text())) errors.push(m.text())
    })
    for (const [path, min] of [[`/ui/guides/motion/`, 12], [`/ui/components/motion/`, 1]] as const) {
      await page.goto(`http://localhost:${server.port}${path}`, { waitUntil: `networkidle` })
      // Surfaces mount near the viewport: walk the page once.
      await page.evaluate(async () => {
        for (let y = 0; y < document.body.scrollHeight; y += 500) {
          window.scrollTo(0, y)
          await new Promise((r) => setTimeout(r, 60))
        }
      })
      await page.waitForFunction((n) => [...document.querySelectorAll(`[data-state]`)].filter((e) => e.querySelector(`:scope > canvas`)).length >= n, min, { timeout: 20_000 })
      // Every player settles (ready or error) once the engine loaded.
      await page.waitForFunction(() => [...document.querySelectorAll(`[data-state]`)].filter((e) => e.querySelector(`:scope > canvas`)).every((e) => (e as HTMLElement).dataset.state !== `loading`), undefined, { timeout: 20_000 })
      const players = await page.evaluate(async () => {
        const out: { label: string; state: string; painted: number }[] = []
        for (const el of [...document.querySelectorAll(`[data-state]`)].filter((e) => e.querySelector(`:scope > canvas`))) {
          el.scrollIntoView()
          await new Promise((r) => setTimeout(r, 150))
          const canvas = el.querySelector(`:scope > canvas`) as HTMLCanvasElement
          const px = canvas.getContext(`2d`)!.getImageData(0, 0, canvas.width, canvas.height).data
          let painted = 0
          for (let i = 3; i < px.length; i += 4) if (px[i]! > 0) painted++
          out.push({ label: el.getAttribute(`aria-label`) ?? ``, state: (el as HTMLElement).dataset.state ?? ``, painted })
        }
        return out
      })
      expect(players.length, path).toBeGreaterThanOrEqual(min)
      for (const p of players) {
        expect(p.state, `${path} ${p.label}`).toBe(`ready`)
        expect(p.painted, `${path} ${p.label}`).toBeGreaterThan(0)
      }
    }
    // The gallery's eight model-written examples, by name (still on the guide page).
    await page.goto(`http://localhost:${server.port}/ui/guides/motion/#examples`, { waitUntil: `networkidle` })
    await page.locator(`#examples`).scrollIntoViewIfNeeded()
    for (const label of [`Live dot`, `Progress ring`, `Typing`, `Heartbeat`, `Equalizer`, `Error`, `Sun`, `Wi-Fi`]) {
      await page.locator(`#examples [aria-label="${label}"][data-state="ready"]`).waitFor({ timeout: 20_000 })
    }
    expect(errors).toEqual([])
  } finally {
    await browser.close()
    server.stop(true)
  }
}, 90_000)
