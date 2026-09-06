//! EXP-746 — everything around the ACP connection that used to be duplicated
//! by hand in `ui::steer_wiring` and `cli::session_host`.
//!
//! On the ACP path the ENGINE owns the publisher (D14): the host registers a
//! kill feed and then only hears back through
//! [`EngineHost::on_exit`](crate::EngineHost::on_exit). What lives here:
//!
//! - `coding::start_heartbeat` (immediate first beat, EXP-701);
//! - `steer::publish` with `PublisherHooks { kill → Shutdown("killed"),
//!   answers: AnswerLink, text_sink → Prompt/Steer, attachments:
//!   `steer::image_localizer` (EXP-511 image embeds), commands: CommandLink,
//!   config: ConfigLink }`;
//! - `DiffSnapshots::next_diff` every `steer::DIFF_INTERVAL` (the wire `diff`
//!   stays the debounced WHOLE-worktree patch on both transports; per-edit ACP
//!   diffs are local-only);
//! - `NeedsInputForwarder` (EXP-214);
//! - the kill feed, where `AfterTurn` waits `steer::STOP_GRACE` on the
//!   `TurnSignal` before ending;
//! - the D8 `runs.json` upsert of `acp_session_id`/`agent_native_session_id`
//!   once `session/new` answers (prepare wrote `transport` already);
//! - the end sequence, in EXACTLY this order (D14, EXP-283): drop the
//!   [`KillFeed`](crate::KillFeed) (which unwatches) → drop the heartbeat →
//!   `publisher.shutdown(Some(outcome))` → `coding::end_session(&trpc, &sid)`
//!   (observer-based, so the CLI's `registry::end_outcome_resolves` still
//!   applies) → `host.on_exit(EngineExit { .., end })`.
//!
//! Signatures land in P0; lane E1 fills the bodies.

use std::sync::Arc;

use crate::host::{EngineCommand, EngineExit, EngineHost, KillFeed, SessionCtx};
use crate::session::EngineError;

/// The run's side-car machinery, alive for exactly as long as the connection.
// EXP-746: the skeleton declares the shape before E1 constructs it — drop
// this allow with the bodies.
#[allow(dead_code)]
pub(crate) struct RunLifecycle {
    // EXP-746 E1: fill — publisher handle, heartbeat stop, diff ticker,
    // needs-input forwarder, kill-feed pump.
}

#[allow(dead_code)]
impl RunLifecycle {
    /// Bring up the heartbeat, the publisher and the tickers, and start
    /// pumping the kill feed into `commands`.
    // EXP-746 E1: fill
    #[allow(unused_variables)]
    pub(crate) fn attach(
        ctx: Arc<SessionCtx>,
        kill: KillFeed,
        commands: flume::Sender<EngineCommand>,
    ) -> Result<RunLifecycle, EngineError> {
        todo!("EXP-746 E1: heartbeat + publisher + diff/needs-input tickers + kill pump")
    }

    /// The D14 end sequence. Consumes the lifecycle so the order cannot be
    /// re-entered, and hands back the exit the host is told about.
    // EXP-746 E1: fill
    #[allow(unused_variables)]
    pub(crate) fn end(
        self,
        ctx: &SessionCtx,
        host: &dyn EngineHost,
        outcome: &str,
        child: Option<terminal::pty::ChildExit>,
        error: Option<String>,
    ) -> EngineExit {
        todo!("EXP-746 E1: unwatch -> heartbeat -> bye -> coding::end_session -> on_exit")
    }
}
