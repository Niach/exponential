//! The headless session host — the CLI's replacement for the desktop's
//! `TerminalManager` tab + `spawn_prepared_with` + `attach_publisher`
//! wiring: a real PTY (`terminal` core; the agent runs its interactive TUI
//! exactly like in the desktop dock), the emulator pump loop, the session
//! heartbeat, the steer publisher + per-agent activity emitter, and the
//! own-row kill-switch (tRPC poll of `codingSessions.get` — no Electric
//! sync here).

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context as _;
use coding::{CodingAgent, PreparedLaunch, SESSION_HEARTBEAT_INTERVAL};
use steer::publisher::pty_writer_input_hook;
use steer::{
    AnswerLink, EmitterConfig, PublishSpec, PublisherHooks, PublisherTickets, SteerRuntime,
    Steering, TrpcPublisherTickets,
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

pub struct RunningSession {
    pub session_id: String,
    pub issue_identifier: String,
    pub worktree: PathBuf,
    pub branch: String,
    done_rx: flume::Receiver<ChildExit>,
    control_tx: flume::Sender<Control>,
    writer: Arc<Mutex<Box<dyn std::io::Write + Send>>>,
}

impl RunningSession {
    /// Block until the agent child exits.
    pub fn wait(&self) -> ChildExit {
        self.done_rx
            .recv()
            .unwrap_or(ChildExit { code: -1, success: false, signal: None })
    }

    /// `Some(exit)` once the child has exited, `None` on timeout.
    pub fn wait_timeout(&self, timeout: Duration) -> Option<ChildExit> {
        self.done_rx.recv_timeout(timeout).ok()
    }

    pub fn is_done(&self) -> bool {
        // The supervisor sends the exit into the bounded(1) channel and then
        // drops its sender — flume keeps the buffered message after
        // disconnect, so "done" is EITHER an exit waiting to be read OR a
        // dead channel (supervisor gone). `disconnected && empty` alone
        // never fires for a finished-but-unwaited daemon session, which
        // jammed the reaper and every dedup gate (review finding, EXP-403).
        !self.done_rx.is_empty() || self.done_rx.is_disconnected()
    }

    pub fn kill(&self) {
        let _ = self.control_tx.send(Control::Kill { outcome: "ended" });
    }

    /// Local stdin bytes (interactive attach) — the same shared PTY writer
    /// remote steer input uses; the child cannot tell them apart.
    pub fn write_stdin(&self, bytes: &[u8]) {
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
        }
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let _ = self.control_tx.send(Control::Resize(cols, rows));
    }
}

/// Everything after `coding::prepare_with_hooks` said `Ready`: spawn the
/// agent on a fresh PTY and wire heartbeat/publisher/emitter/kill-watch.
/// `interactive` mirrors the raw PTY bytes to stdout and leaves terminal
/// query replies (DA/DSR) to the REAL terminal; detached lets the emulator
/// answer them (the child's queries must never hang).
pub fn launch(
    env: &LaunchEnv,
    prepared: PreparedLaunch,
    interactive: bool,
    // The steer room's issue id — `Some` only for real issue sessions
    // (batch/action rooms send none; desktop parity).
    issue_id: Option<String>,
) -> anyhow::Result<RunningSession> {
    let PreparedLaunch {
        session_id,
        issue_identifier,
        worktree,
        clone,
        repository_id,
        branch,
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

    // --- Liveness heartbeat (launcher.rs parity): the stale sweep deletes
    // rows whose updated_at stops advancing; the scope re-creates a swept
    // row under the same id. Stop sender disconnects on exit. ---------------
    let (heartbeat_stop, heartbeat_stopped) = std::sync::mpsc::channel::<()>();
    {
        let trpc = Arc::clone(&env.ctx.trpc);
        let session_id = session_id.clone();
        std::thread::spawn(move || loop {
            match heartbeat_stopped.recv_timeout(SESSION_HEARTBEAT_INTERVAL) {
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    let _ = api::coding_sessions::heartbeat(&trpc, &session_id, Some(&heartbeat_scope));
                }
                _ => return,
            }
        });
    }

    let (control_tx, control_rx) = flume::unbounded::<Control>();
    let (done_tx, done_rx) = flume::bounded::<ChildExit>(1);
    let writer = pty.writer();

    // --- Steer publisher + activity emitter (attach_publisher parity) ------
    let mut publisher: Option<steer::PublisherHandle> = None;
    let activity_active = Arc::new(AtomicBool::new(true));
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
        let session_agent = match agent {
            CodingAgent::Claude => steer::activity::SessionAgent::Claude,
            CodingAgent::Codex => steer::activity::SessionAgent::Codex,
            CodingAgent::Pi => steer::activity::SessionAgent::Pi,
        };
        let pi_subscription = (agent == CodingAgent::Pi)
            .then(|| env.sidecars.subscribe_pi(&worktree))
            .flatten();
        let (pi_events, pi_steer) = match pi_subscription {
            Some((events, steer_handle)) => (Some(events), Some(steer_handle)),
            None => (None, None),
        };
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
                }),
                pi_events,
                bypass_permissions,
                plan_mode,
                claude_session_id: claude_session_id.clone(),
                codex_originator: codex_originator.clone(),
                codex_resume_id: codex_resume_id.clone(),
                foreign_host,
            },
            handle.activity_sender(),
            Arc::clone(&activity_active),
        );
        publisher = Some(handle);
    }

    // --- Kill-switch poll: the desktop's own-row kill-watch, over tRPC.
    // Fires ONLY on an explicit own-row `ended` (vanished row ≠ kill; a
    // resurrected row owned by someone else must never kill this run). -----
    let watch_done = Arc::new(AtomicBool::new(false));
    {
        let trpc = Arc::clone(&env.ctx.trpc);
        let session_id = session_id.clone();
        let own_user = env.ctx.account.user_id.clone();
        let kill_tx = control_tx.clone();
        let watch_done = Arc::clone(&watch_done);
        std::thread::spawn(move || {
            while !watch_done.load(Ordering::SeqCst) {
                std::thread::sleep(KILL_POLL_INTERVAL);
                if watch_done.load(Ordering::SeqCst) {
                    return;
                }
                match api::coding_sessions::get(&trpc, &session_id) {
                    Ok(Some(row)) => match kill_poll_decision(&row, &own_user) {
                        PollDecision::Kill => {
                            let _ = kill_tx.send(Control::Kill { outcome: "killed" });
                            return;
                        }
                        PollDecision::StopWatching => return,
                        PollDecision::Continue => {}
                    },
                    // Swept/foreign row or a transport blip: never a kill.
                    Ok(None) | Err(_) => {}
                }
            }
        });
    }

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
        data_dir: env.ctx.data_dir.clone(),
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
        done_rx,
        control_tx,
        writer,
    })
}

/// One kill-poll observation → what the watcher thread does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PollDecision {
    /// An owned row flipped to `ended` — the durable kill signal.
    Kill,
    /// A live owned row — keep polling.
    Continue,
    /// The row was resurrected under a stranger — its edges are not ours.
    StopWatching,
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
        PollDecision::Kill
    } else {
        PollDecision::Continue
    }
}

struct SupervisorCtx {
    trpc: Arc<api::trpc::TrpcClient>,
    data_dir: PathBuf,
    session_id: String,
    heartbeat_stop: std::sync::mpsc::Sender<()>,
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
                    let _ = pty.resize(cols, rows);
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
    // EXP-641: the registry entry goes only with a RESOLVED end. An end the
    // server rejects (the 426 min-version gate mid-deploy, a dead network)
    // keeps it for the restarted daemon's reconcile instead of stranding the
    // row `running` for the server sweep's 2h window.
    let result = api::coding_sessions::end(&ctx.trpc, &ctx.session_id);
    if registry::end_outcome_resolves(&result) {
        registry::remove(&ctx.data_dir, &ctx.session_id);
    } else if let Err(err) = &result {
        log::info!(
            "end of coding session {} did not resolve ({err}) — kept for the next start's reconcile",
            ctx.session_id
        );
    }
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

    #[test]
    fn own_rows_kill_on_ended_and_continue_while_live() {
        assert_eq!(kill_poll_decision(&row(Some("me"), None, "ended"), "me"), PollDecision::Kill);
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
            PollDecision::Kill
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
        assert_eq!(kill_poll_decision(&row(None, None, "ended"), "me"), PollDecision::Kill);
    }
}
