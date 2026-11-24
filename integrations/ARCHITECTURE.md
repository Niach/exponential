# Exponential in other harnesses

The research behind this document is EXP-1214 (2026-10-07). It answers which
hosts draw Exponential's MCP Apps views, what Claude Code mods and plugins can
do, what a pi extension can draw, and how the integrations stay ONE managed
family. Nothing here changes product behaviour on its own; the work it names
lives on the Integrations board (section 9).

## 1. What every host gets

Every host talks to the same server: `https://<instance>/api/mcp`, streamable
HTTP, one of two credentials:

- an `expu_` personal key as a bearer (full access, the key a person or a
  gateway holds), or
- OAuth 2.1 with PKCE: the server publishes `/.well-known/oauth-protected-resource`
  and `/.well-known/oauth-authorization-server` (dynamic client registration,
  `code_challenge_methods_supported: ["S256"]`, `token_endpoint_auth_methods_supported`
  includes `none`), and every authorization passes the consent page that
  writes the grant (`lib/auth/mcp-authorize-guard.ts`, `lib/emcp/scope.ts`).
  Client ID Metadata Documents are NOT served yet (INT-14).

Five tools carry an MCP Apps binding (`_meta.ui.resourceUri`, the stable
`io.modelcontextprotocol/ui` extension, protocol 2026-01-26), each to a
`ui://exponential/*` resource of MIME `text/html;profile=mcp-app`:

| Tool | Resource | View |
| --- | --- | --- |
| `exponential_issues` op `show` (+ `_meta["openai/ui"].entrypoints: global`) | `ui://exponential/issues` | issue list, opens an issue in place |
| `exponential_sessions` op `get` | `ui://exponential/run` | run report, changes, steer composer |
| `exponential_sessions` op `list` | `ui://exponential/runs` | runs list |
| `exponential_notifications` op `list` | `ui://exponential/inbox` | inbox |
| `exponential_devices` op `list` | `ui://exponential/devices` | devices + agent accounts |

Each resource is its OWN built document (`packages/mcp-apps`, one vite pass
per view, script and stylesheet inlined, nothing shared at runtime; NEW-24)
with the view stamped into its `exp-view` meta tag. Bytes on 2026-11-24
(master `4b3d0b09bf` before, one bundle per view after):

| View | Before | After |
| --- | --- | --- |
| `issues` | 4,535,127 (`app.html`) | 2,349,327 (`issues.html`) |
| `inbox` | 4,535,127 (`app.html`) | 2,347,740 (`inbox.html`) |
| `devices` | 4,535,127 (`app.html`) | 808,753 (`devices.html`) |
| `genui` | 4,814,372 (`genui.html`) | 3,174,425 (`genui.html`) |

The split alone saved nothing: the workspace packages declare no
`sideEffects`, so the `@exp/ui` barrel carried the catalogue's specimens and
every fixture into each document. The views' build marks those modules pure
(`vite.config.ts` `PURE_MODULES`). What is left in `issues` and `inbox` is the
eUI renderer their markdown draws with (the Motion wasm alone is 635 kB
inlined); both are still over OpenClaw's 2 MiB cap.

Its `_meta.ui` carries `prefersBorder: true` and a
CSP of `resourceDomains: [app origin, fonts.googleapis.com, fonts.gstatic.com]`;
no `connectDomains`: data only ever arrives through the host's `tools/call`.

A request that carries a well-formed `X-Exp-Session-Id` header (every coding
run Exponential launches) gets NO resources and NO bindings
(`lib/emcp/server.ts`, `lib/emcp/tools.ts`, EXP-1212). That rule stays until
Exponential renders MCP UI natively through its own renderers (VAPP-96, A2UI,
no webview).

Probe it yourself (read-only):

```bash
# anonymous: 401 + the protected-resource pointer
curl -si -X POST https://app.exponential.at/api/mcp | grep -i www-authenticate
# with a key: the five resources and one view's size
post() { curl -s https://app.exponential.at/api/mcp -X POST \
  -H "Authorization: Bearer $EXP_TOKEN" -H 'content-type: application/json' \
  -H 'accept: application/json, text/event-stream' --data "$1"; }
post '{"jsonrpc":"2.0","id":1,"method":"resources/list","params":{}}'
post '{"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"ui://exponential/issues"}}' | wc -c
```

## 2. Host matrix (verified 2026-10-07)

"Draws" means the host fetches the `ui://` resource a tool names and mounts
it. "Compatible" means the protocol facts above match what the host documents;
end-to-end rendering in the hosts marked "untested" still needs a person's
account (the checklist is in section 7).

| Host | Draws MCP Apps | Facts | Our views |
| --- | --- | --- | --- |
| claude.ai, Claude Desktop **Chat**, Claude iOS/Android | Yes | Official support matrix. Double iframe (mobile: native WebView), honours `_meta.ui.csp`, asks the person to Allow each server's app once, passes style variables + `hostContext.theme` (light and dark), display modes `inline`/`fullscreen`/`pip`, tool results over ~150k chars are spilled to the sandbox FS and never reach the app. `ui.domain` is optional (only an app running its OWN OAuth needs it; value = first 32 hex of sha256(server URL) + `.claudemcpcontent.com`). The connector must be OAuth; the server is added under Customize → Connectors (Pro/Max/Team/Enterprise). | Compatible via DCR; untested end-to-end. Known gap: the views force dark (`app.tsx` adds `html.dark`) and ignore the host's tokens, which clashes in light mode. |
| Claude Code: CLI, IDE extensions, **Desktop Code tab** | **No** | anthropic/claude-code#95149 (open, 2026-09-17): Claude Code drops the UI and shows the text result; image blocks render, HTML does not. The Code tab adds connectors and plugins, not app rendering. | Text only. The answer for Claude Code is a mod (section 4), drawn from the same data. |
| Claude Cowork | Not documented | Treat as no. | |
| ChatGPT (web, desktop, mobile) | Yes | Standard MCP Apps keys plus `openai/*` aliases; our `openai/ui` global entrypoint applies. OAuth 2.1 mandatory, PKCE S256 advertised, **CIMD preferred, DCR fallback**; redirect `https://chatgpt.com/connector_platform_oauth_redirect`. Added via Developer mode or the app directory (OpenAI review). | Compatible via DCR; untested. No "ChatGPT plugin" exists or is needed: the server IS the app. Distribution is a plugin bundle (section 5) + directory submission. |
| Codex CLI / Codex app / IDE extension | **No** | openai/codex#21019: rendering sits behind `enable_mcp_apps`; tools and resources work. Codex plugins (skills + MCP + hooks) load in Codex CLI/app and ChatGPT, not the IDE extension. | Text only. |
| Cursor ≥ 2.6 (2026-03-03) | Yes | Sandboxed iframe + postMessage; `structuredContent` since 3.0; team marketplaces for plugins. | Compatible; untested. |
| VS Code GitHub Copilot Chat | Yes | Official support matrix. | Compatible; untested. |
| OpenClaw (Control UI) | Yes, `mcp.apps.enabled` | Double-iframe proxy on a separate sandbox origin, resources capped at **2 MiB**, 4 requests in flight, 120 requests/min, 30 tool calls/min per view, app HTML + results live in a **10-minute in-memory view lease**. An expired transcript preview shows a recovery message; a pinned or reconstructed card is **read-only** (the host refetches the `ui://` document but grants no tool calls); entrypoint panels offer Relaunch. | The only host EXP-1183 tested. After the lease our view sits on its skeleton (it never receives a `tool-result`) or shows tool errors on interaction: INT-12. |
| pi (coding agent) | No MCP natively | MCP arrives through extensions (`pi-mcp-extension` and friends: tools only, resources "v2"). pi's own TUI is rich: widgets, overlays, `SelectList`, `Markdown`, `Image` (Kitty/iTerm2). | A pi extension draws our data with pi-tui components (section 6). |
| Goose, Postman, MCPJam, Microsoft 365 Copilot, Archestra, PostHog Code | Yes (matrix) | | Compatible; untested. |

Sources: modelcontextprotocol.io/extensions/apps and /extensions/client-matrix;
claude.com/docs/connectors/building/mcp-apps/{getting-started,design-guidelines,troubleshooting};
code.claude.com/docs/en/{desktop,mcp,plugins-reference,plugins/publish,plugins/cli-hints,plugins/mods/reference};
claude.com/docs/plugins/platform-support; developers.openai.com/apps-sdk/{mcp-apps-in-chatgpt,build/auth};
github.com/openai/plugins; cursor.com/changelog/2-6; docs.openclaw.ai/cli/mcp/apps,
/plugins/bundles, /clawhub/publishing; github.com/earendil-works/pi
(`docs/extensions.md`, `docs/tui.md`, `docs/packages.md`); the GitHub issues named above.

## 3. The shape of the family

One server, one playbook, one MCP entry, one set of view-models. Each host
gets a thin, GENERATED adapter, the way icons, emoji and design tokens already
work: a source, a generator, a drift gate.

```
apps/web  /api/mcp ·························· the ONE server: tools, ui:// views, OAuth (DCR), expu_ keys
packages/mcp-apps ··························· the HTML views (MCP Apps 2026-01-26) for hosts that draw HTML
packages/plugin-kit (@exp/plugin-kit) ······· the source every adapter is generated from (NEW-23)
  src/mcp-entry.json + mcp-entry.ts          url, bearer header, X-Exp-Session-Id, key env; one renderer per host
  src/playbook/core.md + playbook.ts         the ONE skill body; a host adds its description and its own paragraphs
  src/model/                                 the view-models: core/ (dependency-free: wire rows, labels, status
                                             glyph + tone, PR line, session state, inbox toasts, typed tool
                                             callers) + the list files the HTML views also read
  src/theme/                                 design tokens → the OpenClaw theme, the tone colours of core/
  src/generate.ts                            writes integrations/*; `bun run plugin-kit:generate`
integrations/
  claude/        the Claude plugin (chat, Cowork, the directory): connector + skills, no code
  claude-code/   the Claude Code plugin: .mcp.json, skills/, hooks/ (the mods), its own marketplace.json
  openclaw/      manifest + skill + theme
apps/desktop/crates/coding ·················· stays the launcher for Exponential's own sessions (untouched)
```

Drift gates: `packages/plugin-kit/src/generate.test.ts` (release suite) and
`apps/web/src/lib/plugin-kit.test.ts` (every pull request) fail when an
adapter file differs from what the sources render; `mcp-entry.test.ts` pins
the launcher's Rust constants and the server's session header to the entry.
pi and the Codex/ChatGPT bundle (sections 5 and 6) are not built.

Rules:

- Hosts that draw HTML get `packages/mcp-apps`. Hosts that draw their own
  elements (Claude Code mods, pi-tui) get the same view-models rendered with
  their element table. There is no second data layer anywhere.
- The playbook has one source, `packages/plugin-kit/src/playbook/core.md`.
  Every `skills/exponential/SKILL.md` under `integrations/` is that body
  with the host's own paragraphs; nobody edits one by hand. The launcher's
  runbook (`crates/coding/src/runbook.md`) is for Exponential's own sessions
  and stays separate; `MCP_SERVER_INSTRUCTIONS` stays the only paraphrase.
- The MCP entry has one source, `mcp-entry.json`. The OpenClaw manifest and the
  Claude plugins' `.mcp.json` derive from it; the Rust launcher keeps its constants and
  a test pins them to the JSON.
- A foreign harness that runs an issue carries the same `X-Exp-Session-Id`
  the launcher stamps, minted by `codingSessions.start`. With that header the
  server behaves exactly as for a native run: session-scoped tools on, views
  off.
- Theme: the HTML views adopt the host's style variables when a host passes
  them (Claude, ChatGPT) and fall back to Exponential's dark palette; mods, pi
  widgets and the OpenClaw theme come from design tokens.
- Distribution per host is a generated artefact plus a publish step (section 5).

## 4. Claude Code: plugin + mods

A Claude Code plugin is a folder with `.claude-plugin/plugin.json` and, at the
root, `skills/`, `commands/`, `agents/`, `hooks/hooks.json`, `.mcp.json`,
`bin/`, `userConfig`. `.mcp.json` takes an `http` server; Claude Code discovers
OAuth (DCR or CIMD) on the first 401, or substitutes `${user_config.api_key}`
into a header, or runs a `headersHelper` for dynamic headers.

A **mod** is a plugin whose `hooks/hooks.json` names a hooks module
(`{ "modules": ["./register.tsx"] }`) exporting `register(on, options)`; every
hook is `($, e, next)`. It draws at render sites (`Pane`, `AbovePrompt`,
`UserMessage`, `AssistantMessage`, `ToolUse`/`ToolResult`, `CommandOutput`,
`Spinner`, `SessionMode`, `PromptHint`) with the surface's element table
(`Box`, `Text`, `Button`, `Link`, `Markdown`, `Input`, `Select`, `Client`;
`Svg` on the desktop; `Image`/`Raster` on the terminal). It reaches data
through `$.mcp.call('exponential', tool, args)` (the session's own MCP
connection, no permission prompt) and `$.http.fetch`, keeps values in
`$.state`/`$.store`, polls with `$.clock.every`, and shows `$.ui.status`,
`$.ui.toast`, `$.ui.open` panes. Surfaces: the terminal and the Desktop Code
tab (the types also name `vscode` and `mobile`). There is no DOM and no
webview: the HTML views never load in a mod; the mod renders the view-models.
Limits: 10 s per hook, 4 MiB `$.store`, 100k characters per tree.

The Exponential plugin for Claude Code (`integrations/claude-code`, plugin
`exponential-code`, NEW-23; built against Claude Code 2.1.308):

- `.mcp.json`: the generated entry, OAuth on the first 401;
  `userConfig.instance` names a self-hosted origin. A server the person added
  under the name `exponential` with a key serves the mods too (headless).
- `skills/exponential/SKILL.md`: the generated playbook.
- `hooks/register.tsx`, one hooks module, every word and colour from
  `hooks/kit/` (the generated copy of plugin-kit's `model/core`):
  - **Issue pane** (`$.ui.open`, a `ui.render` hook on `Pane`): the issue the
    branch names (an identifier anywhere in it: `exp/EXP-12`,
    `feature/exp-12-export`; a session branch names none) or the one pinned
    with `/exp-issue EXP-12`, with its pull request and its live sessions
    from `exponential_issues` op `get` (`pullRequests`, `sessions`), re-read
    every 30 s. `/exp-issues` lists the open issues by status; a row opens
    its issue.
  - **Status line** (`$.ui.status`): `EXP-12 · PR #40 · Checks running`, the
    header pill's label as the server words it (`pullRequests[].status`).
  - **Toasts** (`$.ui.toast`): a new unread comment, mention, reporter reply
    or waiting session from `exponential_notifications` op `list`, polled
    every 60 s; the newest row seen is kept in `$.store`, shared by every
    session on the machine. A `tool.call` hook toasts the link of a PR
    opened or queued through `exponential_pr_write`.
  - `/exp-start EXP-12` fills the prompt with the request to start a coding
    agent on the issue (a command's hook cannot submit a prompt itself).
- A command a mod registers takes letters, digits, `_` and `-` only, so the
  commands are `/exp-issues`, `/exp-issue`, `/exp-start`, not `/exp:…`.
- `tests/mods.test.tsx` runs in the engine's own test host
  (`claude plugin test`); the gate's `plugins` job runs it and
  `claude plugin validate --strict` with a pinned CLI.

Distribution: the plugin folder carries its own
`.claude-plugin/marketplace.json` (`claude plugin marketplace add
./integrations/claude-code` from a checkout). Publishing it (a marketplace
file at the repository root, the public mirror the Claude plugin uses, or a
submission to Anthropic's directory) is a person's decision and not done.
The Claude plugin (`integrations/claude`: connector + skills, no code) is the
directory listing for claude.ai, Cowork and Claude Code; on claude.ai chat
only skills and the remote MCP server load, so the mods live in their own
plugin. The `claude-code-hint` install prompt works only for plugins in an
official Anthropic marketplace.

## 5. ChatGPT and Codex

There is no ChatGPT-specific plugin to build: the MCP server with its MCP
Apps views IS the ChatGPT app (ChatGPT reads the standard `_meta.ui` keys and
our `openai/ui` entrypoint). What ChatGPT adds is distribution and auth:

- A ChatGPT/Codex **plugin bundle** (github.com/openai/plugins):
  `.codex-plugin/plugin.json` (name, version, description, `interface`:
  displayName, developerName, category) plus `skills/` and `.mcp.json`, listed
  in a `marketplace.json` (`.agents/plugins/marketplace.json` in a repository)
  or submitted to the universal ChatGPT/Codex directory. It loads in ChatGPT
  (web, desktop, mobile) and Codex CLI/app, not the IDE extension. Generated
  from plugin-kit: the same skill and the same MCP entry as the Claude plugin,
  no hooks, no UI code.
- Auth: ChatGPT requires OAuth 2.1 with PKCE and prefers CIMD over DCR; our
  server passes with DCR today and gains CIMD with INT-14.
- Codex draws no MCP Apps today, so the bundle gives Codex users text tools
  and the playbook; ChatGPT users get the HTML views from the same bundle.

## 6. pi: Exponential inside pi

pi is never an Exponential agent (EXP-849 retired that); this is the inverse:
an Exponential package a pi user installs (`pi install npm:@exponential/pi`,
`package.json` `pi: { extensions, skills, themes }`).

What a pi extension can draw (pi `docs/extensions.md`, `docs/tui.md`):

- persistent widgets above or below the editor, `ctx.ui.setWidget(key,
  component)`, a component being `render(width): string[]` + `handleInput` +
  `invalidate`; it stays across turns and redraws whenever the extension
  calls `tui.requestRender()` from a timer or an event;
- focused overlays, `ctx.ui.custom(factory, { overlay: true, anchor, width,
  height, closeOnEscape })`, which hold the keyboard until done;
- `ctx.ui.setStatus(key, text)`, `ctx.ui.notify`, `select`/`confirm`/`input`;
- pi-tui components: `SelectList`, `SettingsList`, `ScrollView`,
  `VStack`/`HStack`, `Markdown`, `Input`, `Editor`, `Image` (Kitty and iTerm2
  graphics), `Loader`, `MouseRegion`; theme tokens (`theme.fg('success', …)`,
  `theme.appearance` dark/light).

The Exponential package (`integrations/pi`):

- `/exp issues`: issue list widget (SelectList over the shared view-models),
  Enter opens the detail overlay (status and priority pickers, comments,
  Start coding), pictures through `Image`.
- run status widget + PR state in `setStatus`; `notify` on a reviewer comment.
- the playbook injected through `before_agent_start`; MCP tools through
  `pi-mcp-extension` (`~/.pi/agent/mcp.json`, streamable-http) or a small
  bridge of our own.
- `/exp start <IDENT>`: the start flow in TypeScript, mirroring
  `coding::prepare`: `codingSessions.start` (a device-less row), `git worktree
  add` from the board's default branch on `exp/<IDENT>`, status
  `in_progress`, the MCP entry with the new `X-Exp-Session-Id`, and
  `exponential_pr` op `open` at the end.

Server prerequisite (its own issue): **external-harness sessions**. Today
`coding_sessions.agent` only accepts `claude`/`codex`, `coding-session-sweep.ts`
DELETES a device-less row after 2 h without a heartbeat, and the attended
`sessions_end` compatibility gate reads the host device's version. A row
started by a foreign harness needs an accepted agent id, a heartbeat from the
extension, a sweep exemption and the `sessions_end` gate.

## 7. Verification checklist (needs a person's accounts)

For each host, connect prod `/api/mcp` and ask for the issue list:

1. **claude.ai / Claude Desktop**: Customize → Connectors → Add custom
   connector → `https://app.exponential.at/api/mcp` → consent page → ask
   "show my open issues" → Allow the app. Check: renders, light-mode legibility,
   row opens an issue, Start coding, fullscreen. Developer tools: Help →
   Troubleshooting → Enable Developer Mode, inspect the inner iframe.
2. **ChatGPT**: Settings → Apps → Developer mode → add the server (OAuth) →
   same prompt. Check the global entrypoint opens without a model call.
3. **Cursor ≥ 2.6** and **VS Code Copilot Chat**: add the server as a remote
   MCP server, same prompt.
4. **OpenClaw**: `openclaw plugins install clawhub:@exponential/openclaw-plugin`,
   `openclaw config set mcp.apps.enabled true --strict-json`; wait 10 minutes,
   reopen the card: the read-only state.

Record the outcome per host in the matrix above with the date.

## 8. Later, not now

- `exponential prepare <ISSUE> --json`: expose `coding::prepare` from the CLI
  so the pi start flow and the Claude plugin's `/exp:start` call one binary
  instead of re-implementing the launcher in TypeScript.
- An Exponential TUI (`crates/tui`, `exponential tui`): the Rust `sync`
  (Electric → SQLite), `domain` (nesting, search, fold, rail, board, diff) and
  `steer::feed` crates are gpui-free and the CLI ships a subcommand for free;
  missing are a gpui-free collections layer, a terminal UI crate, a glyph map,
  a terminal markdown renderer and a port of the desktop query layer. Deferred
  on 2026-10-07.

## 9. The issues

Integrations board, filed from EXP-1214 on 2026-10-07.

- **INT-1** `@exp/plugin-kit`: one source for every harness adapter
  - INT-3 Claude Code plugin (`integrations/claude-code`)
  - INT-4 Mod: issue pane · INT-5 Mod: PR status line · INT-6 Mod: reviewer toast
  - INT-7 pi package `@exponential/pi`
  - INT-8 Server: external-harness sessions
  - INT-9 Codex/ChatGPT plugin bundle (`integrations/codex`)
  - INT-10 Distribution: Anthropic directory, ChatGPT/Codex directory, ClawHub, marketplaces
- **INT-2** MCP Apps views for every host
  - INT-11 Host theme + display modes
  - INT-12 Read-only mode after the host's lease expiry
  - INT-13 Verification matrix (needs a person's accounts)
  - INT-14 CIMD on the authorization server
  - INT-15 One bundle per view
