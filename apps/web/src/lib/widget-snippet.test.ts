import { describe, expect, it } from "vitest"
import {
  buildCodexMcpAddCommand,
  buildMcpAddCommand,
  buildMcpEndpoint,
  buildMcpRemoteBridgeCommand,
  buildMcpServersConfig,
  buildWidgetSnippet,
} from "./widget-snippet"

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

describe(`per-client MCP snippets (EXP-141)`, () => {
  const origin = `https://selfhost.example`

  it(`buildMcpEndpoint appends /api/mcp to the origin`, () => {
    expect(buildMcpEndpoint(origin)).toBe(`https://selfhost.example/api/mcp`)
  })

  it(`buildMcpAddCommand registers an http user-scope server for claude`, () => {
    expect(buildMcpAddCommand(origin)).toBe(
      `claude mcp add --transport http --scope user exponential https://selfhost.example/api/mcp`
    )
  })

  it(`buildCodexMcpAddCommand adds the server then logs in via OAuth`, () => {
    expect(buildCodexMcpAddCommand(origin)).toBe(
      `codex mcp add exponential --url https://selfhost.example/api/mcp\ncodex mcp login exponential`
    )
  })

  it(`buildMcpRemoteBridgeCommand bridges stdio clients via mcp-remote`, () => {
    expect(buildMcpRemoteBridgeCommand(origin)).toBe(
      `npx mcp-remote https://selfhost.example/api/mcp`
    )
  })
})
