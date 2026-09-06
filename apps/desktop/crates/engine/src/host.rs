//! EXP-746 — the ACP client host: what the engine advertises, what it asks of
//! whoever started it, and the command loop that keeps ONE `ConnectionTo`
//! open across a whole session.
//!
//! Two dispatch loops sit back to back — our `Client` role and the adapter's
//! `Agent` role — and the ONE rule that keeps them alive is:
//! **any handler that can take more than a few milliseconds must
//! `cx.spawn(...)` with its `Responder` moved in.** Handlers run inside the
//! dispatch loop and block every further message until they return, so a
//! permission handler that awaits a relay round-trip inline deadlocks the
//! whole session — silently, with no error. The matching hazard on the way
//! out: a task handed to `ConnectionTo::spawn` that returns `Err` shuts the
//! WHOLE connection down, so every spawned task swallows its error into
//! `Responder::respond_with_error` and returns `Ok(())`.
//!
//! Signatures land in P0; lane E1 fills the loop.

use std::sync::Arc;

use agent_client_protocol::schema::v1::{
    BooleanConfigOptionCapabilities, ClientCapabilities, ClientSessionCapabilities,
    CompactionCapabilities, ContentBlock, ElicitationCapabilities, ElicitationFormCapabilities,
    FileSystemCapabilities, SessionConfigId, SessionConfigOptionValue, SessionConfigOptionsCapabilities,
    SessionModeId,
};

use crate::adapters::Adapter;
use crate::local::LocalFeedEvent;

/// Where the rich, host-local feed items go. `None` on the daemon, which
/// publishes and nothing else.
pub type LocalSink = Arc<dyn Fn(LocalFeedEvent) + Send + Sync>;

/// Why the run must stop. Produced by the host's own kill source: the
/// desktop's Electric `sync::kill_watch`, the CLI's 15 s tRPC poll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KillReason {
    /// Stop now (a relay `kill` frame, an `ended` row flipped by a person).
    Now,
    /// The agent declared its own run over (EXP-637) — wait `steer::STOP_GRACE`
    /// on the `TurnSignal` so the last turn lands before the row ends.
    AfterTurn,
}

/// A kill source the HOST registered before `engine::start`, so the watch
/// exists before the first frame can arrive. The engine only consumes edges;
/// the decision (`kill_poll_decision`, EXP-681's `GATED_KILL_AFTER`,
/// `ended_policy`) stays with the host that owns it.
///
/// Dropping this unwatches: the engine drops it FIRST in the end sequence, so
/// the row's own `ended` flip can never bounce back as a second kill (EXP-283).
pub struct KillFeed {
    pub rx: flume::Receiver<KillReason>,
    pub unwatch: Option<Box<dyn FnOnce() + Send>>,
}

impl Drop for KillFeed {
    fn drop(&mut self) {
        if let Some(unwatch) = self.unwatch.take() {
            unwatch();
        }
    }
}

/// The one callback the two hosts implement. Fired ONCE per run, off the
/// engine thread — the desktop marshals to the gpui foreground itself.
pub trait EngineHost: Send + Sync + 'static {
    fn on_exit(&self, exit: EngineExit);
}

/// How a run finished. `end` is the result of `coding::end_session`, carried
/// here so the CLI can apply `registry::end_outcome_resolves` (EXP-641)
/// without ending the row a second time itself.
#[derive(Debug)]
pub struct EngineExit {
    pub session_id: String,
    /// The publisher `bye` outcome, in today's vocabulary:
    /// `exit:<code>` | `ended` | `killed`.
    pub outcome: String,
    /// Present when the adapter owned a child process.
    pub child: Option<terminal::pty::ChildExit>,
    /// A handshake or transport failure, for the screen's banner.
    pub error: Option<String>,
    pub end: Option<Result<api::coding_sessions::CodingSession, api::error::ApiError>>,
}

/// What the client advertises at `initialize`.
///
/// `terminal: false` is deliberate (D5): command output renders as local
/// cards built from `ToolCallContent::Content`, so `terminal/create|output|
/// wait_for_exit|kill|release` stay unimplemented and claude takes its
/// fenced-console branch. `elicitation.form` is NOT optional — without it the
/// claude port has to disallow `AskUserQuestion` and codex answers
/// `requestUserInput` with `{}` immediately, silently discarding the agent's
/// question.
pub fn client_capabilities() -> ClientCapabilities {
    ClientCapabilities::new()
        .fs(FileSystemCapabilities::new()
            .read_text_file(true)
            .write_text_file(true))
        .terminal(false)
        .elicitation(ElicitationCapabilities::new().form(ElicitationFormCapabilities::new()))
        .session(
            ClientSessionCapabilities::new()
                .config_options(
                    SessionConfigOptionsCapabilities::new()
                        .boolean(BooleanConfigOptionCapabilities::new()),
                )
                // EXP-724's relay `compaction` kind is fed by
                // `SessionUpdate::CompactionUpdate`, which is gated behind the
                // crate's `unstable_session_compaction` feature.
                .compaction(CompactionCapabilities::new()),
        )
}

#[allow(dead_code)]
/// Fire-and-forget commands the `main_fn` loop drains for the session's whole
/// lifetime. This is the ONLY supported way to hold a `ConnectionTo` open
/// across prompts: `Builder::into_connection_and_future` is private, so the
/// connection lives exactly as long as `main_fn` does.
pub(crate) enum EngineCommand {
    Prompt(Vec<ContentBlock>),
    /// Mid-turn steering — the same entry point a relay `input` frame takes.
    Steer(String),
    Cancel,
    SetConfig {
        id: SessionConfigId,
        value: SessionConfigOptionValue,
    },
    SetMode(SessionModeId),
    Answer(steer::RemoteAnswer),
    /// A `/` command: an agent-advertised one becomes prompt text, a contract
    /// one is handled by `steer`'s `CommandLink` before it ever gets here.
    Command {
        name: String,
        args: String,
    },
    /// Replay history through the mapper (`session/load`).
    LoadHistory,
    Shutdown {
        outcome: &'static str,
    },
}

/// Everything a running session needs that never changes after `start`.
/// Cheap to clone behind an `Arc`; the mutable half (the pending-ask table,
/// the ids learned at `session/new`) lives in `session::Inner`.
// EXP-746: the skeleton declares the shape before E1 constructs it — drop
// this allow with the bodies.
#[allow(dead_code)]
pub(crate) struct SessionCtx {
    pub(crate) session_id: String,
    pub(crate) prepared: coding::PreparedLaunch,
    pub(crate) trpc: Arc<api::TrpcClient>,
    pub(crate) runtime: Arc<steer::SteerRuntime>,
    pub(crate) data_dir: std::path::PathBuf,
    pub(crate) account_id: String,
    pub(crate) own_user_id: Option<String>,
    pub(crate) personal_key: Option<String>,
    pub(crate) issue_id: Option<String>,
    pub(crate) foreign_host: bool,
    pub(crate) publish: bool,
    pub(crate) local_sink: Option<LocalSink>,
    pub(crate) turn_signal: Arc<steer::TurnSignal>,
}

/// The connection driver. Runs on the dedicated `acp-engine-<sid8>` thread
/// inside `SteerRuntime::handle().block_on(..)` and does not return until the
/// session ends.
///
/// Shape (E1): `Client.builder().name("exponential")` + handlers for
/// `SessionNotification` (→ `Mapper::on_update`), `session/request_permission`
/// and `elicitation/create` (both SPAWNED, responder parked) and
/// `fs/read_text_file` / `fs/write_text_file` (answered inline — one
/// `std::fs` call), then `connect_with(adapter, main_fn)` where `main_fn`
/// runs `initialize` → `session/new|load|resume` → publish the first
/// `config_state` → drain [`EngineCommand`] until `Shutdown`.
// EXP-746 E1: fill
#[allow(unused_variables, dead_code)]
pub(crate) async fn run_session(
    ctx: Arc<SessionCtx>,
    adapter: Adapter,
    commands: flume::Receiver<EngineCommand>,
) -> Result<(), agent_client_protocol::Error> {
    todo!("EXP-746 E1: build the Client host and drive the command loop")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_advertises_no_terminal_and_a_form_elicitation() {
        let capabilities = client_capabilities();
        assert!(capabilities.fs.read_text_file);
        assert!(capabilities.fs.write_text_file);
        // D5: command output is a local card, never a live terminal.
        assert!(!capabilities.terminal);
        assert!(capabilities
            .elicitation
            .as_ref()
            .is_some_and(|elicitation| elicitation.supports_form()));
    }

    #[test]
    fn dropping_the_kill_feed_unwatches() {
        let unwatched = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = unwatched.clone();
        let (_tx, rx) = flume::unbounded::<KillReason>();
        drop(KillFeed {
            rx,
            unwatch: Some(Box::new(move || {
                flag.store(true, std::sync::atomic::Ordering::SeqCst)
            })),
        });
        assert!(unwatched.load(std::sync::atomic::Ordering::SeqCst));
    }
}
