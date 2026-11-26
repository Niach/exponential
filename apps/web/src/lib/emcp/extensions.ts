// #NEW-144: THE map of the MCP extensions Exponential's own server declares
// (`capabilities.extensions`, answered by both `initialize` and
// `server/discover`; packages/emcp/fixtures/protocol.json
// `serverExtensions`). An extension enters this map together with its
// handlers, never before (#NEW-143's phases add tasks, skills, files).
// #NEW-141: OpenAI's mentions (`openai/mentions`, mentions.ts) beside the
// MCP Apps views. Session callers (our own launched agents) declare none.
import type { JSONObject } from "@modelcontextprotocol/server"
import { EXTENSION_IDS } from "@exp/emcp/protocol"

/** The tool a host searches `@<query>` mentions with (mentions.ts). */
export const MENTIONS_TOOL = `exponential_issues`

export const SERVER_EXTENSIONS: Readonly<Record<string, JSONObject>> = {
  [EXTENSION_IDS.mentions]: { searchTool: MENTIONS_TOOL },
}

export const MCP_EXTENSION_CAPABILITIES = { extensions: SERVER_EXTENSIONS }
