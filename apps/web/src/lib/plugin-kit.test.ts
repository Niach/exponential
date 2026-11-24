// @vitest-environment node
import { describe, expect, it } from "vitest"
import { drift } from "@exp/plugin-kit/generate"

// #NEW-23: the harness adapters under integrations/ (their MCP server block,
// their SKILL.md, the OpenClaw theme, the Claude Code plugin's copy of the
// view-models) are generated from packages/plugin-kit. The package's own
// suite runs with the release suite; this copy of the drift gate runs on
// every pull request.
describe(`harness adapters`, () => {
  it(`are what @exp/plugin-kit renders (bun run plugin-kit:generate)`, () => {
    expect(drift()).toEqual([])
  })
})
