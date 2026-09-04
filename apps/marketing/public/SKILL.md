---
name: exponential
description: >
  Work with Exponential (exponential.at), an open-source realtime issue
  tracker with local coding agents, a team helpdesk, and an embeddable
  feedback widget. Use this to connect over MCP, manage teams, boards,
  issues, labels, statuses, comments, and pull requests, install and
  script the exponential CLI, or integrate the feedback widget on a
  user's website.
---

# Exponential

Exponential is an open-source (Apache-2.0) realtime tracker for issues,
customer support, and coding agents. Teams file issues on boards, hand them
to AI coding agents (Claude Code, Codex, or pi) that run locally on the
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
   a key under Settings -> API keys in the web app (prefix `expu_`; the raw
   key is shown exactly once at mint time). Send it as either header:

       Authorization: Bearer expu_...
       x-api-key: expu_...

   API keys act as the user with their full membership. Guard them
   accordingly. The same key signs the CLI in via `EXP_TOKEN` (below).

## Tool families

Around 75 tools, all named `exponential_<family>_<verb>`:

- **teams**: list, get, create, update the user's teams.
- **boards**: CRUD boards inside a team; a board can be backed by a GitHub
  repository (`boards_set_repository`).
- **issues**: list and filter (boards, `statusId`/`statusCategory`,
  priority, assignee, labels any/all/unlabeled, comment activity,
  created/updated ranges, title search — each with an `exclude*` twin —
  plus `sort`, where a `-` prefix descends), get by UUID or identifier
  ("ABC-12"), create, update, delete, update_status, subscribe,
  unsubscribe. Every list tool paginates: 50 by default, 200 at most.
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
- **comments**: list, create, update, delete on issues.
- **notifications**: list, mark read.
- **members** and **invites**: list team members (resolve assignee ids),
  manage invite links (owner only).
- **repositories**: list and register GitHub repos; `branch_diff` diffs an
  issue's branch against the repo default branch.
- **actions**: CRUD reusable team prompts that members run locally.
- **attachments**: upload, get, delete images; upload returns the
  embeddable markdown form.
- **automations**: list, create, update, toggle, delete the runs bound to
  one action and one device. `trigger` is either
  `{kind:"schedule", interval:"daily"|"weekly"|"monthly", minuteOfDay,
  weekday?, dayOfMonth?}` (the device's local clock) or
  `{kind:"event", event:"created"|"status_changed"|"assignee_changed"|
  "label_added"|"priority_changed"|"pr_opened"|"pr_merged", filters?}`.
  Writes are owner-only, and an ENABLED automation needs every input of
  its action to be optional.
- **sessions**: list, get, message (steer), kill, and start a coding,
  action or chat session on one of the user's own machines. A start
  targets an ONLINE device (`devices_list` first) — offline devices are
  refused, never queued. Inside a launcher-started run two more tools
  register: `sessions_end` (the run's own close-out summary, unattended
  runs only) and `sessions_ask_parent` (ask the run that started this one).
- **devices**: `devices_list` shows the user's machines, their online state
  and the agent CLIs each one can run.
- **helpdesk**: list and read support threads, reply, add an internal note,
  close, reopen, escalate a ticket into an issue.
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
  attachments.
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
- **Actions** are reusable markdown prompts (up to 10 typed inputs:
  `text`, `textarea`, `repo`, `board`, `pr`, `icon`) that members run as
  local agent sessions from the desktop app, the web, or the CLI. An
  **automation** is a separate row binding one action to one device and a
  schedule-or-event trigger; the bound machine starts the run itself, so
  nothing fires while it is off.
- **Coding agents**: sessions run Claude Code, Codex, or pi locally with
  the Exponential MCP server wired in automatically. pi has no native MCP
  support, so the launcher injects a small pi extension that bridges every
  `exponential_*` tool over HTTP; from the agent's point of view the tools
  look the same in all three.
- **Feedback and support**: the embeddable widget files issues from any
  website; in support mode it opens email-conversation tickets in the
  team's Support inbox (helpdesk).

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
visible under Devices -> My machines in the web app), `update`.
`code` and `run` share `--agent claude|codex|pi`, `--model`, `--effort`,
`--plan`, and `--detach` (run headless but still steerable from the web);
`run` also takes `--team <id>` and repeated `--input k=v`. Full
reference: https://exponential.at/docs/cli/

## Integrating the feedback widget for a user

When a user asks to add Exponential feedback collection to their site:

1. Get their widget key (`expw_...`). If they have none, they create a
   widget in Team settings -> Feedback widget in Exponential (team owners; every
   plan includes at least one), pick the target board, and add their
   site's domain to the allowlist (submissions are only accepted from
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
     `ExponentialWidget.submit({ title, description, screenshot, images,
     labels })` from the site's own form (`images`: up to 3 Blobs, 10 MB
     each; `labels`: ids of the widget's configured labels); it resolves
     with `{ ok, identifier, url }`.

Each submission becomes an issue on the configured board with an annotated
screenshot, reporter metadata, and page context, atomically. Full widget
API: https://exponential.at/docs/widget/

## More resources

- Docs index: https://exponential.at/docs/ (getting started, issues and
  boards, coding agents, actions, CLI and daemon, feedback and helpdesk,
  widget, MCP, apps, self-host)
- MCP guide with per-client setup: https://exponential.at/docs/mcp/
- Self-hosting runbook: https://exponential.at/docs/self-host/
- Machine-readable site map: https://exponential.at/llms.txt
