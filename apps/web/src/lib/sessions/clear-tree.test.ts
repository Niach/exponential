import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/sealed-content.json"
import { startsClearTree } from "@/lib/sessions/sealed"

// #EDE-97 (sealed-content.json `clearTree.startCases`): only an outside MCP
// client's start (over MCP, no calling session, no inline key) is clear.
describe(`startsClearTree`, () => {
  for (const c of fixture.clearTree.startCases) {
    it(c.name, () => {
      expect(
        startsClearTree({
          viaMcp: c.via === `mcpOutside` || c.via === `mcpSession`,
          caller: c.via === `mcpSession` ? { id: `caller` } : null,
          seal: c.seal,
        })
      ).toBe(c.clearTree)
    })
  }
})
