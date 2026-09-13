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
//! - `NeedsInputForwarder` (EXP-214) and its `agent_busy` twin (EXP-848);
//! - the FEED-25 stall watchdog ([`crate::stall`]): a live turn silent for
//!   `STALL_AFTER` gets `Cancel`, one that ignores it for `STALL_KILL_GRACE`
//!   gets `Shutdown` with a `Failed` reason;
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

        spawn_kill_pump(&ctx, kill.rx.clone(), commands.clone(), Arc::clone(&active));
        spawn_tickers(&ctx, Arc::clone(&active), commands);

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
        // 6. EXP-758: this run's child (if it had one) is gone with the
        //    connection, so the record must stop naming it: a pid left here
        //    would make the next start's reaper sweep hunt a dead process,
        //    and a RECYCLED pid is one it could signal by mistake.
        if !ctx.replay {
            upsert_run_record(
                &ctx.data_dir,
                &ctx.session_id,
                &crate::host::SessionIds::default(),
                RunPids::default(),
            );
        }
        // 7. The phases, in the order a view must read them (EXP-758): the
        //    reason first, the end last.
        for phase in end_phases(error.as_deref()) {
            ctx.phase(phase);
        }

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

/// EXP-758: the live-process half of a run record: the ACP child and the
/// host that spawned it. Both set while the run is up, both cleared by the
/// end sequence, so a record still carrying them names an orphan.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RunPids {
    pub(crate) acp_child: Option<u32>,
    pub(crate) host: Option<u32>,
}

/// D8: the ids `prepare` could not know, written back onto `runs.json` the
/// moment `session/new` (or `session/load`) answers, plus the EXP-758 pids,
/// recorded on the same edge because that is the first moment the child is
/// known to have a session at all.
pub(crate) fn record_session_ids(ctx: &SessionCtx) {
    if ctx.replay {
        return;
    }
    let ids = ctx.ids();
    let pids = RunPids {
        acp_child: ctx.child_exit.pid(),
        host: Some(std::process::id()),
    };
    if ids.acp.is_none() && ids.native.is_none() && pids.acp_child.is_none() {
        return;
    }
    upsert_run_record(&ctx.data_dir, &ctx.session_id, &ids, pids);
}

/// The one writer of the engine's fields on a [`coding::run_registry`]
/// record. A present id wins over what is on disk, an absent one leaves it
/// alone, and the pids are written verbatim (the end sequence clears them by
/// passing [`RunPids::default`]). A no-op when nothing changed.
///
/// EXP-781: through [`coding::run_registry::update`], which holds the
/// registry's section across the load, the mutation and the save. Spelled as
/// `get` then `record` it took the lock twice, and a purge landing in the gap
/// (the scratch sweep removing a repo-less run) was undone by the write that
/// followed — a resumable record for a worktree that no longer exists.
pub(crate) fn upsert_run_record(
    data_dir: &std::path::Path,
    session_id: &str,
    ids: &crate::host::SessionIds,
    pids: RunPids,
) {
    coding::run_registry::update(data_dir, session_id, |record| {
        let mut changed = false;
        if ids.acp.is_some() && record.acp_session_id != ids.acp {
            record.acp_session_id = ids.acp.clone();
            changed = true;
        }
        if ids.native.is_some() && record.agent_native_session_id != ids.native {
            record.agent_native_session_id = ids.native.clone();
            changed = true;
        }
        if record.acp_child_pid != pids.acp_child {
            record.acp_child_pid = pids.acp_child;
            changed = true;
        }
        if record.host_pid != pids.host {
            record.host_pid = pids.host;
            changed = true;
        }
        changed
    });
}

/// EXP-758: the phase edges the end sequence emits, in order. An exit that
/// carries an error (a failed spawn, a handshake that never answered, a
/// transport that died) says so FIRST: `EngineExit::error` used to reach the
/// host callback alone, so the CLI printed nothing and the session tab showed
/// an empty transcript that had simply "ended".
pub(crate) fn end_phases(error: Option<&str>) -> Vec<EnginePhase> {
    match error {
        Some(error) => vec![EnginePhase::Failed(error.to_string()), EnginePhase::Ended],
        None => vec![EnginePhase::Ended],
    }
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
    let command_link = steer::CommandLink::new();
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
        agent: crate::adapters::AdapterKind::from_agent(ctx.agent).session_agent(),
        text_sink: Some({
            let commands = commands.clone();
            Arc::new(move |text: String| {
                let _ = commands.send(EngineCommand::Steer(text));
            }) as Arc<dyn Fn(String) + Send + Sync>
        }),
        // EXP-511: image embeds in a steered message become local files the
        // agent can read. EXP-825: the session's ONE hook (the seed prompt
        // used it before this publisher existed).
        attachments: ctx.attachments.clone(),
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
            // EXP-773: every event this run publishes is also appended to
            // `{data_dir}/journal/<sessionId>.jsonl`, so the transcript
            // outlives the process and can be served back on demand.
            journal_dir: Some(ctx.data_dir.clone()),
            // EXP-825: the shared restore map — the seed prompt's images
            // were localized into it before this publisher came up.
            embeds: ctx.embeds.clone(),
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

/// The periodic jobs: the debounced worktree diff (the wire `diff`, same on
/// both transports), the EXP-214 `needs_input` forward and the FEED-25 stall
/// watchdog (`commands` carries its `Cancel` / `Shutdown`).
fn spawn_tickers(
    ctx: &Arc<SessionCtx>,
    active: Arc<AtomicBool>,
    commands: flume::Sender<EngineCommand>,
) {
    let ctx = Arc::clone(ctx);
    let _ = std::thread::Builder::new()
        .name("engine-ticks".to_string())
        .spawn(move || {
            let mut diffs = steer::DiffSnapshots::new();
            let mut needs_input = steer::NeedsInputForwarder::new();
            // EXP-848: the turn mirror — same forwarder, same retry, same
            // clear-on-teardown as `needs_input`.
            let mut agent_busy = steer::AgentBusyForwarder::new();
            // EXP-850 §8: the synced `agent_caption` column — the same
            // forwarder rule plus a 5 s floor, since a workflow's progress
            // moves several times a second and this is a list's second line.
            let mut caption = steer::CaptionForwarder::new();
            let mut blocked = steer::BlockedForwarder::new();
            let mut stall = crate::stall::StallWatchdog::new();
            let hook: Option<steer::NeedsInputHook> = {
                let trpc = Arc::clone(&ctx.trpc);
                let session_id = ctx.session_id.clone();
                Some(Arc::new(move |pending| {
                    api::coding_sessions::set_needs_input(&trpc, &session_id, pending).is_ok()
                }))
            };
            // EXP-848: the synced `agent_busy` column — the ONE input every
            // other client's session list spins its working dot on. The turn
            // signal is already exactly this bool (set false at `start_turn`,
            // true at `on_stop` and at the session's first moment), so the
            // mirror reads it rather than tracking the edges twice.
            let busy_hook: Option<steer::AgentBusyHook> = {
                let trpc = Arc::clone(&ctx.trpc);
                let session_id = ctx.session_id.clone();
                Some(Arc::new(move |busy| {
                    api::coding_sessions::set_agent_busy(&trpc, &session_id, busy).is_ok()
                }))
            };
            // EXP-850 §8: same shape as the busy hook — a failed write is
            // simply not confirmed and the forwarder retries it.
            let caption_hook: Option<steer::CaptionHook> = {
                let trpc = Arc::clone(&ctx.trpc);
                let session_id = ctx.session_id.clone();
                Some(Arc::new(move |caption: Option<&str>| {
                    api::coding_sessions::set_agent_caption(&trpc, &session_id, caption).is_ok()
                }))
            };
            // EXP-804: the same shape as the needs-input hook — a failed
            // write is simply not confirmed, and the forwarder retries it.
            let blocked_hook: Option<steer::BlockedHook> = {
                let trpc = Arc::clone(&ctx.trpc);
                let session_id = ctx.session_id.clone();
                Some(Arc::new(move |wall: Option<&steer::SessionBlocked>| {
                    api::coding_sessions::set_blocked(
                        &trpc,
                        &session_id,
                        wall.map(|wall| api::coding_sessions::BlockedInput {
                            kind: &wall.kind,
                            agent: &wall.agent,
                            window: &wall.window,
                            resets_at: wall.resets_at.as_deref(),
                            since: &wall.since,
                        }),
                    )
                    .is_ok()
                }))
            };
            while active.load(Ordering::SeqCst) {
                std::thread::sleep(steer::POLL_INTERVAL);
                if !active.load(Ordering::SeqCst) {
                    break;
                }
                needs_input.tick(ctx.needs_input.load(Ordering::SeqCst), &hook);
                agent_busy.tick(!ctx.turn_signal.is_idle(), &busy_hook);
                // EXP-850 §8: the caption of the newest RUNNING workflow —
                // `None` while none runs, which is also what a turn end and
                // the teardown below write.
                {
                    let held = ctx.caption_signal.get();
                    caption.tick(held.as_deref(), &caption_hook);
                }
                {
                    // EXP-831 follow-up: a wall whose own reset stamp has
                    // passed is dropped here, so the synced row agrees with
                    // the banner rule every client already applies. An idle
                    // walled run would otherwise keep saying "Rate limited"
                    // in every list long after its window reopened.
                    let wall = ctx
                        .blocked
                        .lock()
                        .ok()
                        .and_then(|wall| wall.clone())
                        .filter(|wall| {
                            !steer::blocked_wall_expired(wall, steer::now_unix_millis())
                        });
                    blocked.tick(wall.as_ref(), &blocked_hook);
                }
                tick_stall(&ctx, &mut stall, &commands);
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
            // EXP-848: a run whose engine is gone is never working.
            agent_busy.clear_on_teardown(&busy_hook);
            // EXP-850 §8: a run whose engine is gone runs no workflow either.
            caption.clear_on_teardown(&caption_hook);
            blocked.clear_on_teardown(&blocked_hook);
        });
}

/// One watchdog tick (FEED-25). Runs on the ticker thread, never in the
/// dispatch loop: the two commands it may send are the same ones Escape and
/// the Stop button send, routed through the inbox like theirs.
fn tick_stall(
    ctx: &SessionCtx,
    stall: &mut crate::stall::StallWatchdog,
    commands: &flume::Sender<EngineCommand>,
) {
    let now = std::time::Instant::now();
    let last_activity = ctx.last_activity();
    // EXP-804: a run behind its agent's usage wall is silent for a REASON, so
    // the watchdog's clock suspends rather than cancelling a turn that was
    // never stuck. Logged when it bites: "silent and not interrupted" is
    // otherwise indistinguishable from a watchdog that stopped working.
    let blocked = ctx
        .blocked
        .lock()
        .ok()
        .map(|wall| wall.is_some())
        .unwrap_or(false);
    let action = stall.tick(crate::stall::StallInput {
        now,
        live: ctx.feed.phase() == Some(EnginePhase::Live),
        idle: ctx.turn_signal.is_idle(),
        needs_input: ctx.needs_input.load(Ordering::SeqCst),
        blocked,
        last_activity,
    });
    if blocked && now.saturating_duration_since(last_activity) >= crate::stall::STALL_AFTER {
        log::info!(
            "engine: session {} silent for {}s behind its agent's usage wall — stall clock suspended",
            ctx.session_id,
            now.saturating_duration_since(last_activity).as_secs()
        );
    }
    let silent = now.saturating_duration_since(last_activity);
    match action {
        crate::stall::StallAction::None => {}
        crate::stall::StallAction::Interrupt => {
            log::warn!(
                "engine: session {} silent for {}s mid-turn — interrupting the stalled turn",
                ctx.session_id,
                silent.as_secs()
            );
            // Say it in the FEED, not just the log: the cancel otherwise
            // shows up as the turn simply stopping, and an unattended run
            // loses that work with nobody able to tell why.
            ctx.notice(crate::stall::StallWatchdog::interrupt_notice(silent));
            // EXP-784: an INTERRUPT, not a Stop — the steers queued behind
            // the wedged turn are what the watchdog is rescuing.
            let _ = commands.send(EngineCommand::Interrupt);
        }
        crate::stall::StallAction::End => {
            log::error!(
                "engine: session {} still silent {}s after the interrupt — ending the run",
                ctx.session_id,
                silent.as_secs()
            );
            ctx.set_failure(crate::stall::StallWatchdog::end_reason(silent));
            let _ = commands.send(EngineCommand::Shutdown { outcome: "ended" });
        }
    }
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

    /// EXP-758: the reason is a phase of its own, and it comes BEFORE the
    /// end: a view that reads the two in order paints the banner and then
    /// closes the session, never the other way round.
    #[test]
    fn a_failed_exit_says_why_before_it_says_ended() {
        assert_eq!(end_phases(None), vec![EnginePhase::Ended]);
        assert_eq!(
            end_phases(Some("initialize timed out")),
            vec![
                EnginePhase::Failed("initialize timed out".to_string()),
                EnginePhase::Ended
            ]
        );
    }

    fn temp_data_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "exp758-lifecycle-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the clock is past the epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("the scratch data dir is creatable");
        dir
    }

    /// A record with only the fields `prepare` writes; the rest defaults.
    fn seed_record(data_dir: &std::path::Path, session_id: &str) {
        let record: coding::run_registry::RunRecord = serde_json::from_value(serde_json::json!({
            "sessionId": session_id,
            "accountId": "acct-1",
            "agent": "claude",
            "kind": "issue",
            "cwd": "/tmp/worktree",
            "transport": "acp",
            // Inside the registry's TTL, or the next write would prune it.
            "recordedAt": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the clock is past the epoch")
                .as_secs(),
        }))
        .expect("the seed record decodes");
        coding::run_registry::record(data_dir, record);
    }

    /// EXP-758: a host that dies without its end sequence (Cmd-Q, a crash)
    /// leaves a codex child with no `claude-hooks` anchor on it, so the
    /// record has to name the pid while the run is live, and stop naming it
    /// the moment the run ends, or the next start's reaper hunts a pid the OS
    /// has since handed to somebody else.
    #[test]
    fn the_run_record_names_the_child_while_it_lives_and_forgets_it_after() {
        let data_dir = temp_data_dir("pids");
        seed_record(&data_dir, "sess-1");

        let ids = crate::host::SessionIds {
            acp: Some("acp-1".to_string()),
            native: Some("native-1".to_string()),
        };
        upsert_run_record(
            &data_dir,
            "sess-1",
            &ids,
            RunPids {
                acp_child: Some(4242),
                host: Some(std::process::id()),
            },
        );
        let live = coding::run_registry::get(&data_dir, "sess-1").expect("the record is there");
        assert_eq!(live.acp_session_id.as_deref(), Some("acp-1"));
        assert_eq!(live.agent_native_session_id.as_deref(), Some("native-1"));
        assert_eq!(live.acp_child_pid, Some(4242));
        assert_eq!(live.host_pid, Some(std::process::id()));

        // The end sequence: pids gone, the ids it learned kept.
        upsert_run_record(
            &data_dir,
            "sess-1",
            &crate::host::SessionIds::default(),
            RunPids::default(),
        );
        let ended = coding::run_registry::get(&data_dir, "sess-1").expect("the record is there");
        assert_eq!(ended.acp_child_pid, None);
        assert_eq!(ended.host_pid, None);
        assert_eq!(ended.acp_session_id.as_deref(), Some("acp-1"));
        assert_eq!(ended.agent_native_session_id.as_deref(), Some("native-1"));

        // A session with no record of its own is left alone.
        upsert_run_record(
            &data_dir,
            "sess-unknown",
            &ids,
            RunPids {
                acp_child: Some(7),
                host: Some(8),
            },
        );
        assert!(coding::run_registry::get(&data_dir, "sess-unknown").is_none());
        let _ = std::fs::remove_dir_all(&data_dir);
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
