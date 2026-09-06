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
//! The tickers (diff + needs-input) run on their OWN small thread, never
//! inside the ACP dispatch loop: `worktree_diff` shells out to git, and a
//! handler that blocks the loop for the length of a `git diff` is exactly the
//! hazard §1.3 of the plan forbids.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::host::{EngineCommand, EngineExit, EngineHost, KillFeed, KillReason, SessionCtx};
use crate::local::EnginePhase;
use crate::session::EngineError;

/// The run's side-car machinery, alive for exactly as long as the connection.
pub(crate) struct RunLifecycle {
    publisher: Option<steer::PublisherHandle>,
    heartbeat: Option<coding::HeartbeatStop>,
    /// Dropped FIRST in the end sequence: dropping unwatches (EXP-283).
    kill: Option<KillFeed>,
    /// Clears the ticker thread and the publisher's activity path.
    active: Arc<AtomicBool>,
    /// EXP-447 parity: the launch-time installation token dies after an hour,
    /// so a session outliving it would strand the agent's `git push`.
    _refresher: Option<coding::RefresherHold>,
}

impl RunLifecycle {
    /// Bring up the heartbeat, the publisher and the tickers, and start
    /// pumping the kill feed into `commands`.
    pub(crate) fn attach(
        ctx: Arc<SessionCtx>,
        kill: KillFeed,
        commands: flume::Sender<EngineCommand>,
    ) -> Result<RunLifecycle, EngineError> {
        let active = Arc::new(AtomicBool::new(true));
        // A replay owns no row: no heartbeat, no publisher, no kill watch,
        // no diffs (`open_transcript`).
        if ctx.replay {
            return Ok(RunLifecycle {
                publisher: None,
                heartbeat: None,
                kill: None,
                active,
                _refresher: None,
            });
        }

        let heartbeat = ctx.run.heartbeat_scope.clone().map(|scope| {
            coding::start_heartbeat(Arc::clone(&ctx.trpc), ctx.session_id.clone(), scope)
        });

        let publisher = ctx
            .publish
            .then(|| attach_publisher(&ctx, &commands, Arc::clone(&active)));
        if let Some(publisher) = &publisher {
            // The sink may already be set (a test's recording sink): the
            // publisher then still runs, but nothing double-publishes.
            let _ = ctx
                .sink
                .set(Arc::new(publisher.activity_sender()) as Arc<dyn crate::sink::EventSink>);
        }

        spawn_kill_pump(&ctx, kill.rx.clone(), commands, Arc::clone(&active));
        spawn_tickers(&ctx, Arc::clone(&active));

        let refresher = ctx.run.repository_id.as_deref().map(|repository_id| {
            coding::clone_refreshers().retain(Arc::clone(&ctx.trpc), repository_id, &ctx.run.clone)
        });

        Ok(RunLifecycle {
            publisher,
            heartbeat,
            kill: Some(kill),
            active,
            _refresher: refresher,
        })
    }

    /// The D14 end sequence. Consumes the lifecycle so the order cannot be
    /// re-entered, and hands back the exit the host is told about.
    pub(crate) fn end(
        mut self,
        ctx: &SessionCtx,
        host: &dyn EngineHost,
        outcome: &str,
        child: Option<terminal::pty::ChildExit>,
        error: Option<String>,
    ) -> EngineExit {
        // 1. Stop our OWN kill watch before the end flip becomes visible, or
        //    we read our own end as a remote kill (EXP-283). Dropping the
        //    feed calls `unwatch`.
        drop(self.kill.take());
        // 2. The row is about to end: stop claiming it is alive.
        drop(self.heartbeat.take());
        // 3. `bye {outcome}` and stop reconnecting.
        if let Some(publisher) = &self.publisher {
            publisher.shutdown(Some(outcome.to_string()));
        }
        // 4. Nothing else may publish from here on (the tickers stop).
        self.active.store(false, Ordering::SeqCst);
        // 5. End the row through the observer-based path, so the CLI's
        //    `registry::end_outcome_resolves` applies on BOTH transports.
        let end = (!ctx.replay).then(|| coding::end_session(&ctx.trpc, &ctx.session_id));
        ctx.phase(EnginePhase::Ended);

        let exit = EngineExit {
            session_id: ctx.session_id.clone(),
            outcome: outcome.to_string(),
            child,
            error,
            end,
        };
        ctx.exit.finish(&exit);
        host.on_exit(exit);
        // The host got the full exit; `wait()` rebuilds it from the summary.
        EngineExit {
            session_id: ctx.session_id.clone(),
            outcome: outcome.to_string(),
            child: ctx.child_exit.get(),
            error: None,
            end: None,
        }
    }
}

/// D8: the ids `prepare` could not know, written back onto `runs.json` the
/// moment `session/new` (or `session/load`) answers.
pub(crate) fn record_session_ids(ctx: &SessionCtx) {
    if ctx.replay {
        return;
    }
    let ids = ctx.ids();
    if ids.acp.is_none() && ids.native.is_none() {
        return;
    }
    let Some(mut record) = coding::run_registry::get(&ctx.data_dir, &ctx.session_id) else {
        return;
    };
    if record.acp_session_id == ids.acp && record.agent_native_session_id == ids.native {
        return;
    }
    if ids.acp.is_some() {
        record.acp_session_id = ids.acp;
    }
    if ids.native.is_some() {
        record.agent_native_session_id = ids.native;
    }
    coding::run_registry::record(&ctx.data_dir, record);
}

/// `steer::publish` with the ACP path's hooks. The differences from the PTY
/// path are all consequences of there being no terminal: `write_input`
/// buffers bytes into whole messages instead of typing them, `text_sink` is
/// always present (every message crosses as a message), and `config` is wired
/// — those frames have no keystroke that could express them.
fn attach_publisher(
    ctx: &Arc<SessionCtx>,
    commands: &flume::Sender<EngineCommand>,
    active: Arc<AtomicBool>,
) -> steer::PublisherHandle {
    let (answer_link, answers) = steer::AnswerLink::new();
    let command_link = steer::CommandLink::new(None);
    let config_link = steer::ConfigLink::new();

    let hooks = steer::PublisherHooks {
        write_input: line_buffered_input(commands.clone()),
        // EXP-283: ONLY an explicit relay `kill` frame lands here — socket
        // closes never do.
        kill: {
            let commands = commands.clone();
            Arc::new(move |_signal| {
                let _ = commands.send(EngineCommand::Shutdown { outcome: "killed" });
            })
        },
        error: Arc::new(|message| log::warn!("engine publisher: {message}")),
        answers: Some(Arc::clone(&answer_link)),
        agent: crate::adapters::AdapterKind::from_agent(&ctx.agent).session_agent(),
        text_sink: Some({
            let commands = commands.clone();
            Arc::new(move |text: String| {
                let _ = commands.send(EngineCommand::Steer(text));
            }) as Arc<dyn Fn(String) + Send + Sync>
        }),
        // EXP-511: image embeds in a steered message become local files the
        // agent can read.
        attachments: Some(steer::image_localizer(
            Arc::clone(&ctx.trpc),
            crate::host::steer_images_dir(&ctx.run.worktree),
        )),
        commands: Some(Arc::clone(&command_link)),
        config: Some(Arc::clone(&config_link)),
    };

    let tickets: Arc<dyn steer::PublisherTickets> = Arc::new(steer::TrpcPublisherTickets {
        trpc: Arc::clone(&ctx.trpc),
        coding_session_id: ctx.session_id.clone(),
    });
    let handle = steer::publish(
        &ctx.runtime,
        steer::PublishSpec {
            session_id: ctx.session_id.clone(),
            issue_id: ctx.issue_id.clone(),
        },
        tickets,
        hooks,
    );

    // The three receivers the publisher fills, drained into the ONE command
    // channel the connection loop owns.
    spawn_drain(
        "engine-answers",
        Arc::clone(&active),
        commands.clone(),
        move |commands, active| {
            while active.load(Ordering::SeqCst) {
                match answers.recv_timeout(DRAIN_POLL) {
                    Ok(answer) => {
                        if commands.send(EngineCommand::Answer(answer)).is_err() {
                            return;
                        }
                    }
                    Err(flume::RecvTimeoutError::Timeout) => {}
                    Err(flume::RecvTimeoutError::Disconnected) => return,
                }
            }
        },
    );
    // `CommandLink` exposes only `try_recv` (its receiver is private, unlike
    // `ConfigLink::receiver`), so this one polls instead of blocking. A `/`
    // command is a human keystroke away either way.
    let commands_link = Arc::clone(&command_link);
    spawn_drain(
        "engine-commands",
        Arc::clone(&active),
        commands.clone(),
        move |commands, active| {
            while active.load(Ordering::SeqCst) {
                while let Some(parsed) = commands_link.try_recv() {
                    let sent = commands.send(EngineCommand::Command {
                        name: parsed.command.name.to_string(),
                        args: parsed.args.clone(),
                    });
                    if sent.is_err() {
                        return;
                    }
                }
                std::thread::sleep(DRAIN_POLL);
            }
        },
    );
    let config_rx = config_link.receiver();
    spawn_drain(
        "engine-config",
        Arc::clone(&active),
        commands.clone(),
        move |commands, active| {
        while active.load(Ordering::SeqCst) {
            let change = match config_rx.recv_timeout(DRAIN_POLL) {
                Ok(change) => change,
                Err(flume::RecvTimeoutError::Timeout) => continue,
                Err(flume::RecvTimeoutError::Disconnected) => return,
            };
            let command = match change {
                steer::ConfigChange::Option { id, value } => EngineCommand::SetConfig {
                    id: agent_client_protocol::schema::v1::SessionConfigId::new(id),
                    value: crate::session::ConfigValue::from_wire(&value).into(),
                },
                steer::ConfigChange::Mode { id } => EngineCommand::SetMode(
                    agent_client_protocol::schema::v1::SessionModeId::new(id),
                ),
            };
            if commands.send(command).is_err() {
                return;
            }
        }
    },
    );

    handle
}

/// How often a drain thread wakes: to notice a queued `/` command
/// (`CommandLink` exposes no receiver), and to notice the run ending so no
/// thread outlives the session that owns it.
const DRAIN_POLL: std::time::Duration = std::time::Duration::from_millis(100);

/// The publisher's `write_input` on a path with no keystrokes: a steerer's
/// raw bytes are buffered and submitted as ONE message on `\r`, which is
/// exactly what `text_sink` receives for composer messages. Keystrokes that
/// mean something only to a TUI (an `\x1b` interrupt) become a cancel; every
/// other control byte is dropped.
fn line_buffered_input(commands: flume::Sender<EngineCommand>) -> steer::publisher::InputHook {
    let buffer = std::sync::Mutex::new(String::new());
    Arc::new(move |bytes: &[u8]| {
        let text = String::from_utf8_lossy(bytes);
        let mut buffer = buffer.lock().unwrap_or_else(|err| err.into_inner());
        for ch in text.chars() {
            match ch {
                '\r' | '\n' => {
                    let message = std::mem::take(&mut *buffer);
                    if !message.trim().is_empty() {
                        let _ = commands.send(EngineCommand::Steer(message));
                    }
                }
                // ESC is the TUI interrupt; on ACP it is `session/cancel`.
                '\u{1b}' => {
                    buffer.clear();
                    let _ = commands.send(EngineCommand::Cancel);
                }
                ch if ch.is_control() => {}
                ch => buffer.push(ch),
            }
        }
    })
}

/// One publisher→engine drain thread. Every one of them ends with the run:
/// the `active` flag clears in the end sequence.
fn spawn_drain(
    name: &str,
    active: Arc<AtomicBool>,
    commands: flume::Sender<EngineCommand>,
    body: impl FnOnce(flume::Sender<EngineCommand>, Arc<AtomicBool>) + Send + 'static,
) {
    let _ = std::thread::Builder::new()
        .name(name.to_string())
        .spawn(move || body(commands, active));
}

/// The kill feed: `Now` ends immediately, `AfterTurn` waits up to
/// `steer::STOP_GRACE` for the agent to finish the turn it is mid-way through
/// (EXP-637 — its close-out message is the point of the wait).
fn spawn_kill_pump(
    ctx: &Arc<SessionCtx>,
    kills: flume::Receiver<KillReason>,
    commands: flume::Sender<EngineCommand>,
    active: Arc<AtomicBool>,
) {
    let turn_signal = Arc::clone(&ctx.turn_signal);
    let _ = std::thread::Builder::new()
        .name("engine-kill".to_string())
        .spawn(move || {
            while active.load(Ordering::SeqCst) {
                let reason = match kills.recv_timeout(DRAIN_POLL) {
                    Ok(reason) => reason,
                    Err(flume::RecvTimeoutError::Timeout) => continue,
                    Err(flume::RecvTimeoutError::Disconnected) => return,
                };
                let outcome = match reason {
                    KillReason::Now => "killed",
                    KillReason::AfterTurn => {
                        let waiter = turn_signal.subscribe();
                        let _ = waiter.recv_timeout(steer::STOP_GRACE);
                        "ended"
                    }
                };
                let _ = commands.send(EngineCommand::Shutdown { outcome });
                return;
            }
        });
}

/// The two periodic jobs: the debounced worktree diff (the wire `diff`, same
/// on both transports) and the EXP-214 `needs_input` forward.
fn spawn_tickers(ctx: &Arc<SessionCtx>, active: Arc<AtomicBool>) {
    let ctx = Arc::clone(ctx);
    let _ = std::thread::Builder::new()
        .name("engine-ticks".to_string())
        .spawn(move || {
            let mut diffs = steer::DiffSnapshots::new();
            let mut needs_input = steer::NeedsInputForwarder::new();
            let hook: Option<steer::NeedsInputHook> = {
                let trpc = Arc::clone(&ctx.trpc);
                let session_id = ctx.session_id.clone();
                Some(Arc::new(move |pending| {
                    api::coding_sessions::set_needs_input(&trpc, &session_id, pending).is_ok()
                }))
            };
            while active.load(Ordering::SeqCst) {
                std::thread::sleep(steer::POLL_INTERVAL);
                if !active.load(Ordering::SeqCst) {
                    break;
                }
                needs_input.tick(ctx.needs_input.load(Ordering::SeqCst), &hook);
                // REV2-17: the run's ONE redactor (`SessionCtx.redactor`),
                // never a weaker key-only one — the worktree patch is the
                // likeliest place a launcher secret an agent copied into a
                // tracked file shows up.
                if let Some(event) = diffs.next_diff(
                    &ctx.run.worktree,
                    ctx.run.base_ref.as_deref(),
                    &ctx.redactor,
                ) {
                    let mut out = crate::mapper::MapOut::default();
                    out.local.push(crate::local::LocalFeedEvent::Activity {
                        event: event.clone(),
                        tool_call_id: None,
                    });
                    out.wire.push(event);
                    ctx.dispatch(out);
                }
            }
            // Teardown tidiness: never leave the synced attention flag stuck
            // on a session whose engine is gone.
            needs_input.clear_on_teardown(&hook);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_buffered_line_becomes_one_steer_message() {
        let (tx, rx) = flume::unbounded();
        let hook = line_buffered_input(tx);
        hook(b"hello ");
        hook(b"world");
        assert!(rx.try_recv().is_err(), "a partial line never submits");
        hook(b"\r");
        match rx.try_recv() {
            Ok(EngineCommand::Steer(text)) => assert_eq!(text, "hello world"),
            _ => panic!("expected one steer message"),
        }
    }

    #[test]
    fn an_escape_keystroke_cancels_the_turn() {
        let (tx, rx) = flume::unbounded();
        let hook = line_buffered_input(tx);
        hook(b"half typed");
        hook(b"\x1b");
        assert!(matches!(rx.try_recv(), Ok(EngineCommand::Cancel)));
        // The half-typed draft is gone with it.
        hook(b"\r");
        assert!(rx.try_recv().is_err());
    }
}
