//! The headless session host — the CLI's replacement for the desktop's
//! `TerminalManager` tab + `spawn_prepared_with` + `attach_publisher`
//! wiring: a real PTY (`terminal` core; the agent runs its interactive TUI
//! exactly like in the desktop dock), the emulator pump loop, the session
//! heartbeat, the steer publisher + per-agent activity emitter, and the
//! own-row kill-switch (tRPC poll of `codingSessions.get` — no Electric
//! sync here).
//!
//! EXP-746 added a SECOND transport: `launch` dispatches on
//! [`coding::LaunchTransport`] into [`launch_pty`] (everything above,
//! unchanged) or [`launch_acp`], which hands the run to the in-process ACP
//! engine and keeps only the two things this host owns either way — the
//! registry entry and the tRPC kill poll. [`RunningSession`]'s public shape
//! is identical across both, so `daemon.rs`'s `LiveSession` bookkeeping, its
//! 1 Hz reap block and the quit sweep never learn there are two.
//!
//! What the engine owns on the ACP arm (D14) and this file therefore does
//! NOT: the publisher (attach, `bye`, shutdown), the heartbeat, the activity
//! vocabulary and `coding::end_session`. What stays here: the kill DECISION
//! ([`kill_poll_decision`], EXP-681's [`GATED_KILL_AFTER`]) — the engine only
//! consumes decided edges through an [`engine::KillFeed`].

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context as _;
use coding::{CodingAgent, PreparedLaunch};
use steer::publisher::pty_writer_input_hook;
use steer::{
    AnswerLink, CommandLink, EmitterConfig, PublishSpec, PublisherHooks, PublisherTickets,
    SteerRuntime, Steering, TrpcPublisherTickets,
};
use terminal::emulator::Emulator;
use terminal::pty::{self, ChildExit};
use terminal::read_loop::{spawn_read_loop, Wake};

use crate::context::Ctx;
use crate::registry;
use crate::sidecars::Sidecars;

/// Detached grid size — a roomy default; the TUI pickers and the activity
/// grid watchers work at any sane geometry. Interactive attaches use the
/// real terminal size instead.
const DETACHED_COLS: u16 = 120;
const DETACHED_ROWS: u16 = 36;

/// Kill-switch poll cadence — the desktop reads the →ended edge off its
/// Electric sync in real time; the daemon polls. Snappy enough for a web
/// "Kill" click, trivial load for the server.
const KILL_POLL_INTERVAL: Duration = Duration::from_secs(15);

pub struct LaunchEnv<'a> {
    pub ctx: &'a Ctx,
    /// `None` = steer publishing off (no runtime); the session still runs.
    pub runtime: Option<&'a Arc<SteerRuntime>>,
    pub sidecars: &'a Sidecars,
    /// The `expu_` personal key — the activity redactor's exact-match
    /// secret (codex/pi carry it env-only; REV2-17).
    pub personal_key: Option<String>,
}

enum Control {
    Kill { outcome: &'static str },
    Resize(u16, u16),
}

/// EXP-746: which engine a [`RunningSession`] rides. The public methods are
/// the same on both — the PTY-only ones (`write_stdin`, `resize`) degrade to
/// no-ops on the ACP arm, where the composer is [`RunningSession::send_prompt`]
/// and there is no grid to resize.
enum Backend {
    Pty {
        done_rx: flume::Receiver<ChildExit>,
        control_tx: flume::Sender<Control>,
        writer: Arc<Mutex<Box<dyn std::io::Write + Send>>>,
    },
    Acp {
        session: engine::EngineSession,
    },
}

/// The pure half of [`Backend`] — which capabilities a transport has. Split
/// out so the dispatch rules are unit-testable without an engine session
/// (constructing one spawns an agent).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BackendKind {
    Pty,
    Acp,
}

impl BackendKind {
    /// Raw bytes reach a child only through a PTY.
    fn supports_stdin(self) -> bool {
        matches!(self, BackendKind::Pty)
    }

    /// Only a PTY has a grid whose geometry can change.
    fn supports_resize(self) -> bool {
        matches!(self, BackendKind::Pty)
    }

    /// The ACP arm takes whole MESSAGES ([`RunningSession::send_prompt`]);
    /// the PTY arm types them as keystrokes and needs a trailing `\r`.
    fn steers_by_message(self) -> bool {
        matches!(self, BackendKind::Acp)
    }
}

impl From<coding::LaunchTransport> for BackendKind {
    fn from(transport: coding::LaunchTransport) -> Self {
        match transport {
            coding::LaunchTransport::Terminal => BackendKind::Pty,
            coding::LaunchTransport::Acp => BackendKind::Acp,
        }
    }
}

impl Backend {
    fn kind(&self) -> BackendKind {
        match self {
            Backend::Pty { .. } => BackendKind::Pty,
            Backend::Acp { .. } => BackendKind::Acp,
        }
    }
}

pub struct RunningSession {
    pub session_id: String,
    pub issue_identifier: String,
    pub worktree: PathBuf,
    pub branch: String,
    /// EXP-746: which vocabulary the local attach speaks to this run — the
    /// contract catalog is agent-scoped (`steer::commands::catalog_for`).
    pub agent: steer::SessionAgent,
    /// EXP-758 (EXP-478): the clone's launch gate, held since before the
    /// worktree existed. It rides HERE, not in `launch`, because the branch
    /// is only protected once its OWNER has registered the run: the daemon's
    /// `held` set is built from the live-session list (`run_device_command`),
    /// which `launch` returns BEFORE being pushed into. Dropping the hold
    /// inside `launch` left exactly that window open, and a `worktree_prune`
    /// landing in it sees a branch with no unique commits and no live session
    /// and removes the worktree under a run that just started. Every owner
    /// calls [`RunningSession::release_launch_hold`] right after its own
    /// registration point (desktop parity: `ui/src/coding_flow.rs` inserts
    /// into `LocalSessions`, THEN drops); a session dropped without that
    /// releases by RAII anyway.
    launch_hold: Mutex<Option<coding::LaunchHold>>,
    backend: Backend,
}

impl RunningSession {
    /// EXP-758: the run is registered with its owner: the auto-prune can
    /// see it now, so the launch gate is free. Idempotent.
    pub fn release_launch_hold(&self) {
        if let Ok(mut hold) = self.launch_hold.lock() {
            drop(hold.take());
        }
    }

    /// Whether this run still parks the prune on its clone (test seam).
    #[cfg(test)]
    pub(crate) fn holds_launch_gate(&self) -> bool {
        self.launch_hold
            .lock()
            .map(|hold| hold.is_some())
            .unwrap_or(false)
    }
}

/// EXP-758 test seam: a session whose backend is an already-dead PTY, for the
/// registration-ORDER tests here and in `daemon` (a real one spawns an agent).
#[cfg(test)]
pub(crate) fn test_session(
    session_id: &str,
    launch_hold: Option<coding::LaunchHold>,
) -> RunningSession {
    let (control_tx, _control_rx) = flume::unbounded::<Control>();
    let (_done_tx, done_rx) = flume::bounded::<ChildExit>(1);
    let writer: Arc<Mutex<Box<dyn std::io::Write + Send>>> =
        Arc::new(Mutex::new(Box::new(Vec::new())));
    RunningSession {
        session_id: session_id.to_string(),
        issue_identifier: "EXP-1".to_string(),
        worktree: PathBuf::from("/tmp/exp-test-worktree"),
        branch: "exp/EXP-1".to_string(),
        agent: steer::SessionAgent::Claude,
        launch_hold: Mutex::new(launch_hold),
        backend: Backend::Pty { done_rx, control_tx, writer },
    }
}

impl RunningSession {
    /// EXP-746: whether the local attach is the raw byte tee (PTY) or the
    /// line transcript + line composer (ACP) — `code` and `run` pick their
    /// attach with it.
    pub fn attaches_by_line(&self) -> bool {
        self.backend.kind().steers_by_message()
    }

    /// Whether raw stdin bytes reach the agent at all.
    pub fn supports_stdin(&self) -> bool {
        self.backend.kind().supports_stdin()
    }

    /// Whether the agent has a grid whose geometry can change.
    pub fn supports_resize(&self) -> bool {
        self.backend.kind().supports_resize()
    }

    /// Block until the agent child exits. Half of the exit contract this
    /// handle mirrors from `engine::EngineSession`; every caller in here
    /// wants the failure text too and goes through `wait_detailed`.
    #[allow(dead_code)]
    pub fn wait(&self) -> ChildExit {
        self.wait_detailed().child
    }

    /// EXP-758: [`RunningSession::wait`] plus the engine's failure text.
    /// `ChildExit`, the vocabulary every caller of this module already
    /// speaks, has no room for [`engine::EngineExit::error`], so a
    /// handshake or transport failure reached the CLI only as "exit -1".
    /// The attaches wait through here and print it. Always `None` on the
    /// PTY arm: a spawned child's failure IS its exit code.
    pub fn wait_detailed(&self) -> SessionExit {
        match &self.backend {
            Backend::Pty { done_rx, .. } => SessionExit {
                child: done_rx
                    .recv()
                    .unwrap_or(ChildExit { code: -1, success: false, signal: None }),
                error: None,
            },
            Backend::Acp { session } => session_exit(&session.wait()),
        }
    }

    /// `Some(exit)` once the child has exited, `None` on timeout.
    pub fn wait_timeout(&self, timeout: Duration) -> Option<ChildExit> {
        self.wait_timeout_detailed(timeout).map(|exit| exit.child)
    }

    /// [`RunningSession::wait_detailed`] with a bound (EXP-758).
    pub fn wait_timeout_detailed(&self, timeout: Duration) -> Option<SessionExit> {
        match &self.backend {
            Backend::Pty { done_rx, .. } => {
                done_rx.recv_timeout(timeout).ok().map(|child| SessionExit {
                    child,
                    error: None,
                })
            }
            Backend::Acp { session } => {
                session.wait_timeout(timeout).map(|exit| session_exit(&exit))
            }
        }
    }

    pub fn is_done(&self) -> bool {
        match &self.backend {
            // The supervisor sends the exit into the bounded(1) channel and
            // then drops its sender — flume keeps the buffered message after
            // disconnect, so "done" is EITHER an exit waiting to be read OR a
            // dead channel (supervisor gone). `disconnected && empty` alone
            // never fires for a finished-but-unwaited daemon session, which
            // jammed the reaper and every dedup gate (review finding,
            // EXP-403).
            Backend::Pty { done_rx, .. } => !done_rx.is_empty() || done_rx.is_disconnected(),
            Backend::Acp { session } => session.is_done(),
        }
    }

    pub fn kill(&self) {
        match &self.backend {
            Backend::Pty { control_tx, .. } => {
                let _ = control_tx.send(Control::Kill { outcome: "ended" });
            }
            // Same `bye` outcome the PTY supervisor publishes, so a viewer
            // cannot tell the transports apart.
            Backend::Acp { session } => session.kill("ended"),
        }
    }

    /// Local stdin bytes (interactive attach) — the same shared PTY writer
    /// remote steer input uses; the child cannot tell them apart. A no-op on
    /// the ACP arm: there is no byte stream to write into (EXP-746), and its
    /// attach composes whole messages with [`RunningSession::send_prompt`].
    pub fn write_stdin(&self, bytes: &[u8]) {
        let Backend::Pty { writer, .. } = &self.backend else {
            debug_assert!(!self.supports_stdin());
            return;
        };
        if let Ok(mut writer) = writer.lock() {
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
        }
    }

    /// A no-op on the ACP arm — it has no grid.
    pub fn resize(&self, cols: u16, rows: u16) {
        let Backend::Pty { control_tx, .. } = &self.backend else {
            debug_assert!(!self.supports_resize());
            return;
        };
        let _ = control_tx.send(Control::Resize(cols, rows));
    }

    /// EXP-746: one whole message from the local attach — a fresh prompt
    /// between turns, steering mid-turn. On the PTY arm it is typed into the
    /// TUI exactly like a remote `input` frame (text then `\r`).
    pub fn send_prompt(&self, text: String) {
        match &self.backend {
            Backend::Pty { .. } => {
                self.write_stdin(text.as_bytes());
                self.write_stdin(b"\r");
            }
            Backend::Acp { session } => session.send_prompt(text),
        }
    }

    /// EXP-746: answer a pending question card. PTY answers ride the relay's
    /// keystroke choreography instead, so this is ACP-only.
    pub fn answer(&self, answer: steer::RemoteAnswer) {
        if let Backend::Acp { session } = &self.backend {
            session.answer(answer);
        }
    }

    /// EXP-746: a `/` command from the local attach — the same sink a remote
    /// one takes. ACP-only for the same reason as [`RunningSession::answer`].
    pub fn run_command(&self, name: &str, args: &str) {
        if let Backend::Acp { session } = &self.backend {
            session.run_command(name, args);
        }
    }

    /// EXP-746: the engine's local feed, for `exponential code`'s line
    /// printer. `None` on the PTY arm, whose attach is the raw byte tee.
    pub fn feed(&self) -> Option<flume::Receiver<engine::LocalFeedEvent>> {
        match &self.backend {
            Backend::Pty { .. } => None,
            Backend::Acp { session } => Some(session.subscribe()),
        }
    }
}

/// EXP-758: how a run ended, for the callers that RENDER it: the child exit
/// every caller already speaks, plus the engine's failure text when the run
/// never got a child to fail (a handshake or transport error).
pub struct SessionExit {
    pub child: ChildExit,
    /// [`engine::EngineExit::error`]; always `None` on the PTY arm.
    pub error: Option<String>,
}

/// EXP-758: [`engine_child_exit`] keeping the error the exit line prints.
fn session_exit(exit: &engine::EngineExit) -> SessionExit {
    SessionExit {
        child: engine_child_exit(exit),
        error: exit.error.clone(),
    }
}

/// EXP-746: an engine exit in the `ChildExit` vocabulary every caller of this
/// module already speaks (`code`/`run` print it, `daemon` logs it). A run
/// whose adapter owned a child reports the child's real exit; a handshake
/// failure that never got one reports failure; every other end (a kill, the
/// agent's own close-out) is a clean zero.
fn engine_child_exit(exit: &engine::EngineExit) -> ChildExit {
    if let Some(child) = &exit.child {
        return child.clone();
    }
    let failed = exit.error.is_some();
    ChildExit {
        code: if failed { -1 } else { 0 },
        success: !failed,
        signal: None,
    }
}

/// Everything after `coding::prepare_with_hooks` said `Ready`. Dispatches on
/// the transport `prepare` resolved (EXP-746): the PTY host below, or the ACP
/// engine. `interactive` is a PTY concern only — it mirrors the raw bytes to
/// stdout and leaves terminal query replies (DA/DSR) to the REAL terminal.
pub fn launch(
    env: &LaunchEnv,
    prepared: PreparedLaunch,
    interactive: bool,
    // The steer room's issue id — `Some` only for real issue sessions
    // (batch/action rooms send none; desktop parity).
    issue_id: Option<String>,
) -> anyhow::Result<RunningSession> {
    match prepared.transport {
        coding::LaunchTransport::Terminal => launch_pty(env, prepared, interactive, issue_id),
        coding::LaunchTransport::Acp => launch_acp(env, prepared, issue_id),
    }
}

/// The PTY host: spawn the agent on a fresh PTY and wire
/// heartbeat/publisher/emitter/kill-watch. `interactive` mirrors the raw PTY
/// bytes to stdout and leaves terminal query replies (DA/DSR) to the REAL
/// terminal; detached lets the emulator answer them (the child's queries must
/// never hang).
fn launch_pty(
    env: &LaunchEnv,
    mut prepared: PreparedLaunch,
    interactive: bool,
    issue_id: Option<String>,
) -> anyhow::Result<RunningSession> {
    // EXP-758 (EXP-478): out FIRST, because the destructure below would
    // drop the gate pre-spawn, which is the ACP arm's bug in a quieter
    // shape. It rides the returned session until the owner registers it
    // ([`RunningSession::release_launch_hold`]); every `?` between here and
    // the return releases it by RAII.
    let launch_hold = prepared.launch_hold.take();
    let PreparedLaunch {
        session_id,
        issue_identifier,
        worktree,
        clone,
        repository_id,
        branch,
        base_ref,
        spawn,
        heartbeat_scope,
        bypass_permissions,
        plan_mode,
        agent,
        claude_session_id,
        codex_originator,
        codex_resume_id,
        ..
    } = prepared;
    // EXP-444/EXP-432: snapshot the relay requester BEFORE the heartbeat
    // thread takes ownership of the scope — a start whose requester is not
    // this daemon's account runs on a shared host, and the emitter
    // suppresses the remote login flow for it.
    let foreign_host = heartbeat_scope
        .started_by_id
        .as_deref()
        .is_some_and(|requester| requester != env.ctx.account.user_id);

    let (cols, rows) = if interactive {
        crate::term::window_size().unwrap_or((DETACHED_COLS, DETACHED_ROWS))
    } else {
        (DETACHED_COLS, DETACHED_ROWS)
    };

    // --- PTY + emulator + read loop + wait thread (session.rs, plus the
    // interactive stdout tee the desktop never needs) -----------------------
    // Graphics stay disabled here (EXP-636): image protocols are parsed so
    // the grid stays consistent, but no pixels are ever retained headlessly.
    let emulator = Emulator::new(cols, rows);
    let mut pty = match pty::open(&spawn, cols.max(1), rows.max(1)) {
        Ok(pty) => pty,
        Err(err) => {
            // The `running` row exists (step 6 ran) but no child will ever
            // exit — end it now or the badge ghosts forever.
            let _ = api::coding_sessions::end(&env.ctx.trpc, &session_id);
            return Err(err).context("spawn the agent PTY");
        }
    };
    let (wake_tx, wake_rx) = flume::unbounded();
    let reader = pty.take_reader();
    let reader: Box<dyn std::io::Read + Send> = if interactive {
        Box::new(TeeReader { inner: reader })
    } else {
        reader
    };
    let read_thread = spawn_read_loop(reader, emulator.term(), wake_tx.clone());
    let (exit_slot, wait_thread) = match pty.spawn_wait_thread(wake_tx) {
        Ok(parts) => parts,
        Err(err) => {
            pty.kill();
            let _ = api::coding_sessions::end(&env.ctx.trpc, &session_id);
            return Err(err).context("spawn the PTY wait thread");
        }
    };

    registry::record(&env.ctx.data_dir, &session_id, &env.ctx.account.id);

    // --- Liveness heartbeat: EXP-746 replaced this host's copy with the
    // shared `coding::start_heartbeat` (same immediate first beat, same
    // 30-minute cadence, same swept-row scope) — the ACP engine runs the
    // very same one. Dropping the handle in the teardown ends the thread.
    let heartbeat_stop = coding::start_heartbeat(
        Arc::clone(&env.ctx.trpc),
        session_id.clone(),
        heartbeat_scope,
    );

    let (control_tx, control_rx) = flume::unbounded::<Control>();
    let (done_tx, done_rx) = flume::bounded::<ChildExit>(1);
    let writer = pty.writer();
    let session_agent = match agent {
        CodingAgent::Claude => steer::SessionAgent::Claude,
        CodingAgent::Codex => steer::SessionAgent::Codex,
        CodingAgent::Pi => steer::SessionAgent::Pi,
    };

    // --- Steer publisher + activity emitter (attach_publisher parity) ------
    let mut publisher: Option<steer::PublisherHandle> = None;
    let activity_active = Arc::new(AtomicBool::new(true));
    // EXP-637: the emitter flips it on every turn boundary; the kill-poll's
    // graceful path waits on it.
    let turn_signal = Arc::new(steer::TurnSignal::new());
    if let Some(runtime) = env.runtime {
        let term = emulator.term();
        let write_input = pty_writer_input_hook(writer.clone(), term.clone());
        let (answer_link, answers) = AnswerLink::new();
        let is_claude = agent == CodingAgent::Claude;
        // EXP-455: keystroke-choreographed remote answering — claude's
        // pickers and codex's approval modals. Pi steers through its
        // observer extension, but its plan-approval confirm dialog
        // (EXP-441) resolves by keystroke too.
        let steers_by_keystroke = is_claude || agent == CodingAgent::Codex;
        let answers_remotely = steers_by_keystroke || agent == CodingAgent::Pi;
        let pi_subscription = (agent == CodingAgent::Pi)
            .then(|| env.sidecars.subscribe_pi(&worktree))
            .flatten();
        let (pi_events, pi_steer) = match pi_subscription {
            Some((events, steer_handle)) => (Some(events), Some(steer_handle)),
            None => (None, None),
        };
        // EXP-724: the remote slash-command seam. Pi's commands run through
        // its observer extension (never the PTY), so its sink is the same
        // steer queue the text sink pushes into; claude and codex get no
        // sink and are typed into by the emitter.
        let command_link = CommandLink::new(pi_steer.clone().map(|handle| {
            Arc::new(move |name: &str, args: &str| handle.push_command(name, args))
                as steer::CommandSink
        }));
        let kill_tx = control_tx.clone();
        let hooks = PublisherHooks {
            write_input: write_input.clone(),
            // EXP-283: ONLY an explicit relay `kill` frame lands here —
            // socket closes never do.
            kill: Arc::new(move |_signal| {
                let _ = kill_tx.send(Control::Kill { outcome: "killed" });
            }),
            error: Arc::new(|message| log::warn!("steer publisher: {message}")),
            answers: answers_remotely.then(|| Arc::clone(&answer_link)),
            agent: session_agent,
            text_sink: pi_steer.map(|handle| {
                Arc::new(move |text: String| handle.push(text)) as Arc<dyn Fn(String) + Send + Sync>
            }),
            // EXP-511 (attach_publisher parity): image embeds in a steered
            // message become local files the agent can read.
            attachments: Some(steer::image_localizer(
                Arc::clone(&env.ctx.trpc),
                worktree.join(coding::launcher::STEER_IMAGES_DIR),
            )),
            commands: Some(Arc::clone(&command_link)),
            config: None,
        };
        let tickets: Arc<dyn PublisherTickets> = Arc::new(TrpcPublisherTickets {
            trpc: Arc::clone(&env.ctx.trpc),
            coding_session_id: session_id.clone(),
        });
        let spec = PublishSpec {
            session_id: session_id.clone(),
            issue_id: issue_id.clone(),
        };
        let handle = steer::publish(runtime, spec, tickets, hooks);

        let needs_input_trpc = Arc::clone(&env.ctx.trpc);
        let needs_input_session = session_id.clone();
        steer::spawn_activity_emitter(
            EmitterConfig {
                agent: session_agent,
                worktree: worktree.clone(),
                // EXP-688: the published diff is measured from the branch's
                // base, so it stays the PR's content after the agent commits.
                base_ref: base_ref.clone(),
                term: Some(term),
                extra_secrets: env.personal_key.iter().cloned().collect(),
                on_needs_input: Some(Arc::new(move |pending| {
                    api::coding_sessions::set_needs_input(
                        &needs_input_trpc,
                        &needs_input_session,
                        pending,
                    )
                    .is_ok()
                })),
                hooks: is_claude
                    .then(|| {
                        env.sidecars
                            .subscribe_hooks(&worktree, claude_session_id.as_deref())
                    })
                    .flatten(),
                steering: answers_remotely.then(|| Steering {
                    answers,
                    link: answer_link,
                    write_input,
                    commands: Some(command_link),
                }),
                pi_events,
                bypass_permissions,
                plan_mode,
                claude_session_id: claude_session_id.clone(),
                codex_originator: codex_originator.clone(),
                codex_resume_id: codex_resume_id.clone(),
                foreign_host,
                // EXP-637: the graceful-stop signal — an agent that ended
                // its own run finishes writing its close-out before the
                // kill-poll tears the child down.
                turn_signal: Some(Arc::clone(&turn_signal)),
            },
            handle.activity_sender(),
            Arc::clone(&activity_active),
        );
        publisher = Some(handle);
    }

    // The PTY arm's kill sink: wait out the agent's own close-out here (this
    // host owns the emitter's `TurnSignal`), then tell the supervisor.
    let watch_done = spawn_kill_watch(env, &session_id, {
        let kill_tx = control_tx.clone();
        let kill_turn_signal = Arc::clone(&turn_signal);
        Box::new(move |graceful| {
            if graceful {
                // EXP-637: the agent said it was done — let it finish the
                // turn it is mid-way through (its close-out message), then
                // kill. The grace is the bound for an idle edge that never
                // arrives.
                let waiter = kill_turn_signal.subscribe();
                let _ = waiter.recv_timeout(STOP_GRACE);
            }
            let _ = kill_tx.send(Control::Kill {
                outcome: if graceful { "ended" } else { "killed" },
            });
        })
    });

    // EXP-447: the CLI counterpart of the desktop's `TokenRefreshers` — the
    // launch-time token dies after GitHub's hard 1h cap, and the credential
    // helper is a deliberate dumb `cat`, so a session outliving the token
    // would strand the agent's `git push`. Repo-less runs have no clone and
    // nothing to refresh.
    let refresher_hold = repository_id.as_deref().map(|repository_id| {
        coding::clone_refreshers().retain(Arc::clone(&env.ctx.trpc), repository_id, &clone)
    });

    // --- The supervisor: pump loop + control events (manager.rs parity:
    // coalesce wake bursts, pump every wake, final sweep after close). ------
    let supervisor_ctx = SupervisorCtx {
        trpc: Arc::clone(&env.ctx.trpc),
        session_id: session_id.clone(),
        heartbeat_stop,
        publisher,
        activity_active,
        watch_done,
        interactive,
        _refresher_hold: refresher_hold,
    };
    {
        let done_tx = done_tx;
        std::thread::Builder::new()
            .name("exp-session".to_string())
            .spawn(move || {
                supervise(supervisor_ctx, emulator, pty, wake_rx, control_rx, exit_slot, done_tx);
                // Bounded joins, then detach (session.rs shutdown rules).
                join_bounded(read_thread);
                join_bounded(wait_thread);
            })
            .context("spawn the session supervisor thread")?;
    }

    Ok(RunningSession {
        session_id,
        issue_identifier,
        worktree,
        branch,
        agent: session_agent,
        launch_hold: Mutex::new(launch_hold),
        backend: Backend::Pty { done_rx, control_tx, writer },
    })
}

/// EXP-746: the ACP host. Everything the PTY arm above does with a terminal —
/// spawn, pump, publisher, emitter, heartbeat, end — belongs to the engine
/// here (D14). This function owns exactly the two facts that are the CLI's
/// and not the engine's: the crash-recovery registry entry and the tRPC kill
/// poll's DECISION.
fn launch_acp(
    env: &LaunchEnv,
    mut prepared: PreparedLaunch,
    issue_id: Option<String>,
) -> anyhow::Result<RunningSession> {
    let session_id = prepared.session_id.clone();
    // Unreachable by construction: `resolve_transport` only answers `Acp`
    // when `CodingDeps::acp_available` said this host has a runtime
    // (`launch::coding_deps`). There is deliberately no spawn-time fallback —
    // an ACP-prepared launch carries an EMPTY `spawn.args`, so there is no
    // TUI invocation left to run. EXP-758: a remote RESUME reaches it anyway
    // (a run recorded as ACP re-enters the engine even on a runtime-less
    // host), and this return used to be the ONE launch failure that left the
    // `running` row behind, a ghost badge until the server sweep.
    let Some(runtime) = env.runtime else {
        return Err(fail_before_start(
            anyhow::anyhow!("the ACP engine needs the steer runtime"),
            || coding::end_session_best_effort(&env.ctx.trpc, &session_id),
        ));
    };

    let issue_identifier = prepared.issue_identifier.clone();
    let worktree = prepared.worktree.clone();
    let branch = prepared.branch.clone();
    let repository_id = prepared.repository_id.clone();
    let clone = prepared.clone.clone();
    let session_agent = acp_session_agent(&prepared);
    // EXP-444/EXP-432: a start whose requester is not this daemon's account
    // runs on a shared host — snapshot it before the engine takes ownership.
    let foreign_host = prepared
        .heartbeat_scope
        .started_by_id
        .as_deref()
        .is_some_and(|requester| requester != env.ctx.account.user_id);

    registry::record(&env.ctx.data_dir, &session_id, &env.ctx.account.id);

    // EXP-447: the launch-time GitHub token dies after the hard 1h cap and
    // the credential helper is a deliberate dumb `cat`, so the clone's
    // ambient auth is kept fresh for the run's life. The hold moves into the
    // host and drops in `on_exit` — the ACP arm has no supervisor thread to
    // scope it to.
    let refresher_hold = repository_id.as_deref().map(|repository_id| {
        coding::clone_refreshers().retain(Arc::clone(&env.ctx.trpc), repository_id, &clone)
    });

    // EXP-283: registered BEFORE `engine::start`, so no `ended` edge can slip
    // through between the row's creation and the first frame. The engine
    // drops the feed FIRST in its end sequence, which unwatches — our own end
    // flip can never bounce back as a second kill.
    let kill = trpc_kill_feed(env, &session_id);

    // EXP-478: the launch gate has to outlive registration, and
    // `EngineStart`'s destructure would drop it pre-spawn, so it comes out
    // here (`engine::start` asserts it did) and rides the returned session.
    // EXP-758: it is released by the OWNER, not here: a `drop` at the end of
    // this function is still pre-registration.
    let launch_hold = prepared.launch_hold.take();

    let host = Arc::new(CliEngineHost {
        data_dir: env.ctx.data_dir.clone(),
        refresher_hold: Mutex::new(refresher_hold),
    });
    let session = engine::start(
        engine::EngineStart {
            prepared,
            trpc: Arc::clone(&env.ctx.trpc),
            runtime: Arc::clone(runtime),
            data_dir: env.ctx.data_dir.clone(),
            account_id: env.ctx.account.id.clone(),
            own_user_id: Some(env.ctx.account.user_id.clone()),
            personal_key: env.personal_key.clone(),
            issue_id,
            foreign_host,
            // A runtime-less host never gets here, so the room is always on.
            publish: true,
            kill,
            // The line printer subscribes on demand (`RunningSession::feed`);
            // a detached daemon run wants no local fan-out at all.
            local_sink: None,
        },
        host,
    )
    .map_err(|err| {
        // Nothing ran, so nothing will end the row: do it here or the badge
        // ghosts until the server sweep (PTY parity, `pty::open` above).
        fail_before_start(anyhow::anyhow!("{err}"), || {
            coding::end_session_best_effort(&env.ctx.trpc, &session_id)
        })
    })?;

    Ok(RunningSession {
        session_id,
        issue_identifier,
        worktree,
        branch,
        agent: session_agent,
        // EXP-758: NOT dropped here: the owner releases it once the run is
        // registered (see [`RunningSession::launch_hold`]).
        launch_hold: Mutex::new(launch_hold),
        backend: Backend::Acp { session },
    })
}

/// EXP-758: the ACP arm's early-failure sequence. `prepare`'s step 6 already
/// created the `running` row, so EVERY path out of [`launch_acp`] that never
/// reaches the engine has to end it first, since the engine owns
/// `coding::end_session` from `engine::start` onwards, and nothing else will.
/// Pure in the end action so the ordering is unit-testable without a server.
fn fail_before_start(err: anyhow::Error, end_row: impl FnOnce()) -> anyhow::Error {
    end_row();
    err
}

/// EXP-746: which steer agent vocabulary an ACP run speaks. An EXTERNAL agent
/// (D13) is deliberately its own value — the contract's curated `/` catalog
/// describes claude/codex/pi behaviour we verified, so an external agent's
/// menu carries only what it advertised itself.
fn acp_session_agent(prepared: &PreparedLaunch) -> steer::SessionAgent {
    let external = prepared
        .acp
        .as_ref()
        .is_some_and(|acp| acp.options.external.is_some());
    if external {
        return steer::SessionAgent::External;
    }
    match prepared.agent {
        CodingAgent::Claude => steer::SessionAgent::Claude,
        CodingAgent::Codex => steer::SessionAgent::Codex,
        CodingAgent::Pi => steer::SessionAgent::Pi,
    }
}

/// EXP-746: the ACP arm's kill source — today's poll thread, producing
/// [`engine::KillFeed`] edges instead of `Control::Kill`. The engine does the
/// after-turn wait itself (D14: `STOP_GRACE` on its own `TurnSignal`), so
/// this sink never blocks — it only decides.
fn trpc_kill_feed(env: &LaunchEnv, session_id: &str) -> engine::KillFeed {
    let (tx, rx) = flume::unbounded::<engine::KillReason>();
    let watch_done = spawn_kill_watch(
        env,
        session_id,
        Box::new(move |graceful| {
            let _ = tx.send(if graceful {
                engine::KillReason::AfterTurn
            } else {
                engine::KillReason::Now
            });
        }),
    );
    engine::KillFeed {
        rx,
        unwatch: Some(Box::new(move || watch_done.store(true, Ordering::SeqCst))),
    }
}

/// EXP-746: the CLI's `EngineHost`. The engine owns the publisher, the
/// heartbeat and `coding::end_session`; this is what is left over.
struct CliEngineHost {
    data_dir: PathBuf,
    /// EXP-447: released exactly when the run ends, on every path.
    refresher_hold: Mutex<Option<coding::RefresherHold>>,
}

impl engine::EngineHost for CliEngineHost {
    fn on_exit(&self, exit: engine::EngineExit) {
        if let Some(error) = &exit.error {
            log::warn!("coding session {}: {error}", exit.session_id);
        }
        // EXP-641/EXP-746: a RESOLVED end drops the registry entry, anything
        // else keeps it for the next start's reconcile — decided ONCE, by the
        // process-wide observer `coding::end_session` fires
        // (`registry::install_end_observer`), so the PTY supervisor and the
        // engine share one implementation. The only case that never reaches
        // it is a run that died before the end sequence ran at all: its entry
        // still carries THIS process's live pid, which no later reconcile
        // would touch, so clear it here.
        if exit.end.is_none() {
            registry::mark_ended(&self.data_dir, &exit.session_id);
        }
        if let Ok(mut hold) = self.refresher_hold.lock() {
            drop(hold.take());
        }
    }
}

/// The own-row kill-switch poll, shared by both transports (EXP-746).
///
/// Fires ONLY on an explicit own-row `ended` (vanished row ≠ kill; a
/// resurrected row owned by someone else must never kill this run).
/// EXP-674: this edge is the daemon's ONLY reaper besides the child's own
/// exit — deliberately no idle bound. A person-started run never reports at
/// all: since EXP-679 `exponential_sessions_end` is registered only for
/// UNATTENDED runs, so this one just keeps waiting for the next reply,
/// exactly like in a desktop tab or an attached terminal, until a web/mobile
/// "Kill session", a merge or the sweep ends the row. Unattended runs
/// (schedule/event/agent-started) do get the tool, and their close-out ends
/// the row and reaps right away.
/// EXP-681: the ONE exception to "vanished row ≠ kill" — a SUSTAINED 426
/// min-version gate. This build can no longer read the edge (the poll is
/// rejected) nor keep the row alive (so is the heartbeat), so once the server
/// sweep has provably deleted it ([`GATED_KILL_AFTER`]) nothing remote can
/// end the run any more and the watcher ends it itself.
///
/// `sink` receives the decided edge (`graceful` = EXP-637's agent-declared
/// end) and is the ONLY thing that differs per transport: the PTY arm waits
/// out the close-out inline and posts `Control::Kill`, the ACP arm hands the
/// reason to the engine, which does its own wait. Returns the watcher's stop
/// flag — set it BEFORE the run's own end flip becomes visible (EXP-283) or
/// the poll reads that end as a remote kill.
fn spawn_kill_watch(env: &LaunchEnv, session_id: &str, sink: KillSink) -> Arc<AtomicBool> {
    let watch_done = Arc::new(AtomicBool::new(false));
    let trpc = Arc::clone(&env.ctx.trpc);
    let session_id = session_id.to_string();
    let own_user = env.ctx.account.user_id.clone();
    let stop = Arc::clone(&watch_done);
    std::thread::spawn(move || {
        // EXP-681: when the CURRENT run of consecutive 426 polls began.
        let mut first_gated_at: Option<Instant> = None;
        while !stop.load(Ordering::SeqCst) {
            std::thread::sleep(KILL_POLL_INTERVAL);
            if stop.load(Ordering::SeqCst) {
                return;
            }
            let result = api::coding_sessions::get(&trpc, &session_id);
            if !matches!(result, Err(api::ApiError::UpgradeRequired)) {
                // Any answer but the gate (a row, a vanished row, a transport
                // blip) means the server talks to this build again — a lifted
                // gate resets the clock.
                first_gated_at = None;
            }
            match result {
                Ok(Some(row)) => match kill_poll_decision(&row, &own_user) {
                    PollDecision::Kill { graceful } => {
                        sink(graceful);
                        return;
                    }
                    PollDecision::StopWatching => return,
                    PollDecision::Continue => {}
                },
                // EXP-681: the min-version gate — see [`GATED_KILL_AFTER`].
                Err(api::ApiError::UpgradeRequired) => {
                    let now = Instant::now();
                    if first_gated_at.is_none() {
                        log::warn!(
                            "coding session {session_id}: HTTP 426 on the kill poll — the server no longer accepts this build; the run ends itself in {}s unless the gate lifts",
                            GATED_KILL_AFTER.as_secs()
                        );
                    }
                    if gated_poll_kills(&mut first_gated_at, now) {
                        log::warn!(
                            "coding session {session_id}: gated for {}s — its row is swept and no remote kill can reach it, ending the run",
                            GATED_KILL_AFTER.as_secs()
                        );
                        sink(false);
                        return;
                    }
                }
                // Swept/foreign row or a transport blip: never a kill.
                Ok(None) | Err(_) => {}
            }
        }
    });
    watch_done
}

/// What [`spawn_kill_watch`] does with a decided kill: `true` = EXP-637's
/// agent-declared end (the close-out is still being written), `false` = now.
type KillSink = Box<dyn Fn(bool) + Send>;

/// One kill-poll observation → what the watcher thread does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PollDecision {
    /// An owned row flipped to `ended` — the durable kill signal.
    /// EXP-637: `graceful` when the AGENT ended its own run
    /// (`exponential_sessions_end`) — the child is still mid-turn writing
    /// the close-out that call was about, so the kill waits for the turn to
    /// finish (bounded by [`STOP_GRACE`]). Every other `ended_by` (a user
    /// kill, a merge, the sweep) means "now", exactly as before.
    Kill { graceful: bool },
    /// A live owned row — keep polling.
    Continue,
    /// The row was resurrected under a stranger — its edges are not ours.
    StopWatching,
}

/// How long an agent-declared end waits for the turn to finish before the
/// kill lands anyway. Mirrors the desktop's `graceful_stop::STOP_GRACE`.
const STOP_GRACE: Duration = Duration::from_secs(60);

/// EXP-681: how long the kill poll tolerates the 426 min-version gate before
/// it ends the run ITSELF. While the gate holds the server rejects every
/// call from this build — the session heartbeat included — so the row's
/// `updated_at` froze at the first gated poll and the server sweep
/// (`coding-session-sweep.ts`: `CODING_SESSION_STALE_MS` from `updated_at`,
/// checked every 30 minutes) has DELETED it by the end of this window. From
/// then on a web/mobile "Kill session" has nothing to kill and the poll can
/// never see an `ended` edge: the run is unreachable from every product
/// surface, and the only useful thing the gated host can still do is stop
/// hosting it — which also un-parks the daemon's own update (daemon.rs
/// waits for idle before it swaps the binary), the day
/// `CLIENT_MIN_VERSION_CLI` is raised past a build holding a forgotten
/// person-started run. Deliberately the server's window plus its sweep
/// cadence, not a shorter guess: a gate that lifts sooner (a rolled-back
/// deploy) simply resumes the normal poll.
const GATED_KILL_AFTER: Duration = Duration::from_millis(
    domain::contract::CODING_SESSION_STALE_MS as u64 + 30 * 60 * 1000,
);

/// EXP-681: one gated (426) poll → whether the watcher gives up on the run.
/// `first_gated_at` is the watcher's memory of the first CONSECUTIVE 426
/// (the caller clears it on any other result); pure so the bound is
/// unit-testable.
fn gated_poll_kills(first_gated_at: &mut Option<Instant>, now: Instant) -> bool {
    let since = *first_gated_at.get_or_insert(now);
    now.saturating_duration_since(since) >= GATED_KILL_AFTER
}

/// The kill-poll's ownership rule. Owner-or-host (EXP-445): a shared-device
/// row carries the REQUESTER as `user_id` while this daemon is only its
/// host — `host_user_id == own` must also count, or the durable →ended kill
/// (unshare, requester account deletion, a relay-less `killSession`) never
/// reaches exactly the sessions a foreign teammate started here. A missing
/// `user_id` degrades to owned (the server always stamps it; partial rows
/// only).
fn kill_poll_decision(row: &api::coding_sessions::CodingSession, own_user: &str) -> PollDecision {
    let owned = row
        .user_id
        .as_deref()
        .map(|id| id == own_user)
        .unwrap_or(true)
        || row.host_user_id.as_deref() == Some(own_user);
    if !owned {
        return PollDecision::StopWatching;
    }
    if row.status.as_deref() == Some("ended") {
        PollDecision::Kill {
            graceful: row.ended_by.as_deref()
                == Some(domain::contract::CODING_SESSION_ENDED_BY_AGENT),
        }
    } else {
        PollDecision::Continue
    }
}

struct SupervisorCtx {
    trpc: Arc<api::trpc::TrpcClient>,
    // EXP-746: no `data_dir` here any more — the registry decision moved to
    // the process-wide session-end observer.
    session_id: String,
    heartbeat_stop: coding::HeartbeatStop,
    publisher: Option<steer::PublisherHandle>,
    activity_active: Arc<AtomicBool>,
    watch_done: Arc<AtomicBool>,
    interactive: bool,
    /// EXP-447: keeps the clone's ambient git credentials fresh for the
    /// session's life — drops (releasing the ref-counted refresher) exactly
    /// when `supervise` returns, on both the exit and kill paths. `None` for
    /// repo-less action runs.
    _refresher_hold: Option<coding::RefresherHold>,
}

fn supervise(
    ctx: SupervisorCtx,
    mut emulator: Emulator,
    pty: pty::Pty,
    wake_rx: flume::Receiver<Wake>,
    control_rx: flume::Receiver<Control>,
    exit_slot: pty::ExitSlot,
    done_tx: flume::Sender<ChildExit>,
) {
    let mut killed_outcome: Option<&'static str> = None;
    let exit = loop {
        enum Event {
            Wake,
            Control(Control),
            WakeClosed,
        }
        let event = flume::Selector::new()
            .recv(&wake_rx, |wake| match wake {
                Ok(_) => Event::Wake,
                Err(_) => Event::WakeClosed,
            })
            .recv(&control_rx, |control| match control {
                Ok(control) => Event::Control(control),
                // All senders gone — treat like a quiet tick.
                Err(_) => Event::Wake,
            })
            .wait();
        match event {
            Event::Wake | Event::WakeClosed => {
                // Coalesce bursts, then pump. Replies (DA/DSR/…) go back to
                // the child in detached mode; interactively the REAL
                // terminal sees the query via the tee and answers itself.
                while wake_rx.try_recv().is_ok() {}
                pump(&mut emulator, &pty, ctx.interactive);
                if let Some(exit) = exit_slot.lock().ok().and_then(|slot| slot.clone()) {
                    break exit;
                }
                if matches!(event, Event::WakeClosed) {
                    // Read loop gone and no captured exit yet — give the
                    // wait thread a beat, then synthesize.
                    std::thread::sleep(Duration::from_millis(200));
                    break exit_slot
                        .lock()
                        .ok()
                        .and_then(|slot| slot.clone())
                        .unwrap_or(ChildExit { code: -1, success: false, signal: None });
                }
            }
            Event::Control(Control::Kill { outcome }) => {
                killed_outcome = Some(outcome);
                pty.kill();
                // The wait thread captures the exit; keep pumping until it
                // lands (loop continues).
            }
            Event::Control(Control::Resize(cols, rows)) => {
                if cols > 0 && rows > 0 {
                    let _ = pty.resize(cols, rows, emulator.cell_px());
                    emulator.resize(cols, rows);
                }
            }
        }
    };
    // Final sweep — a fast exit's last output must not be missed.
    pump(&mut emulator, &pty, ctx.interactive);

    // Teardown ordering (EXP-283): stop our own kill-watch BEFORE the end
    // flip becomes visible, or we read our own end as a remote kill.
    ctx.watch_done.store(true, Ordering::SeqCst);
    drop(ctx.heartbeat_stop);
    if let Some(publisher) = &ctx.publisher {
        let outcome = killed_outcome
            .map(str::to_string)
            .unwrap_or_else(|| format!("exit:{}", exit.code));
        publisher.shutdown(Some(outcome));
    }
    ctx.activity_active.store(false, Ordering::SeqCst);
    // EXP-641: the registry entry goes only with a RESOLVED end. EXP-746
    // moved that decision behind `coding::end_session`'s process-wide
    // observer (`registry::install_end_observer`, installed at every entry
    // point), so the PTY path here and the ACP engine share ONE
    // implementation of it.
    let _ = coding::end_session(&ctx.trpc, &ctx.session_id);
    let _ = done_tx.send(exit);
}

fn pump(emulator: &mut Emulator, pty: &pty::Pty, interactive: bool) {
    if interactive {
        let _ = emulator.drain_events(&mut |_reply| {});
    } else {
        let _ = emulator.drain_events(&mut |reply| pty.writer_write(reply));
    }
}

/// Mirror child output verbatim to the local terminal (interactive attach)
/// while the emulator keeps consuming the same bytes for the steer
/// machinery. The read loop stays the single reader — this wraps it.
struct TeeReader {
    inner: Box<dyn std::io::Read + Send>,
}

impl std::io::Read for TeeReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            let mut stdout = std::io::stdout().lock();
            let _ = stdout.write_all(&buf[..n]);
            let _ = stdout.flush();
        }
        Ok(n)
    }
}

fn join_bounded(handle: std::thread::JoinHandle<()>) {
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while !handle.is_finished() {
        if std::time::Instant::now() >= deadline {
            return; // detach — an orphaned grandchild can hold the slave open
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = handle.join();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(user_id: Option<&str>, host_user_id: Option<&str>, status: &str) -> api::coding_sessions::CodingSession {
        serde_json::from_value(serde_json::json!({
            "id": "sess-1",
            "userId": user_id,
            "hostUserId": host_user_id,
            "status": status,
        }))
        .expect("fixture decodes")
    }

    /// EXP-637: an ended row that also says WHO ended it.
    fn ended_by(ended_by: &str) -> api::coding_sessions::CodingSession {
        serde_json::from_value(serde_json::json!({
            "id": "sess-1",
            "userId": "me",
            "status": "ended",
            "endedBy": ended_by,
        }))
        .expect("fixture decodes")
    }

    #[test]
    fn own_rows_kill_on_ended_and_continue_while_live() {
        assert_eq!(
            kill_poll_decision(&row(Some("me"), None, "ended"), "me"),
            // No `ended_by` (an old server) is not the agent's own close-out.
            PollDecision::Kill { graceful: false }
        );
        assert_eq!(
            kill_poll_decision(&row(Some("me"), None, "running"), "me"),
            PollDecision::Continue
        );
    }

    /// EXP-445 regression: a shared-device row (requester as user_id, this
    /// daemon as host) must keep the watch alive — and its `ended` flip IS
    /// this daemon's kill. Before the fix the watcher stopped on the first
    /// poll, so unshare/deletion kills never landed.
    #[test]
    fn hosted_foreign_rows_stay_watched_and_their_ended_flip_kills() {
        assert_eq!(
            kill_poll_decision(&row(Some("requester"), Some("me"), "running"), "me"),
            PollDecision::Continue
        );
        assert_eq!(
            kill_poll_decision(&row(Some("requester"), Some("me"), "ended"), "me"),
            PollDecision::Kill { graceful: false }
        );
    }

    /// The resurrection rule survives: a row owned by a stranger (no host
    /// claim either) is not ours, whatever its status says.
    #[test]
    fn fully_foreign_rows_stop_the_watch_without_killing() {
        assert_eq!(
            kill_poll_decision(&row(Some("stranger"), None, "ended"), "me"),
            PollDecision::StopWatching
        );
        assert_eq!(
            kill_poll_decision(&row(Some("stranger"), Some("other-host"), "running"), "me"),
            PollDecision::StopWatching
        );
    }

    /// A partial row without user_id degrades to owned (server always stamps
    /// it) — an ended flip still kills.
    #[test]
    fn missing_user_id_degrades_to_owned() {
        assert_eq!(
            kill_poll_decision(&row(None, None, "ended"), "me"),
            PollDecision::Kill { graceful: false }
        );
    }

    /// EXP-637: only the AGENT's own close-out earns the graceful wait —
    /// killing it mid-turn would truncate exactly the summary the
    /// `exponential_sessions_end` call was about. Every other end means now.
    #[test]
    fn only_an_agent_declared_end_kills_gracefully() {
        assert_eq!(
            kill_poll_decision(&ended_by("agent"), "me"),
            PollDecision::Kill { graceful: true }
        );
        for ended_by_value in ["user", "client", "merge", "system"] {
            assert_eq!(
                kill_poll_decision(&ended_by(ended_by_value), "me"),
                PollDecision::Kill { graceful: false },
                "{ended_by_value} must kill immediately"
            );
        }
        // Every contract value is covered above.
        assert_eq!(
            domain::contract::CODING_SESSION_ENDED_BY_VALUES.len(),
            5,
            "a new endedBy value needs a decision here"
        );
    }

    /// EXP-681: a sustained 426 ends the run only once the server's sweep
    /// window (plus its cadence) has passed since the FIRST gated poll —
    /// never on the first one — and the bound never undercuts the window,
    /// or the watcher would kill a run whose row a fixed deploy could still
    /// have ended remotely.
    #[test]
    fn a_sustained_gate_kills_only_past_the_sweep_window() {
        let now = Instant::now();
        let mut first_gated_at = None;
        assert!(!gated_poll_kills(&mut first_gated_at, now));
        assert_eq!(first_gated_at, Some(now));
        assert!(!gated_poll_kills(
            &mut first_gated_at,
            now + GATED_KILL_AFTER - Duration::from_secs(1)
        ));
        assert!(gated_poll_kills(&mut first_gated_at, now + GATED_KILL_AFTER));
        // A lifted gate (the caller cleared the memory) starts over.
        let mut cleared = None;
        assert!(!gated_poll_kills(&mut cleared, now + GATED_KILL_AFTER * 2));
        assert!(
            GATED_KILL_AFTER
                >= Duration::from_millis(domain::contract::CODING_SESSION_STALE_MS as u64)
        );
    }

    /// EXP-746: the two backends differ in exactly the PTY-shaped
    /// capabilities. `daemon.rs`'s bookkeeping, its reap block and the quit
    /// sweep only use `is_done`/`kill`/`wait*`, which both answer — so the
    /// only thing a caller may branch on is this, and an ACP run must never
    /// be handed raw bytes or a geometry.
    #[test]
    fn running_session_backend_dispatch() {
        let pty = BackendKind::from(coding::LaunchTransport::Terminal);
        let acp = BackendKind::from(coding::LaunchTransport::Acp);
        assert_eq!(pty, BackendKind::Pty);
        assert_eq!(acp, BackendKind::Acp);

        assert!(pty.supports_stdin());
        assert!(pty.supports_resize());
        assert!(!pty.steers_by_message());

        assert!(!acp.supports_stdin());
        assert!(!acp.supports_resize());
        assert!(acp.steers_by_message());
    }

    /// EXP-746: `code`/`run` print an exit code and the daemon logs one, so an
    /// engine exit has to read as a child exit. A real child's exit wins; a
    /// handshake failure that never produced one is a failure; a kill or the
    /// agent's own close-out is a clean zero.
    #[test]
    fn an_engine_exit_reads_as_a_child_exit() {
        let exit = |child, error: Option<&str>| engine::EngineExit {
            session_id: "sess-1".to_string(),
            outcome: "ended".to_string(),
            child,
            error: error.map(str::to_string),
            end: None,
        };

        let real = ChildExit { code: 3, success: false, signal: None };
        assert_eq!(engine_child_exit(&exit(Some(real), None)).code, 3);

        let failed = engine_child_exit(&exit(None, Some("the agent did not connect")));
        assert!(!failed.success);
        assert_eq!(failed.code, -1);

        let clean = engine_child_exit(&exit(None, None));
        assert!(clean.success);
        assert_eq!(clean.code, 0);
    }

    /// EXP-758: the engine's failure text survives the trip into the
    /// `ChildExit` vocabulary: it is what `code`/`run` print on the exit
    /// line, and "exit -1" alone never said what went wrong.
    #[test]
    fn an_engine_exit_keeps_its_error_for_the_exit_line() {
        let failed = session_exit(&engine::EngineExit {
            session_id: "sess-1".to_string(),
            outcome: "ended".to_string(),
            child: None,
            error: Some("the agent did not connect".to_string()),
            end: None,
        });
        assert_eq!(failed.error.as_deref(), Some("the agent did not connect"));
        assert!(!failed.child.success);
    }

    /// EXP-758: `prepare`'s step 6 created the `running` row, so a launch
    /// that never reaches the engine must end it BEFORE it returns: the
    /// runtime-less `launch_acp` return used to be the one path that didn't,
    /// leaving a badge that ghosted until the server sweep.
    #[test]
    fn a_launch_failure_ends_the_row_before_it_returns() {
        let ended = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&ended);
        let err = fail_before_start(anyhow::anyhow!("no steer runtime"), move || {
            flag.store(true, Ordering::SeqCst)
        });
        assert!(ended.load(Ordering::SeqCst), "the row must be ended");
        assert_eq!(err.to_string(), "no steer runtime");
    }

    /// EXP-758 (EXP-478): the gate rides the SESSION, not `launch`: the
    /// daemon pushes the run into its live list first and releases only then
    /// (`spawn_prepared`). While the hold is live the clone's prune pass
    /// refuses to run, which is exactly what protects a just-born worktree
    /// with no unique commits from a `worktree_prune` command.
    #[test]
    fn a_session_holds_the_launch_gate_until_its_owner_releases_it() {
        let clone = std::env::temp_dir().join(format!(
            "exp-cli-launch-gate-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&clone).expect("temp clone");
        let session = test_session("sess-1", Some(coding::launch_gate::hold(&clone)));
        assert!(session.holds_launch_gate());
        assert!(
            coding::launch_gate::try_exclusive(&clone, || ()).is_none(),
            "a registered-pending run must park the prune"
        );

        session.release_launch_hold();
        assert!(!session.holds_launch_gate());
        assert!(
            coding::launch_gate::try_exclusive(&clone, || ()).is_some(),
            "the prune runs again once the run is registered"
        );
        // Idempotent: a second release (or the drop) never underflows.
        session.release_launch_hold();
        let _ = std::fs::remove_dir_all(&clone);
    }
}
