// VAPP-87: shared setup of the headless-Chromium suites — the harness server
// on a free port and one Playwright browser per file. The serving + page
// driving live in the shared web tooling
// (@exponential_app/ui/conformance/browser-page.ts; `EXPONENTIAL_UI_CHROME`
// = an explicit Chromium executable).

import type { Browser, Page } from "playwright"
import { join } from "node:path"
import { bunHarness, launchChromium, openPage as openHarnessPage, pageUrl, type Query } from "../../exponential-ui/conformance/browser-page"

export interface Harness {
  url: (query: Query) => string
  browser: Browser
  close: () => Promise<void>
}

export async function startHarness(): Promise<Harness> {
  const served = await bunHarness(join(import.meta.dirname, `..`, `harness`, `serve.ts`)).start()
  const browser = await launchChromium()
  return {
    browser,
    url: (query) => pageUrl(served.baseUrl, query),
    close: async () => {
      await browser.close()
      await served.stop()
    },
  }
}

export async function openPage(h: Harness, query: Query, viewport = { width: 1200, height: 900 }): Promise<Page> {
  return openHarnessPage(h.browser, new URL(h.url({})).origin, query, viewport)
}
