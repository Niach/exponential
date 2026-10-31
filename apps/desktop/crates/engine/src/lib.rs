//! The ACP session engine (EXP-746).
//!
//! Hosts an `agent-client-protocol` client in-process, drives the user's own
//! agent CLI through one of the four adapters, maps every `SessionUpdate` into
//! the steer wire vocabulary, and owns the run lifecycle (the Conversation
//! outbox, publisher, diff snapshots, kill feed, end sequence) that the
//! desktop IDE and the headless CLI daemon used to duplicate by hand.
//!
//! ```text
//! coding::prepare_with_hooks → PreparedLaunch { transport: Acp, acp: Some(..) }
//!   → engine::start(EngineStart, host) ──► EngineSession
//!       ├ thread acp-engine-<sid8>: Client.builder()…connect_with(adapter, main_fn)
//!       │    adapter = Claude | Codex   (ConnectTo<Client>)
//!       ├ mapper.rs : SessionUpdate/requests → wire ActivityEvent + LocalFeedEvent
//!       ├ lifecycle.rs : outbox, publisher, diffs, kill feed, end sequence
//!       └ desktop: Screen::Session (FeedSource::Local) · cli: line printer
//! ```
//!
//! Two invariants worth stating once, because everything else follows from
//! them:
//!
//! - **The wire is the relay's vocabulary and nothing else.** Everything a
//!   viewer can see is a `steer::ActivityEvent`, derived, redacted and capped.
//!   Rich local items (per-edit diff cards, command output, the pinned plan,
//!   thoughts) are [`LocalFeedEvent`]s and never leave the host.
//! - **Handlers must not block the dispatch loop.** Anything that can take
//!   more than a few milliseconds is spawned with its `Responder` moved in;
//!   see [`host`].
//!
//! Landed in P0 as signatures with `todo!()` bodies (the transport, the sink
//! and the small conversions are real); the lanes that fill each one are named
//! in its doc comment: E1 core, E2 claude, E3 codex, S1 spike.

pub mod adapters;
// EXP-1330: a `/btw` aside run's engine half (fork posture, one-shot end).
pub mod aside;
// EXP-1206: the human-readable head of a command line (`git status`), the
// detail a Bash row wears when the agent described nothing. Pure.
pub use exponential_agent_runtime::command_head;
// EXP-936: the run's own compaction ask — the verdict policy and the two
// steps the command loop takes for an accepted one. Pure; `host` drives it.
// EXP-1426 part 4: moved to the agent runtime with the rest of the
// server-free core below (transport, sink, local feed, stall, api_retry).
pub use exponential_agent_runtime::engine::compaction;
// EXP-1051: the `context_layout` frame's arithmetic — bytes → tokens, the
// launcher's layers as estimated segments, and `base` by subtraction from the
// prefix the agent measured. Pure; the adapters stamp it, the mapper emits it.
// EXP-1426 part 4b: moved to the agent runtime with the mapper.
pub use exponential_agent_runtime::engine::context_layout;
// EXP-920: the entity refs an Exponential MCP answer names, distilled on the
// device for every client's chips (fixture-locked with web `lib/emcp/preview.ts`).
// EXP-1426 part 4b: the rule is the core's; this keeps the name-keyed entry.
pub mod exp_tool_refs;
// EXP-1426 part 4d: the host and the session are the agent runtime's; these
// re-export them beside Exponential's start, exit and `RunBackend` (lifecycle).
pub mod host;
pub mod lifecycle;
pub use exponential_agent_runtime::engine::local;
// EXP-1426 part 4b: the core's mapper plus Exponential's `HostTools`.
pub mod mapper;
pub mod outbox;
/// NEW-17: each agent run in its own systemd scope on Linux.
pub use exponential_agent_runtime::run_scope;
pub mod session;
pub use exponential_agent_runtime::engine::sink;
// FEED-61: a turn that died on a transient API error is re-prompted by the
// engine after a backoff. Pure; the lifecycle ticker drives it.
pub use exponential_agent_runtime::engine::api_retry;
// FEED-25: the stall watchdog — a live turn silent for `STALL_AFTER` is
// interrupted, one that ignores the interrupt for `STALL_KILL_GRACE` ends the
// run with a reason. Pure; the lifecycle ticker drives it.
pub use exponential_agent_runtime::engine::stall;
pub mod stop_save;
pub use exponential_agent_runtime::engine::transport;

pub use adapters::{Adapter, AdapterKind, AdapterSpec};
pub use context_layout::{
    estimate_tokens, estimated_segments, layout, measure_base, BasePrefix,
    CONTEXT_LAYOUT_META_KEY,
};
pub use compaction::{
    compact_command, CompactionPolicy, CompactionStep, COMPACT_CONTINUE_PROMPT, COMPACT_COOLDOWN,
    COMPACT_COOLDOWN_TURNS, COMPACT_MIN_CONTEXT_FRACTION,
};
pub use host::{
    client_capabilities, ChildExitLink, EngineExit, EngineHost, KillFeed, KillReason, LocalSink,
    NATIVE_SESSION_META_KEY,
};
pub use local::{
    EnginePhase, LocalFeedEvent, PlanEntryPriorityView, PlanEntryStatusView, PlanEntryView,
    SubagentEdge, SubagentEdgeStatus, ToolCardKind, ToolCardStatus,
    BACKGROUND_TASKS_META_KEY, COMPACTION_TRIGGER_META_KEY,
    SUBAGENT_ID_META_KEY, SUBAGENT_META_KEY, TOOL_DETAIL_META_KEY, TOOL_KIND_META_KEY,
    TURN_META_KEY, TURN_TOKENS_META_KEY, WORKFLOW_META_KEY,
};
pub use mapper::{
    clamp_context_layout, clamp_usage, AnswerDecision, CardAnswer, ExpHostTools, MapOut, Mapper,
    MapperConfig, PendingAskKey, Reply,
};
pub use session::{
    start, start_with, ConfigValue, EngineError, EngineParts, EngineSession, EngineStart,
    HistoryHandle, OpenTranscript, ResumeHandle,
};
pub use sink::{EventSink, RecordingSink};
pub use lifecycle::{keeps_workspace, RELEASED_OUTCOME, SUSPEND_OUTCOME};
pub use stop_save::{save_stopped_run_work, stopped_by_person};
pub use transport::{spawn_lines, ChildLines, LineWriter, StderrPolicy};
