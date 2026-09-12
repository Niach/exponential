//! The ACP session engine (EXP-746).
//!
//! Hosts an `agent-client-protocol` client in-process, drives the user's own
//! agent CLI through one of the four adapters, maps every `SessionUpdate` into
//! the steer wire vocabulary, and owns the run lifecycle (heartbeat,
//! publisher, diff snapshots, kill feed, end sequence) that the desktop IDE
//! and the headless CLI daemon used to duplicate by hand.
//!
//! ```text
//! coding::prepare_with_hooks → PreparedLaunch { transport: Acp, acp: Some(..) }
//!   → engine::start(EngineStart, host) ──► EngineSession
//!       ├ thread acp-engine-<sid8>: Client.builder()…connect_with(adapter, main_fn)
//!       │    adapter = Claude | Codex | External   (ConnectTo<Client>)
//!       ├ mapper.rs : SessionUpdate/requests → wire ActivityEvent + LocalFeedEvent
//!       ├ lifecycle.rs : publisher, heartbeat, diffs, kill feed, end sequence
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
//! in its doc comment: E1 core, E2 claude, E3 codex, E4 external, S1 spike.

pub mod adapters;
pub mod host;
pub mod lifecycle;
pub mod local;
pub mod mapper;
pub mod session;
pub mod sink;
// FEED-25: the stall watchdog — a live turn silent for `STALL_AFTER` is
// interrupted, one that ignores the interrupt for `STALL_KILL_GRACE` ends the
// run with a reason. Pure; the lifecycle ticker drives it.
pub mod stall;
// EXP-750: the ACP `terminal/*` capability. Session-owned, host-local: the
// registry lives on `SessionCtx` and nothing it produces leaves the machine.
mod terminals;
pub mod transport;

pub use adapters::{Adapter, AdapterKind, AdapterSpec};
pub use host::{
    client_capabilities, ChildExitLink, EngineExit, EngineHost, KillFeed, KillReason, LocalSink,
    NATIVE_SESSION_META_KEY,
};
pub use local::{
    EnginePhase, LocalFeedEvent, PlanEntryPriorityView, PlanEntryStatusView, PlanEntryView,
    SubagentEdge, SubagentEdgeStatus, ToolCardKind, ToolCardStatus, COMPACTION_TRIGGER_META_KEY,
    SUBAGENT_ID_META_KEY, SUBAGENT_META_KEY,
};
pub use mapper::{clamp_usage, AnswerDecision, MapOut, Mapper, MapperConfig, PendingAskKey};
pub use session::{
    start, start_with, ConfigValue, EngineError, EngineParts, EngineSession, EngineStart,
    HistoryHandle, OpenTranscript, ResumeHandle,
};
pub use sink::{EventSink, RecordingSink};
pub use transport::{spawn_lines, ChildLines, LineWriter, StderrPolicy};
