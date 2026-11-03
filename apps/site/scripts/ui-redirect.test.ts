import { expect, test } from "bun:test"
import { redirectTarget } from "./ui-redirect"

test.each([
  [`https://ui.exponential.at/`, `https://exponential.at/ui/`],
  [`https://ui.exponential.at/components/button/`, `https://exponential.at/docs/libraries/eui/reference/components/button/`],
  [`https://ui.exponential.at/ui/guides/react/?x=1`, `https://exponential.at/docs/libraries/eui/guides/react/?x=1`],
  [`https://ui.exponential.at/guides/motion`, `https://exponential.at/docs/libraries/motion/guides/animate/`],
  [`https://ui.exponential.at/concepts/`, `https://exponential.at/docs/libraries/eui/`],
  [`https://ui.exponential.at/llms.txt`, `https://exponential.at/llms.txt`],
  [`https://ui.exponential.at/shots/exponential-ui-button/web.webp`, `https://exponential.at/shots/exponential-ui-button/web.webp`],
])(`%s → %s`, (from, to) => {
  expect(redirectTarget(from)).toBe(to)
})
