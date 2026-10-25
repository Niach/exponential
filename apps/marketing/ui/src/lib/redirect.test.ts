import { expect, test } from "bun:test"
import { redirectTarget } from "../../scripts/redirect"

test(`ui.exponential.at keeps the path and query under exponential.at/ui/`, () => {
  expect(redirectTarget(`https://ui.exponential.at/`)).toBe(`https://exponential.at/ui/`)
  expect(redirectTarget(`https://ui.exponential.at/components/button/`)).toBe(`https://exponential.at/ui/components/button/`)
  expect(redirectTarget(`https://ui.exponential.at/components/?q=date`)).toBe(`https://exponential.at/ui/components/?q=date`)
  expect(redirectTarget(`https://ui.exponential.at/ui/guides/react/`)).toBe(`https://exponential.at/ui/guides/react/`)
})

test(`the old site's root files keep their root path`, () => {
  for (const path of [`/llms.txt`, `/sitemap.xml`, `/robots.txt`, `/favicon.ico`, `/og/og-ui.png`, `/shots/exponential-ui-button/ios.webp`, `/fonts/geist-latin.woff2`]) {
    expect(redirectTarget(`https://ui.exponential.at${path}`)).toBe(`https://exponential.at${path}`)
  }
})
