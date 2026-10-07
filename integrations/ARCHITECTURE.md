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
  writes the grant (`lib/auth/mcp-authorize-guard.ts`, `lib/mcp/scope.ts`).
  Client ID Metadata Documents are NOT served yet (INT-14).

Five tools carry an MCP Apps binding (`_meta.ui.resourceUri`, the stable
`io.modelcontextprotocol/ui` extension, protocol 2026-01-26), each to a
`ui://exponential/*` resource of MIME `text/html;profile=mcp-app`:

| Tool | Resource | View |
| --- | --- | --- |
| `exponential_issues_show` (+ `_meta["openai/ui"].entrypoints: global`) | `ui://exponential/issues` | issue list, opens an issue in place |
| `exponential_sessions_get` | `ui://exponential/run` | run report, changes, steer composer |
| `exponential_sessions_list` | `ui://exponential/runs` | runs list |
| `exponential_notifications_list` | `ui://exponential/inbox` | inbox |
| `exponential_devices_list` | `ui://exponential/devices` | devices + agent accounts |

The resource is ONE built document (`packages/mcp-apps`, 1,055,648 bytes on
prod on 2026-10-07, script and stylesheet inlined) with the view stamped into
its `exp-view` meta tag. Its `_meta.ui` carries `prefersBorder: true` and a
CSP of `resourceDomains: [app origin, fonts.googleapis.com, fonts.gstatic.com]`;
no `connectDomains`: data only ever arrives through the host's `tools/call`.

A request that carries a well-formed `X-Exp-Session-Id` header (every coding
run Exponential launches) gets NO resources and NO bindings
(`lib/mcp/server.ts`, `lib/mcp/tools.ts`, EXP-1212). That rule stays until
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
apps/web  /api/mcp ·························· the ONE server: tools, ui:// views, OAuth (DCR, later CIMD), expu_ keys
packages/mcp-apps ··························· the HTML views (MCP Apps 2026-01-26) for hosts that draw HTML
packages/plugin-kit (@exp/plugin-kit, new) ·· the source every adapter is generated from
  src/mcp-entry.json                         url, bearer header, X-Exp-Session-Id, OAuth hint
  src/playbook/                              SKILL.md generated from crates/coding/src/skill.md
  src/model/                                 view-models moved out of packages/mcp-apps
  src/client/                                typed callers for the tools the views and mods use
  src/theme/                                 design tokens → OpenClaw theme, mod colours, pi theme
  scripts/generate.ts                        writes integrations/*; `bun test` gates drift
integrations/
  openclaw/      (exists) manifest + skill + theme, regenerated from plugin-kit
  claude-code/   plugin: plugin.json, marketplace.json, .mcp.json, skills/, hooks/{hooks.json,register.tsx}
  pi/            npm package @exponential/pi: extension.ts (commands, widgets, overlays, start flow), skills/, theme
  codex/         ChatGPT/Codex plugin bundle: .codex-plugin/plugin.json, skills/, .mcp.json
apps/desktop/crates/coding::prepare ········· stays the launcher for claude/codex runs (untouched)
```

Rules:

- Hosts that draw HTML get `packages/mcp-apps`. Hosts that draw their own
  elements (Claude Code mods, pi-tui) get the same view-models rendered with
  their element table. There is no second data layer anywhere.
- The playbook has one source, `apps/desktop/crates/coding/src/skill.md`
  (6 KiB cap, gated by `context-budget.test.ts`). Every `SKILL.md` under
  `integrations/` is generated from it with a per-host preface; nobody edits
  one by hand. `MCP_SERVER_INSTRUCTIONS` stays the only paraphrase.
- The MCP entry has one source, `mcp-entry.json`. The OpenClaw manifest, the
  Claude plugin `.mcp.json`, pi's `mcp.json`, the Codex `.mcp.json` and the
  marketing snippets derive from it; the Rust launcher keeps its constants and
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

The Exponential plugin (`integrations/claude-code`):

- `.mcp.json`: the generated entry, OAuth by default, optional `api_key` in
  `userConfig` for headless use.
- `skills/exponential/SKILL.md`: the generated playbook.
- commands: `/exp:issues` (list, drawn as a tree at the `CommandOutput` site),
  `/exp:issue <IDENT>`, `/exp:start <IDENT>`.
- mods, each a separate issue:
  - **Issue pane**: the issue of the current `exp/<IDENT>` branch
    (`$.session.repo()` / `git branch --show-current`) with its run status,
    polled through `exponential_sessions_get`; opens on `/exp:issues`.
  - **Status line**: the run's PR state from `exponential_issues_get`
    (`prState`, `prUrl`), `$.ui.status`.
  - **Toast**: a new reviewer comment or review notification from
    `exponential_notifications_list`, `$.ui.toast`; a `tool.call` hook on
    `mcp__exponential__exponential_pr_open` toasts the PR link.

Distribution: a `.claude-plugin/marketplace.json` in a git repository
(`claude plugin marketplace add Niach/<repo>`, `claude plugin install
exponential@<marketplace>`), and a submission to Anthropic's directory
(GitHub repository + a paid claude.ai plan; one listing reaches claude.ai,
Cowork and Claude Code, and lists the MCP server as a connector, which is how
claude.ai users get the HTML views). On claude.ai chat only skills and the
remote MCP server load; hooks, mods and `bin/` are ignored there. The
`claude-code-hint` install prompt from a CLI works only for plugins in an
official Anthropic marketplace, so it does not apply.

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
  `exponential_pr_open` at the end.

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
