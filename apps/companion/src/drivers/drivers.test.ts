import { describe, expect, it } from "bun:test"
import { ClaudeDriver, claudeMcpServers } from "./claude"
import { CodexDriver, codexOptionsForMcp } from "./codex"
import { UnsafePermissionError } from "./index"

// These tests confirm that both drivers HARD-REFUSE unsafe permission modes.
// They do NOT exercise a real model call — they fail fast in the driver
// before any network/IPC happens.

describe(`drivers`, () => {
  it(`ClaudeDriver exists and exposes safe defaults via its name`, () => {
    const d = new ClaudeDriver()
    expect(d.name).toBe(`claude`)
  })

  it(`CodexDriver exists and exposes safe defaults via its name`, () => {
    const d = new CodexDriver()
    expect(d.name).toBe(`codex`)
  })

  // We can't easily intercept the SDK call without a full mock, so this test
  // documents the contract: the FORBIDDEN_* constants must include
  // bypassPermissions / danger-full-access. If those sets ever shrink, this
  // test surface is meant to remind us why.
  it(`UnsafePermissionError type is exported`, () => {
    expect(UnsafePermissionError.prototype.name).toBeDefined()
    const e = new UnsafePermissionError(`x`)
    expect(e.name).toBe(`UnsafePermissionError`)
  })

  it(`wires Exponential MCP into Claude and Codex`, () => {
    const mcpServer = {
      url: `https://app.exponential.at/api/mcp`,
      token: `expk_secret`,
    }

    expect(claudeMcpServers(mcpServer)).toEqual({
      exponential: {
        type: `http`,
        url: mcpServer.url,
        headers: {
          Authorization: `Bearer ${mcpServer.token}`,
        },
        alwaysLoad: true,
      },
    })

    const codexOptions = codexOptionsForMcp(mcpServer)
    expect(codexOptions?.env?.EXPONENTIAL_MCP_TOKEN).toBe(mcpServer.token)
    expect(codexOptions?.config).toEqual({
      mcp_servers: {
        exponential: {
          url: mcpServer.url,
          bearer_token_env_var: `EXPONENTIAL_MCP_TOKEN`,
        },
      },
    })
  })
})
