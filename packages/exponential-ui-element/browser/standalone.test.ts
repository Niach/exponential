// FEED-130: the self-contained bundle (scripts/build-standalone.ts) on a
// page with NO bundler: `dist/standalone.js` as a plain module script, one
// `<exponential-surface theme="orange" mode="dark">` in the markup and an
// inline module feeding it A2UI messages through `el.apply`. It must paint
// (`.xui-surface[data-xui-theme="orange"]`) with no page error and fetch
// nothing but the page and the bundle (every dependency inlined). VAPP-107:
// the bundle carries the URL filter: a second element whose `src` is a
// private host builds no transport (closed, with the reason, nothing
// fetched) and a Link to a private host or a javascript: href paints as
// text; only a public https href carries.

import { afterAll, beforeAll, expect, it } from "vitest"
import { spawn, type ChildProcess } from "node:child_process"
import { join } from "node:path"
import type { Browser } from "playwright"
import { freePort, launchChromium } from "../../exponential-ui/conformance/browser-page"
import { buildStandalone } from "../scripts/build-standalone"

const dir = join(import.meta.dirname, `..`)
let bundle: string
let server: ChildProcess
let baseUrl: string
let browser: Browser

// The page + the bundle, served by a tiny Bun server (vitest runs under
// Node: Bun.serve lives in a child, its script inline).
const SERVER = `
const bundle = process.env.XUI_BUNDLE
const page = \`<!doctype html><html lang="en"><head><meta charset="utf-8"><title>standalone</title>
<script type="module" src="/standalone.js"></script></head>
<body style="margin:0;background:#0f0f11">
<exponential-surface id="s" surface-id="s1" theme="orange" mode="dark" width="400"></exponential-surface>
<exponential-surface id="lan" src="http://10.0.0.5/ui" post="http://10.0.0.5/act"></exponential-surface>
<script type="module">
  await customElements.whenDefined("exponential-surface")
  const el = document.getElementById("s")
  el.apply({ version: "v0.9", createSurface: { surfaceId: "s1", catalogId: "https://ui.exponential.at/catalogs/core/v1" } })
  el.apply({ version: "v0.9", updateComponents: { surfaceId: "s1", components: [
    { id: "root", component: "Box", children: ["hi", "go", "docs", "router", "js", "cam"] },
    { id: "hi", component: "Text", text: { path: "/name" } },
    { id: "go", component: "Button", label: "Go" },
    { id: "docs", component: "Link", label: "Docs", href: "https://ui.exponential.at/docs", external: true },
    { id: "router", component: "Link", label: "Router", href: "http://192.168.1.1/admin", external: true },
    { id: "js", component: "Link", label: "Script", href: "javascript:alert(1)" },
    { id: "cam", component: "Image", src: "http://10.0.0.9/cam.jpg", alt: "cam" },
  ] } })
  el.apply({ version: "v0.9", updateDataModel: { surfaceId: "s1", path: "/name", value: "Hello from the bundle" } })
</script></body></html>\`
Bun.serve({
  port: Number(process.env.PORT),
  hostname: "127.0.0.1",
  fetch(req) {
    const path = new URL(req.url).pathname
    if (path === "/") return new Response(page, { headers: { "content-type": "text/html; charset=utf-8" } })
    if (path === "/standalone.js") return new Response(Bun.file(bundle), { headers: { "content-type": "text/javascript; charset=utf-8" } })
    if (path === "/standalone.js.map") return new Response(Bun.file(bundle + ".map"), { headers: { "content-type": "application/json" } })
    return new Response("Not found", { status: 404 })
  },
})
console.log("ready")
`

beforeAll(async () => {
  bundle = await buildStandalone(dir)
  const port = await freePort()
  baseUrl = `http://127.0.0.1:${port}`
  server = spawn(`bun`, [`-e`, SERVER], { env: { ...process.env, PORT: String(port), XUI_BUNDLE: bundle }, stdio: [`ignore`, `pipe`, `inherit`] })
  await new Promise<void>((resolve, reject) => {
    server.stdout!.on(`data`, (d) => String(d).includes(`ready`) && resolve())
    server.on(`exit`, (code) => reject(new Error(`standalone server exited (${code})`)))
  })
  browser = await launchChromium()
}, 300_000)

afterAll(async () => {
  await browser?.close()
  server?.kill()
})

it(`dist/standalone.js paints <exponential-surface> on a page with no bundler`, async () => {
  const context = await browser.newContext({ viewport: { width: 800, height: 600 }, deviceScaleFactor: 1 })
  const page = await context.newPage()
  const errors: string[] = []
  const requests: string[] = []
  page.on(`pageerror`, (e) => errors.push(String(e)))
  page.on(`console`, (m) => m.type() === `error` && errors.push(m.text()))
  page.on(`request`, (r) => requests.push(r.url()))
  await page.goto(`${baseUrl}/`)
  await page.waitForSelector(`exponential-surface .xui-surface[data-xui-theme="orange"] [data-xui-id="go"]`, { timeout: 30_000 })
  const got = await page.evaluate(() => {
    const el = document.getElementById(`s`)!
    const surface = el.querySelector<HTMLElement>(`.xui-surface`)!
    const link = (id: string) => {
      const a = surface.querySelector<HTMLAnchorElement>(`[data-xui-id="${id}"]`)!
      return [a.textContent, a.getAttribute(`href`), a.hasAttribute(`data-denied`)]
    }
    const lan = document.getElementById(`lan`) as HTMLElement & { status?: string; statusDetail?: string }
    return {
      display: getComputedStyle(el).display,
      text: surface.textContent,
      width: surface.getBoundingClientRect().width,
      links: [link(`docs`), link(`router`), link(`js`)],
      cam: surface.querySelector(`[data-xui-id="cam"] img`)?.getAttribute(`src`) ?? null,
      lan: { status: lan.status, detail: lan.statusDetail, painted: lan.querySelector(`.xui-surface`) !== null },
    }
  })
  expect(got.display).toBe(`contents`)
  expect(got.text).toContain(`Hello from the bundle`)
  expect(got.width).toBe(400)
  // VAPP-107: the filter rides in the bundle.
  expect(got.links).toEqual([
    [`Docs`, `https://ui.exponential.at/docs`, false],
    [`Router`, null, true],
    [`Script`, null, true],
  ])
  expect(got.cam).toBeNull()
  expect(got.lan).toEqual({ status: `closed`, detail: `src denied by the URL policy (private)`, painted: false })
  expect(errors).toEqual([])
  expect(requests.map((u) => new URL(u)).filter((u) => u.origin !== baseUrl || ![`/`, `/standalone.js`, `/standalone.js.map`, `/favicon.ico`].includes(u.pathname)).map(String)).toEqual([])
  await context.close()
}, 120_000)
