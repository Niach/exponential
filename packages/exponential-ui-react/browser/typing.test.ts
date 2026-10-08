// VAPP-87 acceptance: zero dropped keystrokes at 150 ms simulated RTT. The
// harness host echoes every input edit into the data model one round trip
// later; 40 characters typed as fast as Playwright can must all land in the
// field AND in the host's copy, in order.

import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { openPage, startHarness, type Harness } from "./support"

const EXPECTED = `abcdefghijklmnopqrstuvwxyz0123456789ABCD`

let h: Harness
beforeAll(async () => {
  h = await startHarness()
})
afterAll(async () => h?.close())

describe(`host-owned input at 150 ms RTT`, () => {
  for (const run of [1, 2, 3]) {
    it(`run ${run}: 40 keys, none dropped, the echo settles`, async () => {
      const page = await openPage(h, { view: `kitchen-sink`, echo: 150, theme: `neutral` })
      const input = page.locator(`[data-xui-id="echo-field"] input`)
      await input.focus()
      await input.pressSequentially(EXPECTED, { delay: 5 })
      await page.waitForTimeout(450)
      expect(await input.inputValue()).toBe(EXPECTED)
      await page.locator(`[data-xui-id="echo-field"] input`).blur()
      await page.waitForTimeout(400)
      expect(((await page.locator(`#host-echo`).textContent()) ?? ``).replace(/^host: /, ``)).toBe(EXPECTED)
      const log = (await page.evaluate(() => window.__xuiLog)) as { input?: string; kind?: string; revision?: number }[]
      const inputs = log.filter((e) => e.input === `title`)
      expect(inputs.length).toBeGreaterThan(0)
      expect(inputs.some((e) => e.kind === `commit`)).toBe(true)
      await page.context().close()
    })
  }
  it(`a stale host echo never clobbers a newer keystroke`, async () => {
    const page = await openPage(h, { view: `kitchen-sink`, echo: 400, theme: `neutral` })
    const input = page.locator(`[data-xui-id="echo-field"] input`)
    await input.focus()
    await input.pressSequentially(`abc`, { delay: 5 })
    await page.waitForTimeout(200)
    await input.pressSequentially(`def`, { delay: 5 })
    await page.waitForTimeout(900)
    expect(await input.inputValue()).toBe(`abcdef`)
    await page.context().close()
  })
})
