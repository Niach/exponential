// VAPP-4 spike: the typing test (LANES.md). Types 40 chars into the echo
// field as fast as Playwright can, waits 400 ms, asserts field + host echo.
import { launch, openSignedIn, openSurface } from "./common"

const EXPECTED = `abcdefghijklmnopqrstuvwxyz0123456789ABCD`
const RUNS = Number(process.env.RUNS ?? 3)

const browser = await launch()
let failures = 0
try {
  const page = await openSignedIn(browser, { width: 1200, height: 900 })
  for (let run = 1; run <= RUNS; run++) {
    await openSurface(page)
    const input = page.getByTestId(`vapp-echo-input`)
    await input.focus()
    const started = performance.now()
    await input.pressSequentially(EXPECTED, { delay: 5 })
    const typedMs = performance.now() - started
    await page.waitForTimeout(400)
    const value = await input.inputValue()
    const host = ((await page.getByTestId(`vapp-echo-host`).textContent()) ?? ``).replace(/^host: /, ``)
    const pass = value === EXPECTED && host === EXPECTED
    if (!pass) failures++
    console.log(
      `run ${run}: ${pass ? `PASS` : `FAIL`} typed=${typedMs.toFixed(0)}ms value=${JSON.stringify(value)} host=${JSON.stringify(host)}`
    )
  }
} finally {
  await browser.close()
}
process.exit(failures ? 1 : 0)
