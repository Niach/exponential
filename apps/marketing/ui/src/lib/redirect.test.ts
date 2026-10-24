import { expect, test } from "bun:test"
import { redirectTarget } from "../../scripts/redirect"

test(`ui.exponential.at keeps the path and query under exponential.at/ui/`, () => {
  expect(redirectTarget(`https://ui.exponential.at/`)).toBe(`https://exponential.at/ui/`)
  expect(redirectTarget(`https://ui.exponential.at/components/button/`)).toBe(`https://exponential.at/ui/components/button/`)
  expect(redirectTarget(`https://ui.exponential.at/components/?q=date`)).toBe(`https://exponential.at/ui/components/?q=date`)
})
