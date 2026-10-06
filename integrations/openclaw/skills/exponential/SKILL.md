---
name: exponential
description: "Track and change work in Exponential (exponential.at): list, show, create and update issues on boards, comment, follow pull requests and coding runs, and show the interactive issue list or a run's report."
---

# Exponential

Exponential is a realtime issue tracker: issues live on boards inside teams,
carry a status, priority, labels and comments, and close through pull requests
that coding agents open. This plugin connects the `exponential` MCP server, so
its tools appear as `exponential__exponential_*`.

## Showing work

- "Show my issues", "what's open on the iOS board": call
  `exponential_issues_show` (optional `boardId`, `teamId`, `assigneeId`,
  `search`, `includeClosed`, `limit`). With MCP Apps on, OpenClaw renders the
  interactive list: status groups, priorities, PR numbers; a row opens the
  issue. Prefer it over `exponential_issues_list` whenever the person will look
  at the result.
- A run's report (summary, topics, touched files):
  `exponential_sessions_get` with the run id from `exponential_sessions_list`.
- Plain data for your own reasoning: `exponential_issues_list` (filters by
  status, label, dates, comments) and `exponential_issues_get` (one issue with
  its latest comments; accepts an identifier like `APP-12`).

## Changing work

- Resolve ids first: `exponential_teams_list`, `exponential_boards_list`,
  `exponential_members_list`, `exponential_labels_list`,
  `exponential_statuses_list`.
- Create with `exponential_issues_create`, edit with `exponential_issues_update`,
  move with `exponential_issues_update_status`, comment with
  `exponential_comments_create`. Descriptions and comments are plain GFM;
  `#APP-12` links an issue, `@person@example.com` mentions a member.
- Confirm before anything destructive (`*_delete`, `exponential_pr_merge`,
  `exponential_sessions_kill`).

## Coding runs

`exponential_sessions_start` hands an issue to a coding agent on one of the
person's devices (`exponential_devices_list` names them and their agent
accounts). The run opens a pull request; follow it with
`exponential_sessions_get`.

## Connection

The server is `https://app.exponential.at/api/mcp` (self-hosted: the
instance's `/api/mcp`). It authenticates with the `EXPONENTIAL_API_KEY`
environment variable (a personal `expu_` key from Settings → Security) or,
when the operator configured `auth: "oauth"`, with OAuth. A 401 means neither
is set: say so instead of retrying.
