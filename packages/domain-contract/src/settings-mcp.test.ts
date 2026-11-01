import { describe, expect, it } from "vitest"
import fixture from "../fixtures/settings-mcp.json" with { type: "json" }
import { activityCaption, clientCaption, levelLabel, toolCountLabel, usedByCaption, type McpScope } from "./settings-mcp"

describe(`settings-mcp.json (VAPP-185)`, () => {
  it.each(fixture.usedByCases)(`usedBy: $name`, ({ apps, expected }) => {
    expect(usedByCaption(apps)).toBe(expected)
  })
  it.each(fixture.clientCaptionCases)(`client caption: $name`, ({ scope, client, expected }) => {
    expect(clientCaption(scope as McpScope, client)).toBe(expected)
  })
  it.each(fixture.activityCaptionCases)(`activity caption: $name`, ({ owner, call, expected }) => {
    expect(activityCaption(owner, call)).toBe(expected)
  })
  it(`labels`, () => {
    expect(toolCountLabel(1)).toBe(`1 tool`)
    expect(toolCountLabel(3)).toBe(`3 tools`)
    expect(fixture.levels.map((l) => levelLabel(l.value as never))).toEqual([`None`, `Read only`, `Full`])
    expect(fixture.sections).toEqual(Object.keys(fixture.testIds).filter((k) => fixture.sections.includes(k)))
  })
})
