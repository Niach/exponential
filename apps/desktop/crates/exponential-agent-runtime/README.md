# exponential-agent-runtime

The single-account agent core shared by **Exponential** and **spezl** (EXP-1426, epic EXP-1420).
It spawns the user's own, unmodified `claude` / `codex` binary over ACP, streams what the agent
does, lists the CLIs' own session history (with their own session names), runs the agent's login
in a PTY and wires MCP servers. One local account, no server.

## Rules

1. **No server coupling.** No tRPC, token store, device id, steer relay or pickup loop. The crate
   depends on no Exponential app crate and no UI toolkit (`tests/dependency_gate.rs`).
2. **Never touch an agent's credentials.** The core spawns the vendor's binary and lets it own its
   login: it never reads, copies, moves or refreshes a token. Anthropic's Claude Code terms forbid
   token intermediation, so `coding/src/claude_oauth.rs` is NOT part of this crate and never will
   be. Exponential's multi-account staging and rotation stay in `coding`.

## Seams

Server coupling becomes traits the hosts implement. Each lands with the first module that needs it.

| Trait | What it answers | Exponential (`coding` + `engine`) | spezl / local |
| --- | --- | --- | --- |
| `LaunchSpec` | what to spawn and how (program, env, cwd, agent, picks, resume, MCP servers, context layers, confine root) | `engine::adapters::AdapterSpec` → `LaunchSpec` | built directly |
| `EventSink` | where mapped events go | the steer publisher | the voice loop / UI |
| `RunBackend` | the side-car a run brings up and its end sequence, the run record, reply cards, consumed messages, open cards | `engine::lifecycle::RunLifecycle` (outbox, publisher, tickers, kill pump, `sessions.*` over tRPC) | `NoBackend` (in memory) |
| `PromptLocalizer` | a host prompt's attachment embeds, as the agent reads them | `engine::host::SteerLocalizer` (`steer::localize_message`) | none |
| `AccountEnv` | config dir + env for the spawn | multi-account rotation | the CLI's default home |
| `McpProvider` | the MCP servers a run gets | team + device servers | the user's own list |
| `HostTools` | the host's OWN tools: display row, subject key, preview spec, code-mode script log | `engine::ExpHostTools` (contract `expToolDisplay` / `expToolPreview`, `codemode` reports) | `NoHostTools` |

## What stays in Exponential

The pickup loop, the launcher, worktrees / git / clones / GitHub tokens, `TrpcClient` and
`TokenStore`, the steer relay, account rotation and usage, prune / reaper, remote admin and
`claude_oauth`.

## Extraction order

Stacked PRs, each behaviour-preserving: the old module paths re-export the moved modules, so no
caller changes until a later cleanup.

1. This crate, the dependency gate and the pure leaves: atomic config writes (`api::atomic_file`),
   the lock file, the claude / codex trust seeders, the codex session index, the confinement
   check, the command head, the Linux run scope, the background `Command` constructor
   (`terminal::process`).
2. **Done.** The event vocabulary out of `steer`, as `wire`: `wire::frames` (`ActivityEvent` and
   its pieces, `TurnState`, `ConfigValue`, `QueuedMessage`, `BackgroundTask`, the relay frames),
   `wire::workflow` (`WorkflowState` / `WorkflowStatus` / `WorkflowAgent*`), `wire::tool_output`
   and `wire::tool_diff`, and the pure half of `steer::activity` as `wire::activity` (`Redactor`,
   `secrets_from_worktree`, `worktree_diff`, `truncate*`, `SessionAgent`, the blocked-wall
   vocabulary, `normalize_agent_title`, `normalize_compaction_trigger`, `clamp_config_state`,
   the relay caps). `steer` re-exports every module under its old path and keeps the contract
   checks (`domain::contract`) for the caps the core inlines. Still in `steer`: the links,
   signals and forwarders, `SessionBlocked` (its `api` report shape) and `DiffSnapshots`.
3. **Done.** Agent identity, detection and the single-account login PTY (no credential moves):
   `agent` (`CodingAgent`, `claude_model_alias`, the model / effort vocabularies; the settings
   default stays in `coding` as `CodingAgentExt`), `detect` (`Tool`, a lean `ToolCheck`,
   `check_tool`, the version and auth-status parsers, the bounded `claude auth status` /
   `codex login status` probes, `is_signed_in`, `probe_codex_acp`, the min versions, the bounded
   `output_with_timeout`), `codex_app_server` (the codex handshake the probes use), the login-screen
   scrapers out of `steer` (`login_picker`, `codex_login_picker`, `login_screen` =
   `steer::agent_login_driver`) and the new `login`: the sign-in plan, `LocalLogin` (the CLI's own
   login in a PTY against its DEFAULT home, URL / device-code scraping into `LoginProgress`),
   logout. `run_doctor`, the profiles, the device caps and Exponential's `ToolCheck` (the lean
   one plus the account fields), the staging / commit / import flow and its emulator-driven PTY
   stay in `coding` / `cli` / `ui` and call the core.
4. The ACP engine core behind `RunBackend` / `EventSink`; `engine` becomes Exponential's adapter.
   - **4a, done.** `spawn` (`SpawnSpec`, `ChildExit`, `login_path`, out of `terminal::pty`, which
     re-exports them; the PTY emulator stays in `terminal`) and `engine::` = the child-process
     line transport (`transport`), the child's exit link (`child_exit`, out of `engine::host`),
     the `EventSink` seam on `wire::ActivityEvent` plus `RecordingSink` (`sink`; `steer`
     implements it for its `ActivitySender`), the local feed vocabulary (`local`) and the pure
     run policies (`stall`, `api_retry`, `compaction`). `engine` re-exports each under its old
     path.
   - **4b, done.** The mapper and its leaves. `engine::mapper` (the ONE `SessionUpdate` → wire
     mapper; `MapperConfig` carries `host_tools`, and `Mapper::new` takes `impl Into<MapperConfig>`
     so Exponential's session-shaped `engine::MapperConfig` fills in `ExpHostTools`),
     `engine::host_tools` (the `HostTools` seam plus `ScriptReport`), `engine::reply`
     (`ReplyCard` / `ReplyOption`, out of `api::session_device`, which re-exports them),
     `engine::context_layout` (plus `ContextLayers` / `ProjectMemoryFile` / `CarriedBase` out of
     `coding`, which re-exports them), `engine::exp_tool_refs` (the rule; the spec comes from
     `HostTools::preview_spec`, and `engine::exp_tool_refs` keeps the name-keyed entry and the
     fixture lock), and `engine::terminals` (the ACP `terminal/*` capability). The headless PTY
     (`Pty`, `open_pty`, `build_command`, `winsize`) moved out of `terminal::pty` into `spawn`;
     the emulator stays in `terminal`. The wire got `RemoteAnswer`, `SessionBlocked` (the
     `sessions.report` conversion is `steer::SessionBlockedReport`), `now_unix_millis`,
     `iso_from_unix_millis` / `now_iso` (chrono-free, pinned to `coding`'s formatter in steer)
     and `JOURNAL_WORKFLOW_CAP`. The contract values the mapper and refs inline are pinned in
     `engine::mapper` / `engine::exp_tool_refs` tests.
   - **4c, done (the adapters).** `engine::adapters` (`mod`, `claude`, `claude_wire`, `codex`,
     `codex_wire`, moved whole): a host starts the user's own `claude` / `codex` ACP adapter
     with `Adapter::new(impl Into<LaunchSpec>)` and drives it as a `ConnectTo<Client>`.
     `LaunchSpec` is the core launch spec (program / args / env / cwd as `SpawnSpec`,
     `CodingAgent`, `RunOptions`, resume handle, `AgentMcp` + `McpServerWire` servers, system
     append, `ContextLayers`, confine root, `ChildExitLink`); Exponential's
     `engine::adapters::AdapterSpec` (the launcher's `coding::LaunchOptions`) converts into it,
     the account slot becoming the profile id. Moved with them, re-exported under the old paths:
     `mcp` (`AgentMcp`, `McpServerWire`, `McpWireTransport`, `McpActor`, `MCP_TOKEN_ENV`,
     `MCP_JSON_FILE`, the codex entry / TOML renderings, out of `coding::argv`, `coding::mcp_json`,
     `api::mcp_servers`), `usage` (`UsageWindow`, the claude / codex live-limit parsers,
     `merge_live_windows` and the process-global `live` registry, out of
     `coding::agent_usage`; the OAuth fetch and the collector stay), `claude_transcript`
     (`locate_claude_transcript`), `engine::launch` (`RunOptions`, `ResumeHandle`, `EngineError`
     whose `Api` arm boxes the backend error, the ACP `_meta` keys, the aside tool posture) and
     `child_exit::until_either_closes`. `coding` converts `ResumeSeed`, `api` its `ApiError`.
   - **4d, done (the host and the session).** `engine::host` and `engine::session` moved whole
     (`engine` re-exports both; `lifecycle`, `outbox` and `aside`'s posture stay there). The knot
     is untied by `RunBackend`: the context holds it, the host calls it (`record_session_ids`,
     `record_context_base`, `recorded_native_session_id`, `post_replies` / `resolve_question` /
     `consume` / `open_cards`, `fresh_context_kept`, `turn_started`), and the core attaches it
     before the connection thread and hands it the end sequence. Exponential's `RunLifecycle`
     implements it (tRPC client, steer runtime, outbox, `lost`, the image hook stay on it).
     Moved with them: `KillFeed` / `KillReason`, `EngineCommand`, `SessionIds`, `RunFacts`,
     `SessionCtx` (public for a backend to read), `ExitState`, `EngineExit<End = ()>` (Exponential's
     is `EngineExit<Result<(), ApiError>>` with its own `EngineHost`), `end_phases`, the aside's
     one-shot state (`engine::aside`), and into `wire` the `TurnSignal` / `CaptionSignal` /
     `AgentTitleSignal` and the answer tap (`wire::answer_tap`, `steer` re-exports both). The
     steer runtime is a `tokio::runtime::Handle`; `domain::plan_card`'s gate line is
     `PLAN_MODE_ON_LINE`, pinned to the fixture in `engine::host`'s tests. What stays in
     `engine`: `EngineStart` from a `coding::PreparedLaunch`, `open_transcript`, the pickup's
     `LiveEngine`, the run-record reads, `fresh_context_downgrade`, the steer image localizer and
     `blocked()`'s expiry (chrono), on an `engine::EngineSession` that derefs to the core one.

     **The server-free API:** `engine::start(LaunchSpec, SessionSpec, Arc<dyn RunBackend>) ->
     EngineSession` (or `start_with(SessionSpec, adapter, backend)` for a scripted
     `ConnectTo<Client>`). `SessionSpec::for_launch(&launch, runtime)` fills it from the launch;
     set `sink` (an `EventSink`, e.g. `RecordingSink`) and `local_sink`. `NoBackend::with_host`
     reports the exit. The `EngineSession` takes `send_prompt` / `deliver` (queued while a turn or
     a compaction runs) / `answer` / `answer_card` / `set_config` / `set_mode` / `cancel_turn` /
     `unqueue` / `kill`, and reads `subscribe` / `phase` / `turn_signal` / `wait`; a resume is a
     new start with `ResumeHandle` (`session/load`). `tests/server_free_start.rs` drives it with
     `NoBackend` + `RecordingSink` against the fake agent.
5. **Done.** The server-free facade `local` (below), the CLIs' own session history
   (`local::history`), `examples/local_chat.rs`, and one additive MCP posture,
   `mcp::AgentMcp::ServersOnly` (the run's own servers, no `exponential` entry, never strict).
   Exponential never builds it; `coding` only names it in one exhaustive match.

## Using it without a server

`exponential_agent_runtime::local` is the whole surface a single-account host needs:

```rust
use exponential_agent_runtime::{agent::CodingAgent, local::{LocalAgent, RunRequest}};

let agent = LocalAgent::new(CodingAgent::Claude);    // or ::with_program(agent, "/abs/claude")
agent.check();                                       // `<cli> --version` -> ToolCheck
agent.signed_in();                                   // the CLI's own auth status: Option<bool>
agent.login(false)?;                                 // LocalLogin: the CLI's own sign-in in a PTY
agent.login_command_line();                          // "claude auth login --claudeai"
let run = agent.start(
    RunRequest::new(cwd)
        .prompt("…")                                 // the first message
        .resume(past.id)                             // or reopen a past conversation
        .mcp_server(server).env("NOTES_TOKEN", tok)  // extra servers, `${VAR}` refs into env
        .system_append("…"),                         // optional: .sink(..), .runtime(handle)
)?;
run.events();                                        // flume::Receiver<wire::ActivityEvent>
run.send("…");  run.answer(question_id, keys, text);  run.interrupt();  run.stop();
run.native_session_id();  run.title();               // the CLI's own id and name
agent.sessions(Some(&cwd), Some(20));                // Vec<PastSession>, newest first
```

- **One account, the default home.** No config-dir variable is set, no credential is read,
  copied or refreshed; the spawn env is the login shell's PATH plus `RunRequest::env`. The
  `AccountEnv` seam's local side is exactly that.
- **Events.** Narration, tool rows, `Turn { Started | Ended }`, and a question or permission as
  `ActivityEvent::Question` (answer it with `LocalRun::answer`). `RunRequest::sink` takes your
  own `EventSink` instead of the channel. Underneath it is `engine::start` + `NoBackend`;
  `LocalRun::session()` reaches the rest of `EngineSession`.
- **MCP.** The CLI keeps the servers the user configured for it; `RunRequest::servers` adds more.
- **History** (`local::history`, read-only over the CLIs' own files). claude:
  `<CLAUDE_CONFIG_DIR|~/.claude>/projects/*/<id>.jsonl`, titled by the latest `custom-title`,
  else `ai-title`, else `summary`, else the first prompt. codex: the rollouts under
  `<CODEX_HOME|~/.codex>/sessions`, titled by `session_index.jsonl`'s `thread_name`, else the
  first `user_message`; subagent threads are left out. `updated` = the file's mtime.

```sh
cd apps/desktop
cargo run -p exponential-agent-runtime --example local_chat -- "reply with the word ok"
cargo run -p exponential-agent-runtime --example local_chat -- --agent codex "hi"
```

## Still Exponential-only

- [ ] The pickup loop, the launcher (`coding::PreparedLaunch`, worktrees, git, clones, GitHub tokens).
- [ ] `RunLifecycle`: the run record, outbox, steer publisher and relay, `sessions.*` over tRPC.
- [ ] Multi-account: profiles, staging / import, rotation, usage collection, `claude_oauth`.
- [ ] The `exponential` MCP server posture (`ClaudeInline` / `CodexOverrides`), code mode, team servers.
- [ ] `ExpHostTools`, the steer image localizer, the plan card's `pr_open` gate, the run playbook.
- [ ] `open_transcript`, `fresh_context_downgrade`, prune / reaper, remote admin, the doctor and device caps.

Named per the shared-libs table (`docs/libraries.md`, EXP-1421): crate `exponential-agent-runtime`, release tag `agent-runtime-v*`.
