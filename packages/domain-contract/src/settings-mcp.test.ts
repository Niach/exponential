import { describe, expect, it } from "vitest"
import fixture from "../fixtures/settings-mcp.json" with { type: "json" }
import { activityCaption, builtinEndpoint, builtinToolBands, mcpServerOrder, mcpToolsState, toolsFailedLine, clientCaption, levelLabel, runsOnCaption, toolCountLabel, usedByCaption, type McpScope } from "./settings-mcp"
import contract from "../contract.json" with { type: "json" }

describe(`settings-mcp.json (VAPP-185)`, () => {
  it.each(fixture.usedByCases)(`usedBy: $name`, ({ apps, expected }) => {
    expect(usedByCaption(apps)).toBe(expected)
  })
  it.each(fixture.runsOnCases)(`runsOn: $name`, ({ device, expected }) => {
    expect(runsOnCaption(device)).toBe(expected)
  })
  it.each(fixture.clientCaptionCases)(`client caption: $name`, ({ scope, client, expected }) => {
    expect(clientCaption(scope as McpScope, client)).toBe(expected)
  })
  it.each(fixture.activityCaptionCases)(`activity caption: $name`, ({ owner, call, expected }) => {
    expect(activityCaption(owner, call)).toBe(expected)
  })
  // EXP-1657: the built-in section = one band per contract tool group.
  it.each(fixture.builtinCases)(`builtin bands: $name`, ({ query, expected }) => {
    const bands = builtinToolBands(query)
    expect(bands.map((b) => ({ id: b.id, label: b.label, count: b.tools.length }))).toEqual(
      expected.map((e) => ({ id: e.id, label: e.label, count: e.count }))
    )
    expected.forEach((e, i) => {
      if (`rows` in e) expect(bands[i].tools.map((t) => t.name)).toEqual(e.rows)
    })
  })
  it(`builtin bands list every live row once and never an alias`, () => {
    const listed = builtinToolBands(``).flatMap((b) => b.tools.map((t) => t.name))
    const live = contract.expToolDisplay.tools.filter((t) => !(`aliasOf` in t)).map((t) => `exponential_${t.name}`)
    expect([...listed].sort()).toEqual([...live].sort())
    expect(listed.some((n) => n.includes(`team_config`))).toBe(false)
  })
  it.each(fixture.serverOrder.cases)(`server order: $name`, ({ appTools, servers, expected }) => {
    expect(mcpServerOrder(appTools, servers)).toEqual(expected)
  })
  it.each(fixture.toolsStateCases)(`tools state: $name`, ({ transport, status, expected }) => {
    expect(mcpToolsState({ transport, status })).toBe(expected)
  })
  it(`built-in endpoints`, () => {
    expect(builtinEndpoint(`https://app.exponential.at/`, `exponential`)).toBe(`https://app.exponential.at/api/mcp`)
    expect(builtinEndpoint(`https://app.exponential.at`, `apps`)).toBe(`https://app.exponential.at/api/mcp/apps`)
    expect(toolsFailedLine(`HTTP 500`)).toBe(`Could not list its tools: HTTP 500`)
  })
  it(`labels`, () => {
    expect(toolCountLabel(1)).toBe(`1 tool`)
    expect(toolCountLabel(3)).toBe(`3 tools`)
    expect(fixture.levels.map((l) => levelLabel(l.value as never))).toEqual([`None`, `Read only`, `Full`])
    expect(fixture.sections).toEqual(Object.keys(fixture.testIds).filter((k) => fixture.sections.includes(k)))
  })
})
