import { describe, expect, it } from "vitest"
import { upstreamToolRow } from "@/lib/emcp/upstream/mcp-client"

// Settings › MCP: an open team server row lists its tools/list answer as
// title + the description's first line, the wire name as the tooltip.
describe(`upstreamToolRow`, () => {
  it(`prefers title, then annotations.title, then the name`, () => {
    expect(upstreamToolRow({ name: `a`, title: `A tool`, annotations: { title: `X` } })?.title).toBe(`A tool`)
    expect(upstreamToolRow({ name: `a`, annotations: { title: `Annotated` } })?.title).toBe(`Annotated`)
    expect(upstreamToolRow({ name: `list_issues` })?.title).toBe(`list_issues`)
  })
  it(`keeps the description's first line`, () => {
    expect(upstreamToolRow({ name: `a`, description: `  Lists issues.\nArgs: …` })?.blurb).toBe(`Lists issues.`)
    expect(upstreamToolRow({ name: `a` })?.blurb).toBe(``)
  })
  it(`drops an entry without a name`, () => {
    expect(upstreamToolRow({ title: `x` })).toBeNull()
    expect(upstreamToolRow(null)).toBeNull()
  })
})
