//! The headless session host — the CLI's half of a coding run.
//!
//! EXP-773 left ONE transport: [`launch`] hands the run to the in-process
//! ACP engine and keeps only the two things this host owns — the
//! crash-recovery registry entry and the tRPC kill poll's DECISION
//! ([`kill_poll_decision`], EXP-681's [`GATED_KILL_AFTER`]); the engine
//! consumes decided edges through an [`engine::KillFeed`].
//!
//! What the engine owns (D14) and this file therefore does NOT: the
//! publisher (attach, `bye`, shutdown), the heartbeat, the activity
//! vocabulary and `coding::end_session`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use coding::{CodingAgent, PreparedLaunch};
use steer::SteerRuntime;
use terminal::pty::ChildExit;

use crate::context::Ctx;
use crate::registry;

/// Kill-switch poll cadence — the desktop reads the →ended edge off its
/// Electric sync in real time; the daemon polls. Snappy enough for a web
/// "Kill" click, trivial load for the server.
const KILL_POLL_INTERVAL: Duration = Duration::from_secs(15);

pub struct LaunchEnv<'a> {
    pub ctx: &'a Ctx,
    /// `None` = no steer runtime, i.e. no engine: a launch is refused.
    pub runtime: Option<&'a Arc<SteerRuntime>>,
    /// The `expu_` personal key — the activity redactor's exact-match
    /// secret (codex carries it env-only; REV2-17).
    pub personal_key: Option<String>,
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
    launch_hold: LaunchGate,
    session: engine::EngineSession,
}

/// The clone's launch gate a live run holds, split out so the release
/// ORDER is unit-testable without an engine session (constructing one spawns
/// an agent).
#[derive(Default)]
pub(crate) struct LaunchGate(Mutex<Option<coding::LaunchHold>>);

impl LaunchGate {
    pub(crate) fn new(hold: Option<coding::LaunchHold>) -> Self {
        Self(Mutex::new(hold))
    }

    /// Drop the hold. Idempotent.
    pub(crate) fn release(&self) {
        if let Ok(mut hold) = self.0.lock() {
            drop(hold.take());
        }
    }

    /// Whether the prune is still parked on the clone (test seam).
    #[cfg(test)]
    pub(crate) fn held(&self) -> bool {
        self.0.lock().map(|hold| hold.is_some()).unwrap_or(false)
    }
}

impl RunningSession {
    /// EXP-758: the run is registered with its owner: the auto-prune can
    /// see it now, so the launch gate is free. Idempotent.
    pub fn release_launch_hold(&self) {
        self.launch_hold.release();
    }
}

impl RunningSession {
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
    /// The attaches wait through here and print it.
    pub fn wait_detailed(&self) -> SessionExit {
        session_exit(&self.session.wait())
    }

    /// `Some(exit)` once the child has exited, `None` on timeout.
    pub fn wait_timeout(&self, timeout: Duration) -> Option<ChildExit> {
        self.wait_timeout_detailed(timeout).map(|exit| exit.child)
    }

    /// [`RunningSession::wait_detailed`] with a bound (EXP-758).
    pub fn wait_timeout_detailed(&self, timeout: Duration) -> Option<SessionExit> {
        self.session
            .wait_timeout(timeout)
            .map(|exit| session_exit(&exit))
    }

    pub fn is_done(&self) -> bool {
        self.session.is_done()
    }

    /// FEED-36: whether the agent is between turns (nothing in flight, no
    /// tool running) — the engine's own `TurnSignal`. A parked question
    /// counts as idle too: the agent is waiting, not working.
    pub fn is_idle(&self) -> bool {
        self.session.turn_signal().is_idle()
    }

    pub fn kill(&self) {
        self.session.kill("ended");
    }

    /// EXP-746: one whole message from the local attach — a fresh prompt
    /// between turns, steering mid-turn.
    pub fn send_prompt(&self, text: String) {
        self.session.send_prompt(text);
    }

    /// EXP-746: answer a pending question card.
    pub fn answer(&self, answer: steer::RemoteAnswer) {
        self.session.answer(answer);
    }

    /// EXP-746: a `/` command from the local attach — the same sink a remote
    /// one takes.
    pub fn run_command(&self, name: &str, args: &str) {
        self.session.run_command(name, args);
    }

    /// EXP-746: the engine's local feed, for `exponential code`'s line
    /// printer.
    pub fn feed(&self) -> flume::Receiver<engine::LocalFeedEvent> {
        self.session.subscribe()
    }
}

/// EXP-758: how a run ended, for the callers that RENDER it: the child exit
/// every caller already speaks, plus the engine's failure text when the run
/// never got a child to fail (a handshake or transport error).
pub struct SessionExit {
    pub child: ChildExit,
    /// [`engine::EngineExit::error`].
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

/// Everything after `coding::prepare` said `Ready`. Spawn, publisher,
/// heartbeat and end belong to the engine (D14); this function owns exactly
/// the two facts that are the CLI's and not the engine's: the
/// crash-recovery registry entry and the tRPC kill poll's DECISION.
///
/// The `issue_id` is the steer room's — `Some` only for real issue sessions
/// (batch/action rooms send none; desktop parity).
pub fn launch(
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
        launch_hold: LaunchGate::new(launch_hold),
        session,
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
/// describes claude/codex behaviour we verified, so an external agent's
/// menu carries only what it advertised itself.
fn acp_session_agent(prepared: &PreparedLaunch) -> steer::SessionAgent {
    if prepared.acp.options.external.is_some() {
        return steer::SessionAgent::External;
    }
    match prepared.agent {
        CodingAgent::Claude => steer::SessionAgent::Claude,
        CodingAgent::Codex => steer::SessionAgent::Codex,
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
        let gate = LaunchGate::new(Some(coding::launch_gate::hold(&clone)));
        assert!(gate.held());
        assert!(
            coding::launch_gate::try_exclusive(&clone, || ()).is_none(),
            "a registered-pending run must park the prune"
        );

        gate.release();
        assert!(!gate.held());
        assert!(
            coding::launch_gate::try_exclusive(&clone, || ()).is_some(),
            "the prune runs again once the run is registered"
        );
        // Idempotent: a second release (or the drop) never underflows.
        gate.release();
        let _ = std::fs::remove_dir_all(&clone);
    }
}
