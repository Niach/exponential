/* The inline scheme script in index.html runs under the site's CSP only
   because public/serve.json allows its exact hash, and it must read the key
   src/lib/scheme.ts writes. */
import { expect, test } from "bun:test"
import { createHash } from "node:crypto"

const read = (rel: string) => Bun.file(new URL(rel, import.meta.url)).text()

test(`the CSP allows the inline scheme script by hash`, async () => {
  const html = await read(`../../index.html`)
  const scripts = [...html.matchAll(/<script>([\s\S]*?)<\/script>/g)].map((m) => m[1]!)
  expect(scripts.length).toBe(1)
  const hash = createHash(`sha256`).update(scripts[0]!).digest(`base64`)
  const serve = JSON.parse(await read(`../../public/serve.json`)) as { headers: { headers: { key: string; value: string }[] }[] }
  const csp = serve.headers.flatMap((h) => h.headers).find((h) => h.key === `Content-Security-Policy`)!.value
  expect(csp).toContain(`'sha256-${hash}'`)
  const source = await read(`./scheme.ts`)
  const key = source.match(/const KEY = `([^`]+)`/)![1]!
  expect(scripts[0]).toContain(`"${key}"`)
  // The browser bar colour: the script and setScheme agree.
  const { THEME_COLOR } = await import(`./scheme`)
  expect(scripts[0]).toContain(`s==="light"?"${THEME_COLOR.light}":"${THEME_COLOR.dark}"`)
  expect(html).toContain(`<meta name="theme-color" content="${THEME_COLOR.dark}" />`)
})

/* The built pages and the built serve.json agree (no Vite or prerender
   transform changed a script); skipped until `bun run build` wrote dist/. */
const dist = new URL(`../../dist/`, import.meta.url)
const built = await Bun.file(new URL(`index.html`, dist)).exists()
test.skipIf(!built)(`every built page's inline scripts are in the built CSP`, async () => {
  const serve = JSON.parse(await Bun.file(new URL(`serve.json`, dist)).text()) as { headers: { headers: { key: string; value: string }[] }[] }
  const csp = serve.headers.flatMap((h) => h.headers).find((h) => h.key === `Content-Security-Policy`)!.value
  const pages = [...new Bun.Glob(`**/index.html`).scanSync({ cwd: dist.pathname })]
  expect(pages.length).toBeGreaterThan(1)
  for (const page of pages) {
    const html = await Bun.file(new URL(page, dist)).text()
    // Inline, executable scripts only (no src, no JSON data blocks).
    const scripts = [...html.matchAll(/<script([^>]*)>([\s\S]*?)<\/script>/g)].filter((m) => !/\bsrc=|json/.test(m[1]!) && m[2]!.trim()).map((m) => m[2]!)
    for (const script of scripts) expect(csp, page).toContain(`'sha256-${createHash(`sha256`).update(script).digest(`base64`)}'`)
  }
})
