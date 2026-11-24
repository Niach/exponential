import entry from "./mcp-entry.json"

// #NEW-23: the ONE MCP server entry, rendered per host. Nothing here is
// written by hand into an adapter: `src/generate.ts` writes each block and
// the drift gate fails a hand edit.

export const MCP_ENTRY = entry

/** The hosted server's URL. */
export const MCP_URL = `${entry.origin}${entry.path}`

const json = (value: unknown) => `${JSON.stringify(value, null, 2)}\n`

/** The Claude plugin (chat, Cowork, the directory listing): OAuth only. */
export function claudeMcpJson(): string {
  return json({ mcpServers: { [entry.server]: { type: `http`, url: MCP_URL } } })
}

/** The Claude Code plugin's `userConfig` field that names the instance. */
export const CLAUDE_CODE_INSTANCE_OPTION = `instance`

/** The Claude Code plugin: the instance is a `userConfig` option (a
 *  self-hoster's own origin); sign-in is OAuth, discovered on the first 401. */
export function claudeCodeMcpJson(): string {
  return json({
    mcpServers: {
      [entry.server]: { type: `http`, url: `\${user_config.${CLAUDE_CODE_INSTANCE_OPTION}}${entry.path}` },
    },
  })
}

/** The `mcpServers` block of `openclaw.plugin.json` (a headless gateway: the
 *  person's key from the environment). */
export function openclawServers(): Record<string, unknown> {
  return {
    [entry.server]: {
      url: MCP_URL,
      transport: `streamable-http`,
      headers: { [entry.authorizationHeader]: `${entry.bearerScheme} \${${entry.apiKeyEnv}}` },
      requestTimeoutMs: entry.requestTimeoutMs,
    },
  }
}
