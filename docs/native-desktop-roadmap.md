# Native Desktop App — Roadmap & Handoff

Status as of 2026-06-02. This doc is the single source of truth for the native
desktop effort: what's **done**, the **locked architecture**, the exact
**agent-core C ABI + protocols** both platforms share, and the **remaining plan**
— sequenced so the **macOS track can be built next** without re-deriving anything.

The original high-level plan lives at
`~/.claude/plans/please-help-me-create-serene-octopus.md`; this file supersedes it
for execution detail.

---

## 1. Goal

Replace the (now-deleted) `apps/companion` Bun daemon with native desktop apps that
(a) are full issue-tracker clients and (b) run coding agents (`claude`/`codex` CLI)
on assigned issues inside an embedded terminal, registering the machine as a
"desktop agent" via the existing `workspace_agents` flow.

- **macOS** — Swift + SwiftUI (extends the existing `apps/ios` Tuist project).
- **Linux** — Zig + GTK4/libadwaita (`apps/linux`). ✅ **built & working.**
- **Shared agent loop** — Rust `crates/agent-core` (cdylib + C ABI). ✅ **done.**

## 2. Locked architecture (do not re-litigate)

- **Shared agent-core = Rust**, built as `cdylib` + `staticlib` exposing a **C ABI**
  (`crates/agent-core/include/agent_core.h`). macOS consumes it via a clang module
  map + a thin Swift wrapper; Linux consumes it via hand-declared `extern` (Zig
  0.16 translate-c can't parse the header's deps, so Linux hand-declares — macOS
  should use the clang module map since Swift/Clang handle it fine).
- **Asymmetric scope.** The Rust core is **only the agent loop**. The tracker
  data/sync layer is **per-platform**: macOS reuses the proven **iOS Swift**
  sync/data layer (extracted into `ExpCore`); Linux has its **own Zig** sync engine.
  The two sync engines stay aligned via `packages/electric-protocol/fixtures` +
  `packages/domain-contract` (same as iOS/Android already do).
- **The core never spawns the CLI.** When the pipeline needs an agent run it emits
  a `run_request` event; the GUI runs `claude`/`codex` in a visible terminal and
  calls `agent_core_submit_run_result(...)` back. See §4.
- **macOS = extend `apps/ios/Project.swift`** (add an `ExpCore` framework + macOS
  app targets), NOT a separate project.

## 3. What's DONE

### Rust `crates/agent-core` — complete (60 tests, 0 warnings)
Full companion loop ported & per-module tested, wired behind the C ABI:
`pipeline` (decision brain: `decide_stage`, `### PLAN`/`### QUESTIONS` parse,
`PLAN_REVISION_CAP=8`, prompt builders) · `state` (rusqlite WAL store) · `trpc`
(ureq; `companion.*` routes) · `electric` (assigned-issues long-poll, replays the
shared fixtures) · `mcp` (stateless Streamable-HTTP `/api/mcp` — verified: bare
`tools/call`, no initialize) · `dispatcher` (threaded queue/concurrency/dedup/
re-entry gating) · `git` · `github` (REST) · `mcp_config` (claude `.mcp.json` /
codex `config.toml`) · `agent_run` (run_request↔submit handshake, safe CLI flags) ·
`run_pipeline` · `pr_poll` · `ffi`. Dep stack: threaded (no tokio), `ureq` +
native-tls (system openssl), `rusqlite` bundled, serde.

### Linux app `apps/linux` — v1 + agent, working & verified
Own Zig sync engine (10 Electric shapes), gated multi-instance login (password +
OIDC + Google), full tracker (grouped/filtered/searchable list, inline-editable
detail, comments, labels, attachments, create/edit, workspace settings), desktop-
agent registration + 30s heartbeat, GitHub device-flow OAuth, and the **embedded
libghostty terminal** that runs agent CLIs visibly (M7) with output/exit captured
back to the core (M8 parity tests done). See [agent-core C ABI](#4-the-agent-core-c-abi--protocols-shared)
and the libghostty build/gotchas in `apps/linux/scripts/build-libghostty.sh` +
the `project_libghostty_embed` memory.

### `apps/companion` — DELETED
Removed (user-approved). Server `lib/trpc/companion/*` routes stay (the desktop
agent uses the same `companion.*` tRPC names). The `agentPlan.*` routes + the
`workspace_agents`/`apikeys`/`setupTokenHash` model are unchanged.

## 4. The agent-core C ABI + protocols (shared, ground truth)

Header: `crates/agent-core/include/agent_core.h`. Ground-truth impl:
`crates/agent-core/src/ffi.rs`. **Exported functions:**

```c
AgentCore *agent_core_create(const char *config_json);
void  agent_core_set_event_callback(AgentCore*, void *ctx, AgentCoreEventCallback cb);
int   agent_core_start(AgentCore*);
int   agent_core_stop(AgentCore*);
void  agent_core_free(AgentCore*);
int   agent_core_claim_setup(const char *base_url, const char *setup_token, char **out_json);
int   agent_core_github_device_login(AgentCore*);
int   agent_core_uninstall(AgentCore*);
int   agent_core_submit_run_result(AgentCore*, const char *run_id, int exit_code, const char *final_text);
int   agent_core_cancel_run(AgentCore*, const char *run_id);
void  agent_core_string_free(char *s);
// callback: void (*)(void *ctx, const char *event_json, size_t len)  // borrowed, copy it
```

**`config_json` (camelCase — the actual `CoreConfigDto`, the header comment is
slightly stale):**
`baseUrl`, `apiKey` (the agent `expk_` key), `botUserId`, `githubToken`,
`reposRoot`, `worktreesRoot`, `branchPrefix` (default `"agent"`), `driver`
(default `"claude"`), `dbPath`, `maxConcurrent` (default 2), `timeoutS` (default 30).

**Outbound event JSON** (`{"type": "...", ...}`): the one that matters is
`run_request`:
```json
{ "type":"run_request", "runId":"run-N", "cwd":"…", "mode":"plan|code",
  "program":"claude|codex", "argv":["…"], "env":{"K":"V"},
  "mcpConfigPath":"…", "systemPrompt":"…", "userPrompt":"…" }
```
Other event types (`log`, etc.) can be surfaced in the UI; only `run_request`
requires a response.

**The run handshake** (this is the heart of M7 on each platform):
1. Core emits `run_request`; the pipeline thread **blocks** in `request_run`.
2. Host (on its UI thread) launches the CLI **in a visible terminal** with
   `program` + `argv` + the combined prompt + `env` + `cwd`, and captures stdout.
3. On child exit, host calls `agent_core_submit_run_result(core, runId, exitCode, finalText)`
   → unblocks the pipeline thread.

**Reference implementation to mirror:** `apps/linux/src/core/agent/agent_manager.zig`
(marshals the request to the UI thread, writes a per-run bash wrapper
`<program> <argv…> "$(cat promptfile)" 2>&1 | tee outfile; echo ${PIPESTATUS[0]} > codefile`,
runs it in the embedded terminal, polls for exit, reads the files, submits). The
prompt goes via a **file** (not the command string) and the real exit code comes
from **PIPESTATUS** — see the `project_libghostty_embed` memory for why.

> Note: Linux does `claimSetup` / device-login / `uninstall` in Zig directly
> (`registration.zig`, `github_auth.zig`) rather than via the C functions. macOS
> can do likewise in Swift OR use `agent_core_claim_setup` / `_github_device_login`
> / `_uninstall`. Either is fine; the C ones exist.

**Server contract (companion.*, unchanged):**
`companion.create {workspaceId,name}` → `{agent, setupToken("expc_…"), installCommand}`;
`claimSetup {setupToken}` → `{apiKey:string (BARE STRING, not {key}), agent{id,userId,name}, workspace{id,slug,name}, projects[], oauth{githubClientId}}`;
`heartbeat()` (30s); `pollControl {activityCursor?}`; `reportGithubIdentity {login,repos[]}`;
`uninstallSelf()`; `setup.revoke {agentId}` (owner). Credentials: create = human
session; claim = none (public); heartbeat/uninstall/poll = `expk_`.

## 5. libghostty embedding — platform notes

The terminal is the one genuinely platform-specific piece of the agent UI.

- **Surface config** `ghostty_surface_config_s` is shared; only the platform union
  differs: Linux `platform.linux = { void* gl_area }` (a `GtkGLArea`), macOS
  `platform.macos = { void* nsview }` (an `NSView`, Metal-rendered). iOS uses
  `uiview`.
- **App lifecycle is identical** across platforms: `ghostty_init` → `ghostty_config_new`
  + load + finalize → `ghostty_app_new(runtime_config, config)` → `ghostty_app_tick`
  on a wakeup callback. Same 6 runtime callbacks (wakeup/action/3×clipboard/close).
- **`GHOSTTY_ACTION_RENDER`** must be handled in the action callback (queue a
  redraw) — ghostty drives redraws through actions, not auto-render. (Bit us on
  Linux.)
- **macOS gets the real upstream embedding API** — the embedded apprt officially
  supports macOS/iOS (Metal). So macOS does NOT need the `douglas/ghostty` fork or
  the Linux GL shim; it can build/link upstream `libghostty` and follow Ghostty's
  own macOS apprt (`GhosttyKit`, an `NSViewRepresentable`). This is *easier* than
  Linux was.
- Linux's build recipe (fork @ `c5028f9`, zig 0.15.2, GLAD, patchelf, the
  `GDK_DEBUG`/`create-context` GL gotchas) is Linux-only — see
  `apps/linux/scripts/build-libghostty.sh` + the `project_libghostty_embed` memory.

## 6. Remaining plan — Track A (macOS), sequenced

Extend `apps/ios/Project.swift`. Read first: `apps/ios/Project.swift`,
`apps/ios/Exponential/{Data,Domain,Shared,UI}`. iOS MUST stay green after each step.

### A1 — Extract `ExpCore` (+ `ExpUI`); iOS stays green ✅ DONE

> Done on branch `macos/a1-expcore`. `ExpCore` framework (`destinations: iOS + macOS`,
> sole external dep **GRDB**, vended **dynamic** to avoid double static-linking) now
> holds the moved Auth/API/DB/Electric/Domain/Shared + `AppConstants` layer with a
> `public` API; the iOS app + both share extensions build green (Tuist + Xcode 26.5,
> strict concurrency). Notes: `IssueStatus`/`IssuePriority` split (Foundation core in
> `ExpCore`, `.color` SwiftUI extension in the app's `IssueColorExtensions.swift`);
> `#if STAGING` in `AppConstants`/`SharedAppGroup` replaced by `Bundle.main` bundle-id
> detection (correct from a once-compiled framework); the Share Extension still compiles
> its own curated Foundation-only subset (no `ExpCore` link → stays GRDB-free);
> `IssueEditorModel` dropped `import UIKit`. **`ExpUI` deferred to A2** (its only A1
> artifact — the `.color` extensions — lives in the app until the first macOS view needs
> it); the full toolkit-neutral `IssueEditorModel`/`MarkdownConversion` abstraction
> stays an **A4** task.
- New **`ExpCore`** framework target (`destinations: iOS + macOS`, dep `GRDB`).
  Move the already-Foundation/GRDB-only code (the `Project.swift`
  `shareExtensionSources` list proves these import only Foundation/Security/
  CryptoKit): `Data/Auth/*`, `Data/API/*`, `Data/Electric/{ShapeMessage,ShapeClient,
  SyncManager}`, `Data/DB/{DatabaseManager,Entities}`, `Domain/{WorkspacePermissions,
  MultiAccountWorkspaceLoader,IssueFilters,Recurrence,DomainContract.generated,…}`,
  `Shared/*`, `AppConstants`. Make symbols `public`. Repoint the iOS app +
  ShareExtension at `ExpCore`.
- Optional **`ExpUI`** framework for cross-platform SwiftUI views + a
  `CrossPlatform.swift` of `#if os(...)` shims (nav title mode, `.sheet`/
  `presentationDetents`, `PhotosPicker`, pasteboard, `openURL`). `IssuePriority`/
  `IssueStatus` import SwiftUI for `Color` → keep them in `ExpUI`.
- **Refactor `IssueEditorModel` to drop `import UIKit`** (toolkit-neutral attributes;
  per-platform render layers) so one model serves both the iOS and macOS editors.
- **Gate:** iOS builds + runs unchanged.

### A2 — macOS skeleton + read-only live sync 🔶 built; runtime gate pending
- New app targets **`Exponential-macOS`** + **`-Staging`**. `NavigationSplitView`
  shell (account/workspace sidebar | project+issue list | issue detail) replacing
  the iOS `AppNavigator` `NavigationStack`; menu `.commands`; `Settings` scene.
- Wire `AppDependencies` + `SyncManager`; login (password + OIDC via
  `ASWebAuthenticationSession` anchored to `NSWindow`), multi-account, read-only
  live sync of all 10 shapes.
- **Gate:** log in against `next.exponential.at`; all 10 shapes live-sync.

> **Done (build + launch):** `ExpUI` framework extracted (theme/colors/avatar/
> CrossPlatform shims, shared iOS+macOS). `Exponential-macOS(-Staging)` targets
> build green and **launch cleanly** (all four dynamic frameworks — ExpCore, ExpUI,
> GRDB, GRDBSQLite — embed/sign/load; no crash). Implemented: `MacAppDependencies`
> (composition root, no Firebase/push; pre-opens pools + starts `SyncManager`),
> `MacLoginView`/`MacLoginViewModel` (instance picker + password + OAuth via
> `ASWebAuthenticationSession`/`NSWindow` anchor), `MacShell` (3-column split view),
> read-only `MacIssueListView`/`MacIssueDetailView` (GRDB `ValueObservation`),
> `MacSettingsView`. `KeychainStore` now uses the default keychain on macOS (no
> access group → no signed entitlement needed).
> **Still to verify (runtime, needs an interactive run on the Mac):** the actual
> log-in + 10-shape live-sync against `next.exponential.at`. Run the
> `Exponential-macOS-Staging` scheme (its bundle id → `next.exponential.at`), sign
> in, confirm projects/issues populate. This is the only unverified part of A2.

### A3 — macOS CRUD 🔶 built; runtime gate pending
Create/edit/delete issues; comments (`regular`/`question`/`plan`); labels;
attachments view + image upload (`/api/issues/{id}/images`); filtering/search;
workspace/member/invite settings. Reuse the tRPC `*Api` services from `ExpCore`.

> **Done (build + launch):** all remaining `*Api` wired into `MacAppDependencies`.
> Editable issue detail (title, status/priority/assignee menus, due-date picker,
> label toggle, **plain-markdown** `TextEditor` description — rich editor is A4),
> delete, and comments (list with regular/question/plan styling, add, delete
> own/admin, plan approve / request-changes). Create-issue sheet. Issue-list
> filter menu (status/priority/labels via `matchesFilters`) + search. Workspace
> settings sheet (general/members/invites/projects/labels, owner/admin-gated).
> Attachments section + `NSOpenPanel` image upload. All gated by
> `WorkspacePermissions`. macOS prod/staging + iOS build green; app launches clean.
> **Still to verify (interactive run):** that the mutations actually round-trip
> against a live server (create/edit/comment/label/settings reflect via Electric
> sync) — same runtime caveat as A2.

### A4 — macOS markdown editor → **macOS v1 feature-complete** 🔶 built; runtime gate pending
`MacMarkdownEditor` (`NSTextView` `NSViewRepresentable`) on the decoupled
`IssueEditorModel`, mirroring `EditorTextView`'s list-continuation / checkbox
toggle / image paste. `MacMarkdownStyle/Toolbar/ImageLoader` (`NSFont/NSColor/
NSImage`). Honor the GFM contract (see root `CLAUDE.md`).

> **Done (build + launch):** the editor core (`IssueEditorModel` + cmark
> `MarkdownConversion`/`MarkdownAttributes`/`ImageUtils`/`RangeUtils`) is now
> shared via ExpUI, cross-platform (`PlatformFont`/`PlatformColor` + AppKit
> symbolic-trait helpers). `MacMarkdownEditor` is a block-based AppKit editor:
> self-sizing per-block `MacEditorTextView` (checkbox-click toggle,
> backspace-at-start merge, image paste), an `NSTextViewDelegate` coordinator
> (live attributed rendering, list continuation/exit, selection→model), a
> SwiftUI toolbar (heading/bold/italic/strike/bullet/ordered/checklist/code/
> quote/link/image) driving the focused view, `NSImage` block views +
> `MacAttachmentImageLoader`. The issue detail + create sheet use it with the iOS
> commit/upload flow (debounced autosave + flush on close). All targets build
> green; app launches clean. **Still to verify (interactive run):** typing,
> formatting, list/checkbox behavior, and image paste/upload against a live
> account — same runtime caveat as A2/A3.

### A5 — macOS desktop-agent (mirror Linux M5–M7) 🔶 identity done; loop+terminal blocked

> **Done — M5 identity (build + launch green):** `MacAgentService`/`MacAgentStore`
> register the Mac (`companion.create` owner session → `claimSetup` → store
> `MacAgentIdentity` + `expk_` in Application Support), 30s heartbeat under
> `expk_`, GitHub device-flow (token → `github.json` + `reportGithubIdentity`),
> and `uninstallSelf`. Wired into `MacAppDependencies` (heartbeats start at launch)
> + a "Desktop Agent" section in the workspace-settings sheet. Pure Swift+HTTP,
> no Rust dep; registering makes the Mac appear in the web agents list.
> **Done — M6 loop (build + launch green):** Rust toolchain installed; a
> Project.swift pre-build script runs `cargo build -p agent-core --release` and
> the macOS app links the cdylib (clang module map over `agent_core.h`,
> `-lagent_core`, `target/release` search path; the dylib loads from its absolute
> install name for local dev — bundle/sign = M8). `MacAgentCore` wraps the C ABI
> (create/start/stop/submit) and fulfils `run_request` via `MacAgentRunner`
> (`Foundation.Process` runs `program argv… <combined-prompt>`, captures
> stdout+stderr + exit code, calls `submit_run_result`) — mirrors
> `agent_manager.zig`. A core runs per registered workspace alongside the
> heartbeat (v1 = while app open).
> **Deferred — M7 terminal (build blocked on a toolchain mismatch):** a macOS
> build recipe exists at `apps/ios/scripts/build-libghostty-macos.sh` (download
> local zig + clone the pinned ghostty `c5028f9` + `zig build -Dapp-runtime=none`
> → `libghostty.dylib` + `ghostty.h`; Metal-native, no GLAD/patchelf). **It can't
> complete on this machine:** ghostty 1.3.1 pins **zig 0.15.2**, which **cannot
> link macOS binaries against the macOS 26 SDK** (even `hello.zig` fails with
> undefined `libSystem` symbols). zig **0.16** links fine here, so the fix is to
> build a **newer ghostty that compiles with zig 0.16** (diverges from the
> Linux-pinned `c5028f9` ABI — a version call for the team), or build on an older
> macOS SDK. Until libghostty is produced, the headless `Foundation.Process`
> runner stands in (the agent works; you just don't see the CLI live). Once the
> `.dylib` + `ghostty.h` exist, `GhosttyKit` mirrors `apps/linux/src/ui/terminal.zig`
> (process-global app ticked on `wakeup`; one surface per `NSView` via
> `platform.macos={nsview}`; `ACTION_RENDER`→redraw; `NSEvent` input forwarding;
> `process_exited` polling) — Metal is the apprt's native target.
> **Runtime gate (needs an interactive run):** register from an owner account →
> appears online in web `agents-section.tsx`; with `claude`/`codex` on PATH + a
> GitHub token, assign an issue → plan→approve→code→PR. Verifies M5+M6.
- **Link agent-core:** add a clang module map over `agent_core.h` + a Swift
  `@Observable AgentService` wrapper around the C ABI (§4). Build the Rust cdylib
  for macOS (`cargo build` for the Mac arch; bundle the dylib, sign it).
- **Registration + heartbeat + GitHub** (mirror `apps/linux/src/core/agent/*`):
  in-app register (`companion.create`→`claimSetup`) + setupToken paste; store
  `AgentIdentity` in Keychain; 30s heartbeat under `expk_`; GitHub device-flow.
- **Embedded terminal (M7):** a `GhosttyKit` Swift-over-libghostty `NSView`
  (Ghostty's macOS apprt pattern, Metal). Route `run_request` → run CLI in the
  surface → capture → `agent_core_submit_run_result`. Mirror
  `apps/linux/src/core/agent/agent_manager.zig`'s wrapper/capture/exit logic.
- **Gate:** register from an owner account → appears online in web `agents-section.tsx`;
  assign an issue → plan→approve→code→PR works, visible in the terminal.

## 7. Cross-cutting remaining items (either platform)

- **Headless / background agent mode.** v1 agents run only while the GUI is open.
  The companion's one capability not yet replaced is headless/systemd operation.
  Decide: a `--headless` daemon mode (run the core without a window; the terminal
  becomes optional/off-screen) vs. accept GUI-only. (Product call.)
- **Linux parity gaps** (open, low priority): status multi-select in the filter
  popover (currently tabs only); error-comment **Retry** button (→ `agentPlan.retry`);
  project rename/delete + workspace delete in settings; search across description/
  comments (title only today); `workspaceInvites.create` is a blocking call.
- **macOS packaging/M8:** notarization + hardened runtime (must allow spawning
  child processes + the bundled Rust dylib + the libghostty Metal surface).
- **Linux packaging:** Flatpak/native; GUI-open-vs-background-service decision.

## 8. Build/verify quick reference

```bash
# Rust agent-core (both platforms share it)
cargo test -p agent-core                         # 60 tests
cargo build -p agent-core --release              # cdylib in target/release

# Linux app
bash apps/linux/scripts/build-libghostty.sh      # one-time: build embeddable libghostty
cd apps/linux && zig build && zig build test     # build + 24 tests
./zig-out/bin/exponential term-smoke             # embedded terminal smoke
./zig-out/bin/exponential run-smoke bash -c '…'  # agent-run capture smoke

# Contract sync (when enum values change)
bun run --filter @exp/domain-contract generate   # refresh Swift/Kotlin/Rust/Zig constants

# macOS (after A1+)
cd apps/ios && tuist generate                    # regenerate the Xcode project
```

## 9. Milestone ledger

| Milestone | Linux | macOS |
|---|---|---|
| M0 shared base (agent-core scaffold + contract emitters) | ✅ | ✅ (shared) |
| v1 tracker (login, sync, CRUD, editor, settings) | ✅ B1–B4 | 🔶 A1–A4 built (ExpCore · shell+login+sync · CRUD/comments/labels/filter/settings/attachments · NSTextView WYSIWYG editor) — build+launch green; runtime gate (login + mutations against a live server) unverified |
| M5 desktop-agent identity (register/heartbeat/GitHub) | ✅ | ✅ A5 (build+launch green; runtime unverified) |
| M6 agent loop (Rust core) | ✅ (shared) | ✅ linked + Process runner (build+launch green; runtime unverified) |
| M7 libghostty embedded terminal | ✅ | ☐ needs macOS libghostty build (Process runner stands in) |
| M8 parity tests | ✅ (60 tests) | — (shared core already covered) |
| M8 decommission `apps/companion` | ✅ deleted | — |
| M8 packaging/notarization | ☐ Flatpak | ☐ notarize+harden |
| Headless/background mode | ☐ | ☐ |

**Next action:** macOS A1–A5 are built (v1 tracker + desktop-agent identity + agent
loop via the Process runner). Run `Exponential-macOS-Staging` against
`next.exponential.at` to exercise the runtime gates: A2–A4 (login/sync/CRUD/editor),
A5 M5 (register → the Mac appears in web `agents-section.tsx`), and A5 M6 (with
`claude`/`codex` on PATH + a GitHub token, assign an issue → plan→approve→code→PR
runs headlessly). The remaining macOS work is **M7** — the embedded **libghostty
(Metal)** terminal for the visible "watch & steer" UX (needs a macOS libghostty
build; the Process runner stands in meanwhile) — and **M8 packaging** (bundle +
sign the `agent_core` dylib, notarize, hardened runtime allowing child processes).
