import { readFileSync } from "node:fs"
import { join } from "node:path"
import { REPO_ROOT } from "./generate"
import { MCP_ENTRY, MCP_URL, claudeMcpJson, openclawServers } from "./mcp-entry"

const source = (path: string) => readFileSync(join(REPO_ROOT, path), `utf8`)

// #NEW-23: the launcher (Rust) and the server keep their own constants; they
// are pinned to the entry here so the three never drift apart.
describe(`the MCP server entry`, () => {
  it(`matches the launcher's constants`, () => {
    const mcp = source(`apps/desktop/crates/eagent/src/mcp.rs`)
    expect(mcp).toContain(`pub const MCP_TOKEN_ENV: &str = "${MCP_ENTRY.launcherTokenEnv}";`)
    expect(mcp).toContain(`("${MCP_ENTRY.sessionHeader}".to_string()`)
    expect(source(`apps/desktop/crates/coding/src/mcp_json.rs`)).toContain(`#[serde(rename = "${MCP_ENTRY.sessionHeader}"`)
  })

  it(`matches the server's session header`, () => {
    expect(source(`apps/web/src/lib/emcp/session-header.ts`).toLowerCase()).toContain(MCP_ENTRY.sessionHeader.toLowerCase())
  })

  it(`renders each host's block`, () => {
    expect(MCP_URL).toBe(`https://app.exponential.at/api/mcp`)
    expect(JSON.parse(claudeMcpJson())).toEqual({ mcpServers: { exponential: { type: `http`, url: MCP_URL } } })
    expect(openclawServers()).toEqual({
      exponential: {
        url: MCP_URL,
        transport: `streamable-http`,
        headers: { Authorization: `Bearer \${EXPONENTIAL_API_KEY}` },
        requestTimeoutMs: 660000,
      },
    })
  })
})
