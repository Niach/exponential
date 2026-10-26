// NEW-77: THE CHILD RULE (catalog/style.json `voiceWave`) measured on the web component:
// the native (`?view=mount`, an `<exponential-surface>`) draws each `child` case of fixtures/voice-wave.json centred, scaled by
// voiceWaveChildBox (never up, nothing at sm), within 0.5 px of the
// fixture (exponential-ui/conformance/voice-wave-child.ts).

import { afterAll, beforeAll, expect, it } from "vitest"
import { join } from "node:path"
import type { Browser } from "playwright"
import { bunHarness, launchChromium, openPage, type RunningHarness } from "../../exponential-ui/conformance/browser-page"
import { measureNativeVoiceWaveChildren, voiceWaveChildCases } from "../../exponential-ui/conformance/voice-wave-child"

let served: RunningHarness
let browser: Browser
beforeAll(async () => {
  served = await bunHarness(join(import.meta.dirname, `..`, `harness`, `serve.ts`)).start()
  browser = await launchChromium()
})
afterAll(async () => {
  await browser?.close()
  await served?.stop()
})

const allPass = () => voiceWaveChildCases().map((c) => ({ name: c.name, verdict: true }))

it(`the fixture carries the child cases`, () => expect(voiceWaveChildCases().length).toBeGreaterThanOrEqual(4))

it(`the native draws each child case within 0.5 px`, async () => {
  const page = await openPage(browser, served.baseUrl, { view: `mount` }, { width: 1000, height: 600 }, { waitForSurface: false })
  const got = await measureNativeVoiceWaveChildren(page)
  await page.context().close()
  expect(got).toEqual(allPass())
})
