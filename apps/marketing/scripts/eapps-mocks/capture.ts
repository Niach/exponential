/* Renders the eApps concept pictures (mocks.ts) with the eUI React harness
   (`?view=tree`, dark) at 2×, encodes webp and writes them to
   apps/marketing/public/eapps/ and docs/spec/apps/.
   `bun apps/marketing/scripts/eapps-mocks/capture.ts [--only <file>]` */
import { resolve } from "node:path"
import sharp from "sharp"
import { chromium } from "playwright"
import { MOCKS } from "./mocks"

const ROOT = resolve(import.meta.dirname, `../../../..`)
const OUT = [resolve(ROOT, `apps/marketing/public/eapps`), resolve(ROOT, `docs/spec/apps`)]
const only = process.argv.includes(`--only`) ? process.argv[process.argv.indexOf(`--only`) + 1] : undefined
const port = 4190 + Math.floor(Math.random() * 500)

const harness = Bun.spawn([`bun`, `./harness/serve.ts`], { cwd: resolve(ROOT, `packages/exponential-ui-react`), env: { ...process.env, PORT: String(port) }, stdout: `ignore`, stderr: `inherit` })
try {
  for (let i = 0; i < 100; i++) {
    if (await fetch(`http://localhost:${port}/`).then((r) => r.ok, () => false)) break
    await Bun.sleep(100)
  }
  const browser = await chromium.launch()
  for (const mock of MOCKS.filter((m) => !only || m.file === only)) {
    const page = await browser.newPage({ viewport: { width: mock.width, height: 900 }, deviceScaleFactor: 2 })
    const url = `http://localhost:${port}/?view=tree&mode=dark&width=${mock.width}&tree=${encodeURIComponent(JSON.stringify(mock.tree))}`
    await page.goto(url, { waitUntil: `networkidle` })
    await page.evaluate(() => document.fonts.ready)
    await page.waitForTimeout(600)
    const png = await page.locator(`#app > *`).first().screenshot()
    const webp = await sharp(png).webp({ quality: 82 }).toBuffer()
    for (const dir of OUT) await Bun.write(`${dir}/${mock.file}.webp`, webp)
    const meta = await sharp(png).metadata()
    console.log(`${mock.file}.webp ${meta.width}×${meta.height}`)
    await page.close()
  }
  await browser.close()
} finally {
  harness.kill()
}
