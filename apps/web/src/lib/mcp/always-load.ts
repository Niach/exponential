// EXP-637: the always-loaded core of the MCP tool surface.
//
// Claude Code and Codex both defer MCP tool definitions behind tool search by
// default — only tool NAMES and the server `instructions` land in the context
// window at session start, and a tool's full definition is fetched when the
// model searches for it. `_meta["anthropic/alwaysLoad"]` opts a tool back into
// the always-present set. Other agents mirror the same split through their
// dynamic tool loading, reading this very flag off `tools/list`.
//
// What matters is that the handful of tools a session needs on its FIRST
// turn are present without a search: read the issue, talk on it, the one
// session verb (EXP-1291 `sessions_send`), open and merge the PR, close out.
// Everything else is discovered through search, which the instructions tell
// the agent to do.
//
// Keep this list short. Each entry costs context on every single session.
export const ALWAYS_LOAD_TOOLS = [
  `exponential_issues`,
  `exponential_comments`,
  `exponential_sessions_send`,
  `exponential_guide_write`,
  `exponential_pr`,
  // S-3: show / ask / plan, the card every turn may end with.
  `exponential_show`,
] as const

export const GATED_ALWAYS_LOAD_TOOLS = [] as const

/** Spread into a `registerTool` config to mark it always-loaded. */
export const ALWAYS_LOAD_META = { "anthropic/alwaysLoad": true } as const
