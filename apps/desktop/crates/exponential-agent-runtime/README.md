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
| `RunBackend` (4d) | report state, reply cards, consume queued messages, answers | `api::session_device` over tRPC | in memory |
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
   - **4d, next: `host` and `session`.** They cannot move on their own: they and `lifecycle` /
     `outbox` / `aside` (which stay in `engine`) use each other. `session::start` builds
     `RunLifecycle`; `host` calls `lifecycle::{record_session_ids, lost_reason,
     record_context_base}`, holds the `Outbox` and `AsideState` / `AsideEnd` on `SessionCtx`;
     `lifecycle` reads `host::{SessionCtx, EngineCommand, EngineExit, EngineHost, KillFeed,
     KillReason, SessionIds}`. So 4d first puts the lifecycle behind a trait on `SessionCtx`
     (the outbox's `ConversationApi` reply / consume plus the `record_*` reports become
     `RunBackend`, plain core types, `NoBackend` in the core, the tRPC impl in `engine`), then
     moves `host` + `session`. What they still pull, i.e. the seams 4d defines: `RunBackend`
     (`api::TrpcClient` on `SessionCtx` / `EngineStart` / `OpenTranscript`, `api::sessions::
     AnswerInput` (the fresh-context downgrade), `api::session_device::LostReason`,
     `ApiError` in `EngineExit::end`); a start from `LaunchSpec` instead of
     `coding::PreparedLaunch` (`prepare_with_hooks`, `ResumeSeed`, `skill::system_append`,
     `coding::aside`, `run_registry::{get, RunRecord}` in `HistoryHandle`, `pickup::LiveEngine`
     / `write_back_card`, `end_session`, `computer::turn_started`, `launcher::STEER_IMAGES_DIR`);
     from `steer` the relay behaviour behind a trait (`SteerRuntime` as a tokio handle the host
     passes in, `TurnSignal` / `CaptionSignal` / `AgentTitleSignal`, `answer_tap::{notify,
     card_resolution}`, `image_localizer` / `localize_message` / `ImageEmbeds` /
     `AttachmentHook` / `has_attachment_embed` / `PublishSpec::embeds`, `CompactVerdict`) and
     the pure parts into `wire` (`workflow_caption`, `RESUMED_IDLE_CAPTION`, `QUEUE_MAX`,
     `CONFIG_ID_MAX`, `STOP_GRACE`, `blocked_wall_expired` (chrono)); `domain::plan_card::copy`
     and `domain::session_kind` as core types pinned in `engine` tests (like 4b).
5. A local backend and a runnable example: one turn plus the session history with your own login,
   no server.

Named per the shared-libs table (`docs/libraries.md`, EXP-1421): crate `exponential-agent-runtime`, release tag `agent-runtime-v*`.
