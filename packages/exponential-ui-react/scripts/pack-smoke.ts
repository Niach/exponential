#!/usr/bin/env bun
// VAPP-87 acceptance: a PLAIN Vite app OUTSIDE the monorepo, installed from
// the packed tarballs of `@exponential-at/ui` and `@exponential-at/ui-react`,
// renders the kitchen sink. Packs both packages, writes a throwaway app
// under a temp dir, `bun install`s it (React + Vite from the registry or the
// local cache), builds it, serves `dist/` and checks the DOM in Chromium.
//
//   bun run --filter @exponential-at/ui-react smoke:pack [--keep]

import { chromium } from "playwright"
import { execSync } from "node:child_process"
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"

const pkgRoot = join(import.meta.dirname, `..`)
const repoRoot = join(pkgRoot, `..`, `..`)
const keep = process.argv.includes(`--keep`)
const version = (name: string) => (JSON.parse(readFileSync(join(repoRoot, `node_modules`, name, `package.json`), `utf8`)) as { version: string }).version

const dir = mkdtempSync(join(tmpdir(), `xui-smoke-`))
console.log(`smoke app: ${dir}`)
const run = (cmd: string, cwd: string) => {
  console.log(`$ ${cmd}  (${cwd})`)
  execSync(cmd, { cwd, stdio: `inherit`, env: { ...process.env, CI: `1` } })
}

// 1. Pack.
run(`bun pm pack --destination ${dir}`, join(repoRoot, `packages/exponential-ui`))
run(`bun pm pack --destination ${dir}`, join(repoRoot, `packages/exponential-ui-react`))
const tarballs = readdirSync(dir).filter((f) => f.endsWith(`.tgz`))
const ui = tarballs.find((f) => f.startsWith(`exponential-at-ui-`) && !f.includes(`ui-react`))
const uiReact = tarballs.find((f) => f.includes(`ui-react`))
if (!ui || !uiReact) throw new Error(`tarballs missing: ${tarballs.join(`, `)}`)

// 2. The app.
const app = join(dir, `app`)
mkdirSync(join(app, `src`), { recursive: true })
writeFileSync(
  join(app, `package.json`),
  JSON.stringify(
    {
      name: `xui-smoke`,
      private: true,
      type: `module`,
      scripts: { build: `vite build` },
      dependencies: {
        "@exponential-at/ui": `file:../${ui}`,
        "@exponential-at/ui-react": `file:../${uiReact}`,
        react: version(`react`),
        "react-dom": version(`react-dom`),
      },
      devDependencies: {
        "@vitejs/plugin-react": version(`@vitejs/plugin-react`),
        vite: version(`vite`),
      },
      // The renderer's own dependency on the catalog package is a workspace
      // link here and a registry version once VAPP-91 publishes; until then
      // the tarball stands in for the registry.
      overrides: { "@exponential-at/ui": `file:../${ui}` },
    },
    null,
    2
  )
)
writeFileSync(join(app, `vite.config.js`), `import react from "@vitejs/plugin-react"\nexport default { plugins: [react()], base: "./" }\n`)
writeFileSync(join(app, `index.html`), `<!doctype html><html><head><meta charset="utf-8"><title>smoke</title></head><body><div id="root"></div><script type="module" src="/src/main.jsx"></script></body></html>`)
writeFileSync(
  join(app, `src/main.jsx`),
  `import { createRoot } from "react-dom/client"
import { CORE_CATALOG_ID, reduceNested } from "@exponential-at/ui"
import kitchenSink from "@exponential-at/ui/fixtures/kitchen-sink.json"
import { ExponentialSurface } from "@exponential-at/ui-react"
const { root } = reduceNested(kitchenSink, { catalogId: CORE_CATALOG_ID })
createRoot(document.getElementById("root")).render(
  <div style={{ display: "grid", gap: 16 }}>
    <ExponentialSurface id="a" root={root} theme="exponential" mode="dark" />
    <ExponentialSurface id="b" root={root} theme="playful" mode="light" />
  </div>
)
`
)
run(`bun install`, app)
run(`bun run build`, app)

// 3. Serve dist and look.
const dist = join(app, `dist`)
const server = Bun.serve({
  port: 0,
  fetch(req) {
    const path = new URL(req.url).pathname
    const file = Bun.file(join(dist, path === `/` ? `index.html` : path))
    return new Response(file)
  },
})
const browser = await chromium.launch()
try {
  const page = await browser.newPage({ viewport: { width: 1200, height: 900 } })
  const errors: string[] = []
  page.on(`pageerror`, (e) => errors.push(String(e)))
  await page.goto(`http://127.0.0.1:${server.port}/`)
  await page.waitForSelector(`[data-xui-surface="b"] [data-xui-id="root"]`, { timeout: 30_000 })
  const nodes = await page.locator(`[data-xui-surface="a"] [data-xui-id]`).count()
  const unknown = await page.locator(`[data-xui-c="Unknown"]`).count()
  const bg = await page.locator(`[data-xui-surface="b"]`).evaluate((el) => getComputedStyle(el).getPropertyValue(`--xui-color-primary`).trim())
  await page.screenshot({ path: join(dir, `smoke.png`), fullPage: false })
  console.log(`nodes ${nodes}, unknown ${unknown}, playful primary ${bg}, errors ${errors.length}`)
  if (errors.length) throw new Error(errors.join(`\n`))
  if (nodes < 100 || unknown !== 0 || !bg.startsWith(`#`)) throw new Error(`the kitchen sink did not render`)
  console.log(`OK — screenshot ${join(dir, `smoke.png`)}`)
} finally {
  await browser.close()
  server.stop(true)
  if (!keep) rmSync(dir, { recursive: true, force: true })
}
