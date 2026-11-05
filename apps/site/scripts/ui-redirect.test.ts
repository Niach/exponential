import { expect, test } from "bun:test"
import { redirectFor } from "../src/redirects"
import { redirectTarget } from "./ui-redirect"

test.each([
  [`https://ui.exponential.at/`, `https://exponential.at/sdk/genui/`],
  [`https://ui.exponential.at/components/button/`, `https://exponential.at/docs/sdk/genui/reference/components/button/`],
  [`https://ui.exponential.at/ui/guides/react/?x=1`, `https://exponential.at/docs/sdk/genui/guides/react/?x=1`],
  [`https://ui.exponential.at/guides/motion`, `https://exponential.at/docs/sdk/motion/guides/animate/`],
  [`https://ui.exponential.at/concepts/`, `https://exponential.at/docs/sdk/genui/`],
  [`https://ui.exponential.at/llms.txt`, `https://exponential.at/llms.txt`],
  [`https://ui.exponential.at/shots/exponential-ui-button/web.webp`, `https://exponential.at/shots/exponential-ui-button/web.webp`],
])(`%s → %s`, (from, to) => {
  expect(redirectTarget(from)).toBe(to)
})

test.each([
  [`/ui/`, `/sdk/genui/`],
  [`/talk`, `/sdk/voice/`],
  [`/mcp/`, `/sdk/mcp-proxy/`],
  [`/apps/`, `/sdk/apps/`],
  [`/docs/libraries/eui/reference/components/button/`, `/docs/sdk/genui/reference/components/button/`],
  [`/docs/libraries/etalk/guides/your-voice/`, `/docs/sdk/voice/guides/your-voice/`],
  [`/docs/libraries/install/`, `/docs/sdk/install/`],
  [`/docs/eapps/build/`, `/docs/sdk/apps/build/`],
  [`/docs/reference/spec/eui-round-3/`, `/docs/reference/spec/genui-round-3/`],
])(`%s → %s`, (from, to) => {
  expect(redirectFor(from)).toBe(to)
})

test(`a file served under an old prefix stays`, () => {
  expect(redirectFor(`/talk/skills/voice-reference-clip/SKILL.md`)).toBeNull()
})
