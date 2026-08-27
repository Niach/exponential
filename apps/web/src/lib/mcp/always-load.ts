// EXP-637: the always-loaded core of the MCP tool surface.
//
// Claude Code and Codex both defer MCP tool definitions behind tool search by
// default — only tool NAMES and the server `instructions` land in the context
// window at session start, and a tool's full definition is fetched when the
// model searches for it. `_meta["anthropic/alwaysLoad"]` opts a tool back into
// the always-present set. The pi bridge mirrors the same split through pi's
// dynamic tool loading, reading this very flag off `tools/list`.
//
// So the old "every tool must fit one shared byte ceiling" rule is gone; what
// matters now is that the handful of tools a coding run needs on its FIRST
// turn are present without a search. Everything else — boards, labels,
// members, attachments, notifications, repositories, invites, teams — is
// discovered through search, which the instructions tell the agent to do.
//
// Keep this list short. Each entry costs context on every single session.
export const ALWAYS_LOAD_TOOLS = [
  `exponential_issues_get`,
  `exponential_comments_list`,
  `exponential_comments_create`,
  `exponential_issues_update_status`,
  `exponential_pr_open`,
  `exponential_pr_merge`,
  `exponential_pr_retarget`,
  `exponential_sessions_end`,
  `exponential_actions_create`,
] as const

/** Spread into a `registerTool` config to mark it always-loaded. */
export const ALWAYS_LOAD_META = { "anthropic/alwaysLoad": true } as const
