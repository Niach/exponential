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

   An unscoped key acts as the user with their full membership; the mint
   dialog offers the consent screen's team/board scope picker, and a scoped
   key is confined to that pick and works for MCP only. Guard keys
   accordingly. The same key signs the CLI in via `EXP_TOKEN` (below).

## Tools

16 tools, one per noun, all named `exponential_<noun>`. Every call carries an
`op` and that op's fields beside it, e.g. `exponential_issues {op: "get", id:
"ABC-12"}`; each tool's description (from `tools/list`) lists its ops with
their parameters, so read it instead of a static list.

- **issues**: `list` (filters: boards, `status`/`statusId`/`statusCategory`
  each with an `exclude*` twin, priority, assignee, labels any/all/unlabeled,
  `source`, comment activity, created/updated/due ranges, text and
  identifier `search`, plus `sort`; OPEN work only unless `includeClosed` or
  a status filter), `get`
  (UUID or identifier "ABC-12"), `show` (the interactive list in MCP Apps
  hosts), `create`, `update`, `delete`, `activity`, `update_status`,
  `subscribe`, `unsubscribe`, `labels_add`, `labels_remove`,
  `relations_add`, `relations_remove` (`blocks`, `parent`, `duplicate`,
  `related`; `inverse: true` states it the other way round). List ops
  paginate: 50 by default, 200 at most (1000 for issues; comments `list`
  defaults to 100).
- **comments**: `list`, `create`, `update`, `delete`. `create` takes
  `audience: "reporter"` on a widget-filed issue with a reporter email: the
  comment is emailed, `reporterEmailed` says whether the mail went out.
- **attachments**: `list`, `get`, `upload` (bytes, or a signed URL then a
  finalize call; returns the embeddable markdown), `delete`.
- **pr**: `open` (links the session's issues; a stacked base is inferred),
  `merge` (squash through the GitHub App; `stack: true` lands a stack
  member and everything beneath it), `close`, `retarget`, `update`,
  `unlink`, `files`. A PR is named by `pullRequestId`, `issueId` or
  `repositoryId` + `prNumber`.
- **teams** (`list`, `get`, `create`, `update`), **boards** (`list`, `get`,
  `create`, `update`, `delete` = the 48-hour trash, `set_repository`),
  **people** (`list`, `add`, `invites_list`, `invites_create`,
  `invites_revoke`), **team_config** (`labels_*`, `statuses_*` — builtins
  are locked, deleting one in use needs `reassignToId` —, `prompts_list`,
  `prompts_get`, `agents_list`, `mcp_servers_list`/`add`/`remove`,
  `repositories_list`/`add`).
- **apps**: a team's Exponential apps (declarative Exponential UI packages
  rendered in every client): `list`, `get` (the head or a `version`'s
  package), `upsert` (`slug` + `package` publishes a new version; a new slug
  creates the app, `name` required), `delete` (to the trash), `catalog` (the
  components, bindings and functions a package may use; read it before
  writing one).
- **sessions_send**: THE session verb, no op. `{teamId, deviceId, issueIds?,
  text}` starts a thread on an ONLINE device (`exponential_devices
  {op: "list"}` first); `{sessionId, text}` messages a running or idle one;
  `{sessionId, stop: true}` interrupts a turn; `{agentId, text}` messages a
  team agent (the caller's own conversation with it); `{parentId, text}`
  starts a child; `{appId, text}` messages the caller's editor session of an
  app. Every transition of a session comes back as a message.
- **sessions**: `get` (a tree: title, issues, PRs, Guide, children, latest
  messages; no id = the caller's own), `list`, `link`, `unlink`, `end`.
- **session_self** (inside a launcher-started session only): `ask`, `plan`,
  `suggest`, `compact`, `report_bug` (cloud).
- **guide_write**: the thread's one Guide, the body of every PR in it.
- **devices** (`list`, `account_login`), **events** (`emit`, `list`),
  **notifications** (`list`, `mark_read`).
## Core concepts

- **Teams** hold members, boards, labels, statuses, prompts, agents, and
  apps. Roles are owner and member; owners hold the settings and destructive surface.
- **Boards** hold issues. Each board has an identifier prefix; issues get
  identifiers like `ABC-12`, accepted by most tools wherever an issue id
  is. A board optionally links to one GitHub repository, which is what
  enables the coding features.
- **Issues** carry GFM markdown descriptions (plain `@<email>` mentions,
  `#<IDENTIFIER>` issue refs, image embeds), a priority (`none`, `urgent`,
  `high`, `medium`, `low`), labels, an assignee, a due date, comments, and
  attachments. A `#<IDENTIFIER>` ref in a description or comment auto-links
  the two issues as related; `exponential_issues {op: "create"}` takes
  `parentId` to file a sub-issue, and the other relations (blocks,
  duplicate, related) are set with `op: "relations_add"`.
- **Statuses** are per-team rows in six categories (backlog, unstarted,
  started, completed, cancelled, duplicate). Six builtins always exist
  (backlog, in_progress, in_review, done, cancelled, duplicate) and
  teams add custom named statuses. Enum-taking tools accept the builtin
  values; `exponential_team_config {op: "statuses_list"}` + `statusId`
  reach the custom rows.
- **Pull requests**: one branch per session: `exp/<IDENTIFIER>` when the
  session holds exactly one issue, else `exp/session-<id>`. Several issues
  fixed on one pushed branch share one PR via `exponential_pr {op: "open"}`
  with `issueIds` + `head`. PR open moves linked issues to the team's
  configured PR-open status (default In Review);
  merging moves them to the PR-merge status (default Done).
- **Prompts** are a team's saved prompts: a name, an icon, and a markdown
  body, nothing else (no inputs, no triggers, no repository). They live
  under Settings -> Prompts (owners create, edit, and delete; members read)
  and run from the composer's `/` menu, from the CLI's `exponential run`,
  or as `exponential_sessions_send {promptId}` (the body becomes the
  session's first message).
- **Agents** are team definitions under Settings -> Agents: a name, icon
  and colour, a Device to run on (required), an Account (the device's agent
  login; optional), Triggers (each a schedule or an event name; none = the
  agent only waits for messages), and Instructions. Each member talks to an
  agent in their own conversation (`exponential_sessions_send {agentId}`);
  `exponential_events {op: "emit"}` wakes every agent with a trigger on that
  event name.
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
`exponential code EXP-42 --agent claude`), `run <prompt>` (run a team's
saved prompt by id or name), `devices`, `sessions` (list, or
`show|log|message|stop|end <id>`), `mcp list|connect|set-secret|disconnect`
(team MCP servers and your connection to each), `daemon
install` (register a Linux or macOS machine as an always-on agent box,
visible under Devices -> My devices in the web app), `update`.
`code` and `run` share `--agent claude|codex`, `--model`, `--effort`,
`--plan`, `--detach` (run headless but still steerable from the web), and
`--device <label|id>` (start it on another of your devices instead);
`run` also takes `--team <team-id>` and `--prompt <text>` (appended to the
prompt's body in the session's first message). Full
reference: https://exponential.at/docs/cli/

## Integrating the feedback widget for a user

When a user asks to add Exponential feedback collection to their site:

1. Get their widget key (`expw_...`). If they have none, they create a
   widget under Settings -> Widget in Exponential (team owners; every
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
  boards, coding agents, prompts, CLI and daemon, feedback and reporters,
  widget, MCP, apps, self-host)
- MCP guide with per-client setup: https://exponential.at/docs/mcp/
- Self-hosting runbook: https://exponential.at/docs/self-host/
- Machine-readable site map: https://exponential.at/llms.txt
