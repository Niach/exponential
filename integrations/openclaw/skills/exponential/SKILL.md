---
name: exponential
description: "Track and change work in Exponential (exponential.at): list, show, create and update issues on boards, comment, follow pull requests and sessions, message team agents, and show the interactive issue list."
---

# Exponential

Exponential is a realtime issue tracker: issues live on boards inside teams,
carry a status, priority, labels and comments, and close through pull requests
that coding agents open. This plugin connects the `exponential` MCP server:
15 tools, one per noun, each taking an `op` with that op's fields beside it
(`exponential_issues {op: "get", id: "APP-12"}`); every tool's description
lists its ops. They appear as `exponential__exponential_*`.

## Showing work

- "Show my issues", "what's open on the iOS board": `exponential_issues
  {op: "show"}` (optional `boardId`, `teamId`, `assigneeId`, `search`,
  `includeClosed`, `limit`). With MCP Apps on, OpenClaw renders the
  interactive list: status groups, priorities, PR numbers; a row opens the
  issue. Prefer it over `op: "list"` whenever the person will look at the
  result.
- A thread's report (title, issues, PRs, Guide, latest messages):
  `exponential_sessions {op: "get", id}` with an id from `op: "list"`.
- Plain data for your own reasoning: `exponential_issues {op: "list"}`
  (filters by status, label, dates, comments) and `op: "get"` (one issue with
  its latest comments; accepts an identifier like `APP-12`).

## Changing work

- Resolve ids first: `exponential_teams {op: "list"}`, `exponential_boards
  {op: "list"}`, `exponential_people {op: "list"}`, `exponential_team_config
  {op: "labels_list" | "statuses_list"}`.
- Create with `exponential_issues {op: "create"}`, edit with `op: "update"`,
  move with `op: "update_status"`, comment with `exponential_comments
  {op: "create"}`. Descriptions and comments are plain GFM; `#APP-12` links an
  issue, `@person@example.com` mentions a member.
- Confirm before anything destructive (`op: "delete"`, `exponential_pr
  {op: "merge"}`, `exponential_sessions {op: "end"}`).

## Sessions and agents

`exponential_sessions_send` is the one session verb: `{teamId, deviceId,
issueIds, text}` starts a thread on one of the person's devices
(`exponential_devices {op: "list"}` names them and their agent accounts),
`{sessionId, text}` messages a running or idle one, `{agentId, text}`
messages a team agent (the person's own conversation with it), `{sessionId,
stop: true}` interrupts a turn. Every transition of a session (a reply, a
question, a PR, its end) arrives as a message in its thread: read it with
`exponential_sessions {op: "get", id}`; never poll in a loop.

## Connection

The server is `https://app.exponential.at/api/mcp` (self-hosted: the
instance's `/api/mcp`). It authenticates with the `EXPONENTIAL_API_KEY`
environment variable (a personal `expu_` key from Settings → Security) or,
when the operator configured `auth: "oauth"`, with OAuth. A 401 means neither
is set: say so instead of retrying.
