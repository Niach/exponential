---
name: exponential
description: >
  Work with Exponential (exponential.at), an open-source realtime issue
  tracker with local coding agents and an embeddable feedback widget
  whose reports land as issues. Use this to connect over MCP, manage teams, boards,
  issues, labels, statuses, comments, and pull requests, install and
  script the exponential CLI, or integrate the feedback widget on a
  user's website.
---

# Exponential

Exponential is an open-source (Apache-2.0) realtime tracker for issues,
user feedback, and coding agents. Teams file issues on boards, hand them
to AI coding agents (Claude Code or Codex) that run locally on the
user's own machines, and review the pull requests the agents open. Native
clients exist for web, iOS, Android, macOS, Windows, and Linux; everything
syncs in realtime.

- Cloud app: https://app.exponential.at (free for teams of three)
- Source: https://github.com/Niach/exponential
- Self-hosting is free at any company size. A self-hosted deployment serves
  only the app at its own domain, not the marketing site; substitute the
  instance origin wherever this file says app.exponential.at.

## MCP endpoint

Every instance exposes a streamable-HTTP MCP server:

    https://app.exponential.at/api/mcp

Self-hosted: same path on the instance, `https://<instance>/api/mcp`. There
is no separate `/sse` variant; modern clients speak streamable HTTP
directly. The server's `tools/list` is self-describing: call it for exact
tool names and parameter schemas instead of relying on a static list.

## Authentication

Two ways in:

1. **OAuth, for interactive clients.** Point the client at the endpoint
   with no credentials. It registers itself (dynamic client registration)
   and opens a browser consent screen, which is a scope picker: the user
   grants everything, specific teams, or specific boards. The token is
   confined to exactly that grant; re-running consent widens or narrows it.
2. **Personal API keys, for headless and scripted use.** The user generates
   a key under Settings -> Security in the web app (prefix `expu_`; the raw
   key is shown exactly once at mint time). Send it as either header:

       Authorization: Bearer expu_...
       x-api-key: expu_...

   API keys act as the user with their full membership. Guard them
   accordingly. The same key signs the CLI in via `EXP_TOKEN` (below).

## Tool families

Around 80 tools, all named `exponential_<family>_<verb>`:

- **teams**: list, get, create, update the user's teams.
- **boards**: CRUD boards inside a team; a board can be backed by a GitHub
  repository (`boards_set_repository`).
- **issues**: list and filter (boards, `statusId`/`statusCategory`,
  priority, assignee, labels any/all/unlabeled, `source` user/widget,
  comment activity, created/updated ranges, title search, each with an
  `exclude*` twin,
  plus `sort`, where a `-` prefix descends), get by UUID or identifier
  ("ABC-12"), create, update, delete, update_status, subscribe,
  unsubscribe. `issues_list` returns OPEN work unless `includeClosed` (or
  a status filter) asks for the completed, cancelled and duplicate ones.
  Every list tool paginates: 50 by default, 200 at most (1000 for
  `issues_list`).
- **statuses**: `statuses_list` returns a team's issue statuses (builtin
  and custom); pass a row's id as `statusId` to `issues_update` to set a
  custom status precisely. `statuses_create` / `_update` / `_delete`
  manage the custom ones (builtins are locked; deleting one that issues
  still use needs a `reassignToId`).
- **PRs**: `pr_open` links a pushed branch's pull request to one issue, to
  a whole batch via `issueIds` + `head`, or to nothing at all via
  `repositoryId` + `head` (a chore PR); `pr_merge` squash-merges through
  the GitHub App (no gh, no token) and mirrors those three forms
  (`repositoryId` + `prNumber` for the chore case), with `endSessions`
  overriding the team's end-sessions-on-merge setting for one call;
  `pr_retarget` repoints an open PR's base; `issues_pr_files` lists the
  linked PR's changed files with patches.
- **labels** and **issue_labels**: team label CRUD; attach and detach.
- **issue_relations**: `add` / `remove` link two issues as `blocks`,
  `parent` (sub-issue), `duplicate` or `related`; `inverse: true` states
  the relation the other way round (blocked by, sub-issue of, duplicated by).
- **comments**: list, create, update, delete on issues. `comments_create`
  takes `audience: "reporter"` on a widget-filed issue whose reporter left
  an email (top-level only): the comment is emailed to them, and the
  result's `reporterEmailed` says whether that mail went out.
- **notifications**: list, mark read.
- **members** and **invites**: list team members (resolve assignee ids),
  manage invite links (owner only).
- **repositories**: list and register GitHub repos; `branch_diff` diffs an
  issue's branch against the repo default branch.
- **actions**: CRUD reusable team prompts that members run locally.
  `actions_list` returns each action's `triggers`; `actions_update` takes
  `triggers` as the WHOLE array (every trigger you keep with its `id`, a
  new one without). A trigger is `{enabled, deviceId, agent?, account?, model?,
  effort?}` plus either `{kind:"schedule",
  interval:"daily"|"weekly"|"monthly", minuteOfDay, weekday?,
  dayOfMonth?}` (the device's local clock) or `{kind:"event",
  event:"created"|"status_changed"|"assignee_changed"|"label_added"|
  "priority_changed"|"pr_opened"|"pr_merged", filters?}`. Writes are
  owner-only, and a trigger can only be enabled while every input of its
  action is optional.
- **attachments**: upload, get, delete images; upload returns the
  embeddable markdown form.
- **sessions**: list, get, message (steer), kill, and start a coding,
  action or chat session on one of the user's own machines. A start
  targets an ONLINE device (`devices_list` first) — offline devices are
  refused, never queued. A run may delegate independent work to a second
  run with `sessions_start`; the child reports back into the starter's
  session. Inside a launcher-started run four more tools register:
  `sessions_end` (ends the run: unattended runs last, attended ones when
  asked), `sessions_ask_parent` (ask the run that started this one, or the
  person who owns it), `sessions_results` (the run's report by topic, with
  screenshots; it becomes the PR body) and `sessions_show` (a picture in the
  run's transcript while it works). Every launched run also gets a short
  playbook of these conventions appended to its system prompt.
- **devices**: `devices_list` shows the user's machines, their online state
  and the agent CLIs each one can run.
- **report_bug**: file a bug about Exponential itself with its developers.

Every call is confined to the OAuth grant's scope, or to the API key
user's membership.

## Core concepts

- **Teams** hold members, boards, labels, statuses, and actions. Roles are
  owner and member; owners hold the settings and destructive surface.
- **Boards** hold issues. Each board has an identifier prefix; issues get
  identifiers like `ABC-12`, accepted by most tools wherever an issue id
  is. A board optionally links to one GitHub repository, which is what
  enables the coding features.
- **Issues** carry GFM markdown descriptions (plain `@<email>` mentions,
  `#<IDENTIFIER>` issue refs, image embeds), a priority (`none`, `urgent`,
  `high`, `medium`, `low`), labels, an assignee, a due date, comments, and
  attachments. A `#<IDENTIFIER>` ref in a description or comment auto-links
  the two issues as related; `issues_create` takes `parentId` to file a
  sub-issue, and the other relations (blocks, duplicate, related) are set
  with `issue_relations_add`.
- **Statuses** are per-team rows in six categories (backlog, unstarted,
  started, completed, cancelled, duplicate). Six builtins always exist
  (backlog, in_progress, in_review, done, cancelled, duplicate) and
  teams add custom named statuses. Enum-taking tools accept the builtin
  values; `statuses_list` + `statusId` reach the custom rows.
- **Pull requests**: one PR per issue, on branch `exp/<IDENTIFIER>`. A
  batch of issues fixed on one pushed branch shares one PR via
  `exponential_pr_open` with `issueIds` + `head`. PR open moves linked
  issues to the team's configured PR-open status (default In Review);
  merging moves them to the PR-merge status (default Done).
- **Actions** are reusable markdown prompts (up to 10 typed pick inputs:
  `repo`, `board`, `pr`, `icon`; free text goes in the run's prompt, the
  composer's "Additional instructions") that members run as local agent
  sessions from the desktop app, the web, or the CLI. An
  action's **triggers** each bind it to one device and a schedule or an
  event; the bound machine starts the run itself, so nothing fires while
  it is off.
- **Coding agents**: sessions run Claude Code or Codex locally with the
  Exponential MCP server wired in automatically; from the agent's point of
  view the tools look the same on both.
- **Feedback widget**: every submission from the embeddable widget becomes
  an issue on the widget's board (title from the message's first line,
  screenshot and pictures attached, page context in the description,
  `source: "widget"`). A reporter who left an email gets a private link to
  their report; a `comments_create` with `audience: "reporter"` emails them
  a reply, and their answers come back as comments on the issue (a
  `reporter_reply` notification; an answer on a done issue reopens it).

## The CLI

Install (Linux x86_64/arm64, macOS Apple Silicon):

    curl -fsSL https://exponential.at/install.sh | sh

Self-hosted instances use the same script with `EXP_INSTANCE`:

    curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=https://issues.example.com sh

`exponential login` uses a device-code flow (RFC 8628): the CLI prints a
short code and a URL, the user approves in any signed-in browser. For
non-interactive setups, skip the browser with an API key:

    EXP_INSTANCE=https://issues.example.com EXP_TOKEN=expu_... exponential login

Key commands: `whoami`, `status`, `doctor` (checks git and the agent
CLIs), `code <ISSUE>` (start a coding session for an issue, e.g.
`exponential code EXP-42 --agent claude`), `run <action>`, `daemon
install` (register a Linux or macOS machine as an always-on agent box,
visible under Devices -> My devices in the web app), `update`.
`code` and `run` share `--agent claude|codex`, `--model`, `--effort`,
`--plan`, and `--detach` (run headless but still steerable from the web);
`run` also takes `--team <id>` and repeated `--input k=v`. Full
reference: https://exponential.at/docs/cli/

## Integrating the feedback widget for a user

When a user asks to add Exponential feedback collection to their site:

1. Get their widget key (`expw_...`). If they have none, they create a
   widget in Team settings -> Widget in Exponential (team owners; every
   plan includes at least one), pick the board its reports land on, and add
   their site's domain to the allowlist (submissions are only accepted from
   allowlisted domains; the key itself is public by design).
2. Paste this snippet before `</head>`. It is async and never blocks the
   page. On self-hosted instances, point the loader URL at the instance:

   ```html
   <script>
     (function (w, d, u) {
       if (w.ExponentialWidget) return;
       var q = [], api = { q: q };
       ["init","identify","setCustomData","setTheme","setLauncherHidden","open","close","submit"].forEach(function (m) {
         api[m] = function () { q.push([m, [].slice.call(arguments)]); };
       });
       w.ExponentialWidget = api;
       var s = d.createElement("script");
       s.async = true; s.src = u;
       d.head.appendChild(s);
     })(window, document, "https://app.exponential.at/widget/v1/loader.js");
     ExponentialWidget.init({ key: "expw_YOUR_KEY" });
   </script>
   ```

3. Optional wiring when the site has auth or theming:
   - `ExponentialWidget.identify({ email, name, userId })` after sign-in,
     so reports arrive with a real reporter.
   - `ExponentialWidget.setCustomData({ plan: "business", version: "1.2" })`
     to stamp context onto every submission.
   - `ExponentialWidget.setTheme("dark" | "light" | "auto")` to follow the
     site's theme toggle.
   - `ExponentialWidget.setLauncherHidden(true)` to hide just the button
     while the site's own UI covers its corner; the panel and
     `open()`/`close()`/`submit()` keep working.
   - Launcher placement per device:
     `init({ key, launcher: { desktop: { mode: "fab", position:
     "bottom-right" }, mobile: { mode: "tab", position: "middle-right" } } })`
     — `mode` is `fab` or `tab`, `position` one of `top-|middle-|bottom-`
     `left|right`; devices split at a 767px viewport. The older
     `position: "bottom-right" | "bottom-left"` option still works but is
     ignored once `launcher` is present. `host` overrides the API origin
     when a self-hosted loader is served from elsewhere.
   - Headless mode: `init({ key, showButton: false })`, then call
     `ExponentialWidget.submit({ message, title?, email?, screenshot,
     images, labels })` from the site's own form (`message`: the one form
     field, its first line becomes the issue title unless `title` is
     given; `images`: up to 3 Blobs, 10 MB each; `labels`: ids of the
     widget's configured labels); it resolves with `{ ok, identifier, url,
     emailDelivered }` (`emailDelivered` is null when no email was given).
     Older hosts' `description` is read as the message; a `mode` is
     ignored.

Each submission becomes an issue on the widget's board with an annotated
screenshot, reporter metadata, and page context, atomically. Full widget
API: https://exponential.at/docs/widget/

## More resources

- Docs index: https://exponential.at/docs/ (getting started, issues and
  boards, coding agents, actions, CLI and daemon, feedback and reporters,
  widget, MCP, apps, self-host)
- MCP guide with per-client setup: https://exponential.at/docs/mcp/
- Self-hosting runbook: https://exponential.at/docs/self-host/
- Machine-readable site map: https://exponential.at/llms.txt
