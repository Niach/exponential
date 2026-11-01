// EXP-1604: print the web Motion painter's pixel parity per case
// (conformance/motion-parity.ts); exits non-zero when a case is under 99 %.
import { launchChromium } from "../conformance/browser-page"
import { motionParityFloor, runMotionParity } from "../conformance/motion-parity"

const browser = await launchChromium(process.env.MOTION_PARITY_ARGS ? process.env.MOTION_PARITY_ARGS.split(` `) : [])
const page = await browser.newPage()
await page.setContent(`<!doctype html><html><body></body></html>`)
const results = await runMotionParity(page)
await browser.close()
let failed = 0
for (const r of results) {
  const floor = motionParityFloor(r.name)
  const ok = r.share >= floor
  if (!ok) failed++
  console.log(`${ok ? `ok  ` : `FAIL`} ${r.name.padEnd(36)} ${(r.share * 100).toFixed(2).padStart(6)} %${floor < 0.99 ? `  (known gap, floor ${(floor * 100).toFixed(1)} %)` : ``}  max ${r.maxDiff}`)
}
console.log(`${results.length - failed}/${results.length} cases at or above their floor (≤ 2/255 per channel on ≥ 99 % of pixels unless a known gap)`)
process.exit(failed ? 1 : 0)
