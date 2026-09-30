---
name: exponential
description: >
  Work with Exponential, a realtime issue tracker with coding agents and a
  team helpdesk, through its MCP tools: teams, boards, issues, comments,
  labels, statuses, relations, linked pull requests, coding sessions and
  support tickets. Use it whenever the user talks about their Exponential
  issues, boards or tickets.
---

# Exponential

Exponential is an open-source realtime tracker for issues, customer support
and coding agents. Teams file issues on boards, hand them to coding agents
that run on the team's own machines, and review the pull requests those
agents open. Native clients exist for web, iOS, Android, macOS, Windows and
Linux; everything syncs in realtime, so a change made here shows up for
every member at once.

All tools are named `exponential_<family>_<verb>`. Their definitions are
self-describing: read a tool's schema before calling it instead of guessing
parameters from this file.

## Scope and identity

Every call runs as the signed-in user, inside the teams and boards chosen on
the consent screen when the plugin was connected. A team or board that does
not appear in `exponential_teams_list` / `exponential_boards_list` is
outside that grant (or the user is not a member); say so, do not invent it.

## Core concepts

- **Teams** hold members, boards, labels, statuses and actions. Roles are
  owner and member; owners hold the settings and destructive surface.
- **Boards** hold issues. Each board has an identifier prefix; issues get
  identifiers like `ABC-12`, accepted by most tools wherever an issue id
  is. A board may be backed by one GitHub repository, which is what enables
  pull requests and coding sessions for its issues.
- **Issues** carry a Markdown description (`@<email>` mentions a member,
  `#<IDENTIFIER>` references another issue and links the two as related,
  images embed as Markdown), a priority (`none`, `urgent`, `high`, `medium`,
  `low`), labels, an assignee, a due date, comments and attachments.
  `exponential_issues_create` takes `parentId` for a sub-issue; the other
  relations (`blocks`, `duplicate`, `related`) are set with
  `exponential_issue_relations_add` (`inverse: true` states it the other way
  round).
- **Statuses** are per team: six builtin ones plus custom ones a member
  may add. `exponential_statuses_list` returns them; pass a row's id as
  `statusId` to `exponential_issues_update` to set a custom status
  precisely. Status changes on PR open and PR merge are automatic.
- **Pull requests**: one PR per issue, on a branch named after the issue.
  `exponential_pr_open` links a pushed branch's PR to an issue (or several,
  as one batch PR); `exponential_pr_merge` squash-merges it through the
  team's GitHub App connection; `exponential_issues_pr_files` lists the
  files it changed with patches.
- **Coding sessions** run on machines the user connected
  (`exponential_devices_list`). `exponential_sessions_start` needs an
  online device and one subject: an issue, several issues (one batch PR) or
  an action. `exponential_sessions_message` steers a live run,
  `exponential_sessions_kill` stops it, `exponential_sessions_get` reports
  its state and pull request.
- **Actions** are reusable team prompts members run on their machines;
  **automations** bind an action to a device and a schedule or event.
- **Helpdesk** (teams that enabled it): support tickets arrive from the
  team's website widget. Members read threads, reply (the reporter is
  emailed), add internal notes, close, reopen, or escalate a ticket into an
  issue.

## Workflows

### Find work

1. `exponential_boards_list` to resolve the board the user means.
2. `exponential_issues_list` with `boardIds`, and filters as asked:
   `statusCategory` or `statusId`, `priority`, `assigneeId`
   (`exponential_members_list` resolves a name or email to an id), labels,
   created/updated ranges, a title `search`, and `sort` (a `-` prefix
   descends). It returns OPEN work unless `includeClosed` or a status filter
   asks for closed, cancelled and duplicate issues. Page with `limit` and
   `offset`.
3. Present identifier, title, status, priority and assignee. Link an issue
   as `https://app.exponential.at/t/<team-slug>/boards/<board-slug>/issues/<identifier>`
   when the tool result carries a URL; otherwise name it by identifier.

### File an issue

1. Confirm the board (see the get-started skill) and, if the user named
   people or labels, resolve them with `exponential_members_list` and
   `exponential_labels_list`.
2. `exponential_issues_create` with a clear title and a Markdown
   description that states the problem, steps and expected behaviour as the
   user described them; add priority, assignee, labels, due date only when
   the user gave them.
3. Report the new identifier. Do not create duplicates: when the user
   describes something that sounds like an existing open issue, search first
   and offer to comment on that one instead.

### Read and summarize

`exponential_issues_get` (labels, relations, linked PR, recent comments) and
`exponential_comments_list` when the discussion matters. Summaries state
only what the issue and comments say; unknown owners, dates or decisions
stay unknown.

### Update, comment, label

- `exponential_issues_update`: pass only the fields that change.
- `exponential_comments_create` posts as the user; replies go under a
  top-level comment via `parentId`.
- `exponential_issue_labels_add` / `_remove`, `exponential_issues_subscribe`.

### Pull requests and coding sessions

- To have an issue worked on: `exponential_devices_list`, pick an online
  device, `exponential_sessions_start` with the issue. Track it with
  `exponential_sessions_get`; the run opens its PR itself.
- To review: `exponential_issues_pr_files` for the diff, then
  `exponential_pr_merge` only after the user explicitly asks to merge that
  PR. A merge is irreversible.

### Helpdesk

`exponential_helpdesk_threads_list` / `_get` to read,
`exponential_helpdesk_reply` to answer the reporter (it is emailed to
them: draft it, show it, send only on confirmation),
`exponential_helpdesk_note` for internal notes, `exponential_helpdesk_escalate`
to turn a ticket into an issue on a board.

## Safety rules

- Additive calls (create, comment, add a label, subscribe, note) may run
  directly when the user asked for exactly that.
- Overwriting or ending calls (update, delete, merge, retarget, close,
  revoke, kill, toggle) need the user's explicit confirmation of the exact
  target, one at a time. Never delete, merge or close in bulk from a single
  request.
- Never guess an identifier. If a name is ambiguous, list the candidates
  and ask.
- Return only what the user asked for: no internal ids or timestamps
  unless they need them for a follow-up.

## Self-hosted instances

A self-hosted Exponential serves the same MCP server at
`https://<instance>/api/mcp`. This plugin reaches the cloud instance only;
self-hosted users add their instance as a custom MCP server in their client
(https://exponential.at/docs/mcp/).

## More

- Docs: https://exponential.at/docs/ (issues and boards, coding agents,
  actions, CLI, feedback and helpdesk, widget, MCP, apps, self-host)
- Source: https://github.com/Niach/exponential
