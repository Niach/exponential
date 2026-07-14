import { describe, expect, it } from "vitest"
import { buildMcpServersConfig, buildWidgetSnippet } from "./widget-snippet"

describe(`buildWidgetSnippet`, () => {
  it(`points the loader at the given origin and inlines the key`, () => {
    const snippet = buildWidgetSnippet(`expw_abc123`, `https://example.com`)
    expect(snippet).toContain(`"https://example.com/widget/v1/loader.js"`)
    expect(snippet).toContain(`ExponentialWidget.init({ key: "expw_abc123" })`)
  })
})

describe(`buildMcpServersConfig`, () => {
  it(`emits the docs' mcpServers shape for the given origin`, () => {
    const config = buildMcpServersConfig(`https://app.exponential.at`)
    expect(JSON.parse(config)).toEqual({
      mcpServers: {
        exponential: { url: `https://app.exponential.at/api/mcp` },
      },
    })
  })
})
