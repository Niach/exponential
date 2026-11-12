---
name: exponential
description: "Work in Exponential (exponential.at), the realtime issue tracker with coding agents: find, show, create and update issues on boards, comment, follow pull requests and sessions, open the interactive issue list and inbox, and start a coding agent on one of the person's devices. Use when the person mentions Exponential, an issue identifier like EXP-12, their boards, or asks to track or hand off work there."
---

# Exponential

Exponential is a realtime issue tracker. Issues live on boards inside teams and
carry a status, a priority, labels, an assignee and comments. They close
through pull requests that coding agents open from the team's own machines.

The `exponential` connector reaches it. Act on what the person asks for: read
what you need to answer, and change something only when they asked for that
change.

## The tools

Every area of the tracker is a pair of tools. The read tool never changes
anything; its `_write` twin holds every other operation. Each tool takes `op`
plus that op's fields flat beside it, never nested:

```
exponential_issues {op: "get", id: "EXP-12"}
exponential_issues_write {op: "update", id: "EXP-12", priority: "high"}
```

Each tool's description lists its ops and their fields; read it before a call
you have not made yet.

| Read tool | Read ops | Write twin |
| --- | --- | --- |
| `exponential_issues` | list, get, show, activity | `exponential_issues_write` |
| `exponential_comments` | list | `exponential_comments_write` |
| `exponential_pr` | files | `exponential_pr_write` |
| `exponential_boards` | list, get | `exponential_boards_write` |
| `exponential_labels` | list, get | `exponential_labels_write` |
| `exponential_statuses` | list | `exponential_statuses_write` |
| `exponential_attachments` | list, get | `exponential_attachments_write` |
| `exponential_teams` | list, get | `exponential_teams_write` |
| `exponential_people` | list, invites_list | `exponential_people_write` |
| `exponential_repositories` | list | `exponential_repositories_write` |
| `exponential_devices` | list | `exponential_devices_write` |
| `exponential_agents` | list, get | `exponential_agents_write` |
| `exponential_prompts` | list, get | `exponential_prompts_write` |
| `exponential_skills` | list, get | `exponential_skills_write` |
| `exponential_mcp_servers` | list | `exponential_mcp_servers_write` |
| `exponential_sessions` | get, list | `exponential_sessions_write` |
| `exponential_apps` | list, get, catalog, records_list, records_history | `exponential_apps_write` |
| `exponential_events` | list | `exponential_events_write` |
| `exponential_notifications` | list | `exponential_notifications_write` |

Two verbs stand alone: `exponential_sessions_send` (start or message a
session) and `exponential_show` (draw a genUI card).

In Claude Code the tools carry a plugin prefix
(`mcp__plugin_exponential_exponential__exponential_issues`); the names above
are the part after the last `__`.

## Resolve ids first

Write ops take UUIDs, so look them up before you write:

- teams: `exponential_teams {op: "list"}`
- boards of a team: `exponential_boards {op: "list", teamId}`
- members (assignees, mentions): `exponential_people {op: "list", teamId}`
- labels: `exponential_labels {op: "list", teamId}`
- statuses: `exponential_statuses {op: "list", teamId}`

Issues accept their identifier (`EXP-12`) wherever an issue id goes. When the
person has only one team or the board is obvious from the conversation, do not
ask; when several fit, ask which one.

## Reading work

- **Interactive list**: when the person wants to *see* issues ("show my
  issues", "what's open on the iOS board"), use `exponential_issues
  {op: "show"}` with optional `boardId`, `teamId`, `assigneeId`, `search`,
  `includeClosed`, `limit`. Where Claude renders MCP Apps it opens as a
  view: status groups, priorities, PR numbers. A row opens the issue's detail
  in place, with its description, comments and pull request. The model gets a
  short summary with each row's identifier, so you can follow up.
- **Data for your own reasoning**: `exponential_issues {op: "list"}` (filters
  by status, label, assignee, dates, comments; open issues only unless
  `includeClosed` or a status filter) and `{op: "get", id}` (one issue with
  its latest comments).
- **History**: `exponential_issues {op: "activity", issueId}`; comments with
  `exponential_comments {op: "list", issueId}`.
- **Pull requests**: the changed files and CI checks of an issue's pull
  request: `exponential_pr {op: "files", issueId}`.
- **Inbox**: `exponential_notifications {op: "list"}` opens the person's
  notifications as a view.
- **Devices**: `exponential_devices {op: "list"}` opens the person's machines
  (online or not, and the coding agents signed in on each) as a view.

## Changing work

- Create: `exponential_issues_write {op: "create", boardId, title,
  description?, priority?, assigneeId?, labelIds?, dueDate?}`.
- Edit: `{op: "update", id, ...}`; move between statuses: `{op:
  "update_status", id, status}` (or `statusId` from the team's statuses).
  Statuses normally move on their own when a pull request opens or merges, so
  set one only when the person asks.
- Labels: `{op: "labels_add" | "labels_remove", issueId, labelId}`;
  relations between issues: `{op: "relations_add", issueId, relatedIssueId,
  type}`.
- Comment: `exponential_comments_write {op: "create", issueId, body}`; a
  reply passes `parentId`.
- Before anything destructive or hard to undo (any `op: "delete"`,
  `exponential_pr_write {op: "merge"}`, `exponential_sessions_write
  {op: "end"}`), say what will happen and get the person's go-ahead.

### Text format

Descriptions and comments are plain GitHub-flavoured Markdown, the same on web,
desktop, iOS and Android:

- `#EXP-12` links an issue (its identifier after `#`).
- `@person@example.com` mentions a member and notifies them; take the email
  from `exponential_people {op: "list"}`. Mention only people the person
  asked you to involve.
- Emoji are plain unicode; images are `![alt](/api/attachments/{id})` from an
  uploaded attachment.
- No HTML.

## Coding agents on the person's devices

Exponential runs coding agents (Claude Code, Codex and others) on the team's
own machines through the Exponential desktop app or CLI, never in Claude.
`exponential_sessions_send` is the one call for that:

1. `exponential_devices {op: "list"}` names the devices and the agents signed
   in on each. Ask which device if more than one is online and the person did
   not say.
2. Start a root session: `exponential_sessions_send {teamId, deviceId,
   issueIds: ["EXP-12"], text?}`. Optional: `cli` (which agent),
   `repositoryId`, `model`, `effort`, `promptId` (a saved prompt from
   `exponential_prompts {op: "list"}`).
3. Message a running or idle session: `{sessionId, text}`; answer its open
   question or plan: `{sessionId, answer: {...}}`; interrupt its turn:
   `{sessionId, stop: true}`.
4. Talk to a team agent: `{agentId, text}` (`exponential_agents
   {op: "list"}` names them).

The reply carries the session's id and a link. Sessions run on their own: read
progress later with `exponential_sessions {op: "get", id}` (title, linked
issues, PRs, latest messages) when the person asks. Never poll in a loop.

## Cards

`exponential_show {surface, title?}` draws one genUI card inline, where Claude
renders MCP Apps: a status summary, a table, a small dashboard. A surface is a
tree of nodes `{id, component, props?, children?}`; use a Stack or List of
real components (Row, Badge, StatTile, Table, Chart), never a Card root.
`exponential_apps {op: "catalog"}` lists every component and its props, and
the `exponential-genui` skill covers the format. Questions and plans with
options need an Exponential session's chat; outside one, ask in plain text.

## Apps

Teams build their own apps in Exponential: declarative genUI packages with
their own records (a CRM, a stand-up, a time tracker). `exponential_apps
{op: "list"}` names a team's apps, `{op: "get", slug}` opens one (as a view
where Claude renders MCP Apps), `{op: "records_list", slug}` reads its data.
Writing a package or records goes through `exponential_apps_write`; the
`exponential-apps` skill covers the package format.

## When something fails

- A 401 or a sign-in prompt: the connector is not signed in or the grant was
  revoked; ask the person to reconnect instead of retrying.
- "not granted": the person's consent covers other teams or boards; they can
  widen it by reconnecting.
- A tool that needs an Exponential session (asking with options, planning,
  reporting on your own session) is not available from Claude; say so plainly.
