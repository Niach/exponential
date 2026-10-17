//! The headless session host — the CLI's half of a coding run.
//!
//! EXP-773 left ONE transport: [`launch`] hands the run to the in-process
//! ACP engine and keeps only what this host owns: the crash-recovery
//! registry entry and the clone's token refresher. EXP-1292: the kill poll
//! is gone — the pickup's `Kill` action (a row gone `ended` off the
//! `sessions` shape) stops the engine, so the engine gets an inert feed.
//!
//! What the engine owns (D14) and this file therefore does NOT: the
//! publisher (attach, `bye`, shutdown), the row's status reports, the
//! activity vocabulary and `coding::end_session`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use coding::{CodingAgent, PreparedLaunch};
use steer::SteerRuntime;
use terminal::pty::ChildExit;

use crate::context::Ctx;
use crate::registry;

pub struct LaunchEnv<'a> {
    pub ctx: &'a Ctx,
    /// `None` = no steer runtime, i.e. no engine: a launch is refused.
    pub runtime: Option<&'a Arc<SteerRuntime>>,
    /// The `expu_` personal key — the activity redactor's exact-match
    /// secret (codex carries it env-only; REV2-17).
    pub personal_key: Option<String>,
    /// EXP-1005: `true` only under the daemon, which runs the account-
    /// rotation beat; a foreground `exponential code`/`run` reports its
    /// walls unhandled so the owner is told.
    pub rotation_host: bool,
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
    /// is only protected once its OWNER has registered the run: a prune's
    /// `held` set is built from the live-session list, which `launch`
    /// returns BEFORE being pushed into. Dropping the hold inside `launch`
    /// left exactly that window open, and a prune landing in it sees a branch with no unique commits and no live session
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

    /// EXP-1292: stop the process but not the session — the row idles
    /// ([`engine::SUSPEND_OUTCOME`]) for an immediate re-spawn of the same
    /// row (the account switch).
    pub fn suspend(&self) {
        self.session.kill(engine::SUSPEND_OUTCOME);
    }

    /// EXP-1292: the run ended by [`Self::suspend`] (an account switch
    /// re-spawns the same row): its workspace — a scratch dir too — stays.
    ///
    /// EXP-1252: a RELEASED run (handed to another device) keeps it too — the
    /// handoff packs it, the source's cleanup removes it once the target
    /// has it ([`engine::keeps_workspace`]).
    pub fn was_suspended(&self) -> bool {
        self.session
            .wait_timeout(Duration::ZERO)
            .is_some_and(|exit| engine::keeps_workspace(&exit.outcome))
    }

    /// EXP-1292: the engine handle the pickup drives (`coding::pickup::
    /// LiveEngine` — deliver, unqueue, answer, kill). A cheap clone.
    pub fn engine(&self) -> engine::EngineSession {
        self.session.clone()
    }

    /// EXP-1005: the agent's usage wall as the engine last saw it (`None` =
    /// not blocked, or the wall's reset already passed) — what the daemon's
    /// rotation beat reads together with [`Self::is_idle`].
    pub fn blocked(&self) -> Option<steer::SessionBlocked> {
        self.session.blocked()
    }

    /// EXP-1005: the agent this run is on, in the rotation's vocabulary.
    pub fn coding_agent(&self) -> CodingAgent {
        self.session.agent()
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

/// Everything after `coding::prepare` said `Ready`. Spawn, publisher, the
/// row's reports and its end belong to the engine (D14); this function owns
/// exactly the facts that are the CLI's: the crash-recovery registry entry
/// and the clone's token refresher.
///
/// EXP-1292: the row is already CLAIMED (`coding::pickup` claims before it
/// prepares), so a launch that fails here leaves the row to its caller — the
/// pickup replies the reason into the conversation and settles the row.
/// `foreign_host` (EXP-444): the session's starter is not this account (a
/// teammate's start on a shared machine).
pub fn launch(
    env: &LaunchEnv,
    mut prepared: PreparedLaunch,
    foreign_host: bool,
) -> anyhow::Result<RunningSession> {
    let session_id = prepared.session_id.clone();
    // Unreachable by construction: `prepare` refuses with `AcpUnavailable`
    // unless `CodingDeps::acp_available` said this host has a runtime
    // (`launch::coding_deps`). A native resume of a recorded ACP run reaches
    // it anyway (EXP-758), and the pickup settles the row on this error.
    let Some(runtime) = env.runtime else {
        anyhow::bail!("the ACP engine needs the steer runtime");
    };

    let issue_identifier = prepared.issue_identifier.clone();
    let worktree = prepared.worktree.clone();
    let branch = prepared.branch.clone();
    let repository_id = prepared.repository_id.clone();
    let clone = prepared.clone.clone();
    let session_agent = acp_session_agent(&prepared);
    // EXP-1292: a team agent's ROOT never ends — its engine idles the row on
    // exit. `prepare` recorded the kind in the run record a moment ago.
    let persistent = coding::run_registry::get(&env.ctx.data_dir, &session_id)
        .is_some_and(|record| record.kind.is_persistent());
    // EXP-792: the team MCP servers this run starts WITHOUT. The daemon has
    // no toast, so the run's log names them once the engine is up (the
    // desktop shows the same lines as one notification).
    let mcp_warnings = std::mem::take(&mut prepared.mcp_warnings);

    registry::record(&env.ctx.data_dir, &session_id, &env.ctx.account.id);

    // EXP-447: the launch-time GitHub token dies after the hard 1h cap and
    // the credential helper is a deliberate dumb `cat`, so the clone's
    // ambient auth is kept fresh for the run's life. The hold moves into the
    // host and drops in `on_exit` — the ACP arm has no supervisor thread to
    // scope it to.
    let refresher_hold = repository_id.as_deref().map(|repository_id| {
        coding::clone_refreshers().retain(Arc::clone(&env.ctx.trpc), repository_id, &clone)
    });

    // EXP-478: the launch gate has to outlive registration, and
    // `EngineStart`'s destructure would drop it pre-spawn, so it comes out
    // here (`engine::start` asserts it did) and rides the returned session.
    // EXP-758: it is released by the OWNER, not here: a `drop` at the end of
    // this function is still pre-registration.
    let launch_hold = prepared.launch_hold.take();

    let host = Arc::new(CliEngineHost {
        data_dir: env.ctx.data_dir.clone(),
        refresher_hold: Mutex::new(refresher_hold),
        worktree: worktree.clone(),
        persistent,
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
            persistent,
            foreign_host,
            rotation_host: env.rotation_host,
            // A runtime-less host never gets here, so the room is always on.
            publish: true,
            // EXP-1292: no kill source of its own — the pickup's `Kill`
            // action stops the engine directly.
            kill: engine::KillFeed::inert(),
            // The line printer subscribes on demand (`RunningSession::feed`);
            // a detached daemon run wants no local fan-out at all.
            local_sink: None,
        },
        host,
    )
    .map_err(|err| {
        // Nothing ran: this process no longer owns the session.
        registry::mark_ended(&env.ctx.data_dir, &session_id);
        anyhow::anyhow!("{err}")
    })?;
    for warning in &mcp_warnings {
        log::warn!("coding session {session_id}: {warning}");
    }

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

/// EXP-746: which steer agent vocabulary an ACP run speaks.
fn acp_session_agent(prepared: &PreparedLaunch) -> steer::SessionAgent {
    match prepared.agent {
        CodingAgent::Claude => steer::SessionAgent::Claude,
        CodingAgent::Codex => steer::SessionAgent::Codex,
    }
}

/// EXP-746: the CLI's `EngineHost`. The engine owns the publisher, the
/// row's reports and `coding::end_session`; this is what is left over.
struct CliEngineHost {
    data_dir: PathBuf,
    /// EXP-447: released exactly when the run ends, on every path.
    refresher_hold: Mutex<Option<coding::RefresherHold>>,
    /// FEED-63: where a stopped run's uncommitted work is saved.
    worktree: PathBuf,
    /// EXP-1292: a team agent's root — its last write is an idle report,
    /// which never passes the end observer (nor does a suspended run's).
    persistent: bool,
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
        match &exit.end {
            None => registry::mark_ended(&self.data_dir, &exit.session_id),
            // EXP-1292: an agent root's (or a suspended run's) idle report
            // is its "end" here — the observer only sees
            // `coding::end_session`, so apply it now.
            // EXP-1252: a RELEASED run wrote nothing (the row is another
            // device's): its entry simply goes.
            Some(result) if self.persistent || engine::keeps_workspace(&exit.outcome) => {
                registry::apply_end_outcome(&self.data_dir, &exit.session_id, result)
            }
            // EXP-1292: a thread's END that never resolved is the one report
            // the next start's reconcile must re-send as an end — every other
            // host-less row is reconciled `idle` (resumable).
            Some(result) if !registry::end_outcome_resolves(result) => {
                coding::run_registry::update(&self.data_dir, &exit.session_id, |record| {
                    record.set_unreported_end(true);
                    true
                });
            }
            Some(_) => {}
        }
        // FEED-63: a run a PERSON stopped keeps its uncommitted work as a
        // local WIP commit on its branch (never pushed).
        engine::save_stopped_run_work(&exit, &self.worktree);
        if let Ok(mut hold) = self.refresher_hold.lock() {
            drop(hold.take());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// EXP-758 (EXP-478): the gate rides the SESSION, not `launch`: the
    /// daemon pushes the run into its live list first and releases only then
    /// (`spawn_prepared`). While the hold is live the clone's prune pass
    /// refuses to run, which is exactly what protects a just-born worktree
    /// with no unique commits from a prune.
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
