//! EXP-746 — starting a run and the handle both hosts hold onto.
//!
//! [`start`] is the whole entry point after `coding::prepare_with_hooks`
//! returned `Prepared::Ready` with `transport == LaunchTransport::Acp`. It is
//! NON-BLOCKING: it spawns the `acp-engine-<sid8>` thread and returns, so a
//! handshake failure never surfaces here — it arrives as an
//! [`EngineExit`](crate::EngineExit) with `error: Some(..)` through
//! [`EngineHost::on_exit`](crate::EngineHost::on_exit).
//!
//! EXP-1426 part 4d: the session itself is the agent runtime's
//! (`eagent::engine::session`); this builds Exponential's
//! start on it — the launcher's [`coding::PreparedLaunch`], the tRPC client,
//! the steer runtime and publisher, the run record — with the
//! [`RunLifecycle`] as the run's backend, and wraps the handle so its exit
//! carries the row's last write and the pickup can drive it.
//!
//! The exit contract deliberately mirrors `cli::session_host::RunningSession`
//! (`is_done`/`wait`/`wait_timeout`/`kill`) so the daemon's `LiveSession`
//! bookkeeping, reap block and quit sweep compile against either backend.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use agent_client_protocol::{Client, ConnectTo};
use eagent::engine::session as core;

use crate::adapters::{Adapter, AdapterKind, AdapterSpec};
use crate::host::{ChildExitLink, EngineExit, EngineHost, KillFeed, LocalSink, RunFacts};
use crate::lifecycle::{LifecycleSpec, RunLifecycle};
use crate::local::EnginePhase;
use crate::mapper::ExpHostTools;
use crate::sink::EventSink;

pub use core::{plan_mode_id, ConfigValue, SessionSpec};

/// Everything a run needs once `prepare` said Ready on the Acp arm.
pub struct EngineStart {
    /// `transport == Acp` and `acp.is_some()`. Its `launch_hold` must ALREADY
    /// be taken by the host (EXP-478: the gate has to outlive registration in
    /// `LocalSessions`/`LiveSession`, and a destructure in here would drop it
    /// pre-spawn) — [`start`] debug-asserts it.
    pub prepared: coding::PreparedLaunch,
    pub trpc: Arc<api::TrpcClient>,
    pub runtime: Arc<steer::SteerRuntime>,
    pub data_dir: PathBuf,
    pub account_id: String,
    /// The signed-in user's id — the kill-watch owner pin (EXP-105).
    pub own_user_id: Option<String>,
    /// The `expu_` personal key: the redactor's exact-match secret (REV2-17)
    /// AND the codex MCP bearer that rides the spawn env.
    pub personal_key: Option<String>,
    /// EXP-1292/1341: the session is PERSISTENT (`domain::session_kind`, a
    /// team agent's root). It never ends: the run's end idles the row
    /// (`sessions.report{status: idle}`) where a thread session's ends it.
    pub persistent: bool,
    /// EXP-444: a foreign requester on a shared host — suppresses login
    /// affordances.
    pub foreign_host: bool,
    /// EXP-1005: the host runs an account-rotation beat (the desktop, the
    /// daemon) — a wall it will rotate or wait out itself is reported
    /// `handled`, so the owner gets no rate-limit push. A foreground
    /// `exponential code`/`run` has no beat: `false`, and the owner hears.
    pub rotation_host: bool,
    /// `false` = no relay room at all (unit tests, `--no-publish`).
    pub publish: bool,
    /// Registered by the host BEFORE this call, so no edge can be missed
    /// between row creation and the first frame.
    pub kill: KillFeed,
    /// Desktop / `exponential code` attach; `None` on the daemon.
    pub local_sink: Option<LocalSink>,
}

/// The pieces [`start`] normally builds itself. Split out so a test (and an
/// adapter lane's own harness) can drive the REAL engine with a scripted
/// `ConnectTo<Client>` instead of a child process.
pub struct EngineParts<A> {
    pub adapter: A,
    /// Where the adapter records its child's exit — the `exit:<code>` the
    /// publisher says `bye` with.
    pub child_exit: ChildExitLink,
    /// Publish through this instead of a relay room. `None` = the room (or
    /// nothing at all when `publish` is false).
    pub sink: Option<Arc<dyn EventSink>>,
    /// EXP-1292: write the Conversation (replies, consumes) through this
    /// instead of the run's tRPC client — a test's recording fake.
    pub conversation: Option<Arc<dyn crate::outbox::ConversationApi>>,
}

/// Open an ENDED run's transcript read-only (a Past row on the desktop).
/// Writes nothing to the `sessions` row, runs no publisher and no kill feed:
/// it replays history through the same mapper and stops. Resuming is a
/// different action entirely (EXP-1292: a message to the session).
pub struct OpenTranscript {
    pub runtime: Arc<steer::SteerRuntime>,
    pub data_dir: PathBuf,
    pub personal_key: Option<String>,
    pub handle: HistoryHandle,
    pub local_sink: LocalSink,
}

/// Everything needed to find an ended run's history, straight off its
/// `runs.json` [`coding::run_registry::RunRecord`].
pub struct HistoryHandle {
    pub agent: coding::CodingAgent,
    pub cwd: PathBuf,
    /// The recorded ACP session id (`session/load`), when the run took the
    /// ACP path at all.
    pub acp_session_id: Option<String>,
    /// The agent-native handle — claude's transcript uuid, codex's thread id.
    pub native: ResumeHandle,
}

/// A cheap-clone handle on a live (or replaying) run. `Send + Sync`. The
/// agent runtime's handle (every method of it reads through), with the
/// exit typed as Exponential's.
#[derive(Clone)]
pub struct EngineSession(core::EngineSession);

impl std::ops::Deref for EngineSession {
    type Target = core::EngineSession;

    fn deref(&self) -> &core::EngineSession {
        &self.0
    }
}

impl EngineSession {
    /// Read-only transcript replay. Ends by itself once the load's updates
    /// stop arriving; there is nothing to kill and nothing to end.
    ///
    /// The `local_sink` is honoured, but the desktop reads the replay off
    /// [`core::EngineSession::subscribe`] instead: the backlog carries every
    /// event the load produced, so a view that attaches after the load
    /// finished still renders the whole transcript.
    pub fn open_transcript(open: OpenTranscript) -> Result<EngineSession, EngineError> {
        let OpenTranscript {
            runtime,
            data_dir,
            personal_key,
            handle,
            local_sink,
        } = open;
        let HistoryHandle {
            agent,
            cwd,
            acp_session_id,
            native,
        } = handle;
        let kind = AdapterKind::from_agent(agent);
        let child_exit = ChildExitLink::new();
        // The recorded ACP id is what `session/load` takes; a run recorded without
        // one falls back to the agent-native handle the adapter knows how to
        // reopen.
        let resume = match acp_session_id {
            Some(id) => ResumeHandle::Acp(id),
            None => native,
        };
        let adapter = Adapter::new(AdapterSpec {
            kind,
            agent,
            spawn: terminal::pty::SpawnSpec {
                program: agent.default_binary().to_string(),
                args: Vec::new(),
                cwd: Some(cwd.clone()),
                env: Vec::new(),
            },
            options: coding::LaunchOptions {
                agent,
                model: String::new(),
                effort: String::new(),
                ultracode: false,
                plan_mode: false,
                subagent_model: String::new(),
                mcp_server_ids: Vec::new(),
                account: None,
                computer_use: None,
            },
            // A replay never talks to MCP: it reads history and stops.
            mcp: coding::AgentMcp::ClaudeFile,
            servers: Vec::new(),
            cwd: cwd.clone(),
            session_id: String::new(),
            prompt: None,
            resume: Some(resume.clone()),
            replay: true,
            personal_key: personal_key.clone(),
            reaper_settings_path: None,
            // A replay never prompts; the bare playbook keeps codex's
            // `thread/resume` developer message stable.
            system_append: coding::skill::system_append(None),
            // A transcript replay draws no bar: it never prompts, so nothing
            // ever measures a prefix to subtract from.
            context_layers: coding::ContextLayers::default(),
            name_conversation: false,
            aside: false,
            // A replay never runs a tool.
            confine_to: None,
            exit: child_exit.clone(),
        })?;

        // A replay's session id is local bookkeeping only — no row exists.
        let session_id = format!("replay-{}", uuid::Uuid::new_v4());
        let lifecycle = Arc::new(RunLifecycle::new(LifecycleSpec {
            // A replay owns no row: its backend never calls the server.
            trpc: Arc::new(api::TrpcClient::new("http://127.0.0.1", Arc::new(|| None))),
            runtime: Arc::clone(&runtime),
            host: Arc::new(NoHost),
            kill: KillFeed::inert(),
            session_id: session_id.clone(),
            conversation: None,
            attachments: None,
            embeds: steer::ImageEmbeds::default(),
            audience: None,
        }));
        let session = SessionSpec {
            session_id,
            run: RunFacts {
                worktree: cwd.clone(),
                ..RunFacts::default()
            },
            runtime: runtime.handle().clone(),
            data_dir,
            personal_key,
            persistent: false,
            seed_message_id: None,
            rotation_host: false,
            publish: false,
            local_sink: Some(local_sink),
            agent,
            // A replay spawned nothing, so there is nothing to mask.
            mcp_secrets: Vec::new(),
            replay: true,
            // The HOST issues `session/load` for it: a replay that opened a
            // fresh `session/new` would show an empty transcript.
            resume: Some(resume),
            prompt: None,
            aside: false,
            child_exit,
            attachments: None,
            host_tools: Arc::new(ExpHostTools),
            sink: None,
        };
        core::start_with(session, adapter, lifecycle).map(EngineSession)
    }

    /// Blocks until the run ends. The FULL exit (with the row's last status
    /// write) went to
    /// [`EngineHost::on_exit`](crate::EngineHost::on_exit); this rebuilds the
    /// same exit with `end: None`, which is all a waiter needs — the
    /// `SessionEndObserver` already applied that result (EXP-641).
    pub fn wait(&self) -> EngineExit {
        widen(self.0.wait())
    }

    pub fn wait_timeout(&self, timeout: Duration) -> Option<EngineExit> {
        self.0.wait_timeout(timeout).map(widen)
    }

    /// EXP-1005: the agent's usage wall as the engine last saw it (`None` =
    /// not blocked) — what a host's account-rotation beat reads, together
    /// with the turn slot, to decide whether this run may move to another
    /// account right now. An expired wall (its own reset stamp passed)
    /// reads as `None`, by the same rule the synced row follows.
    pub fn blocked(&self) -> Option<steer::SessionBlocked> {
        self.0
            .blocked_wall()
            .filter(|wall| !steer::blocked_wall_expired(wall, steer::now_unix_millis()))
    }
}

/// The core's exit (no backend result) as Exponential's.
fn widen(exit: eagent::engine::EngineExit) -> EngineExit {
    EngineExit {
        session_id: exit.session_id,
        outcome: exit.outcome,
        child: exit.child,
        error: exit.error,
        end: None,
    }
}

/// Start a run. Non-blocking; handshake failures arrive through
/// [`EngineHost::on_exit`].
pub fn start(start: EngineStart, host: Arc<dyn EngineHost>) -> Result<EngineSession, EngineError> {
    debug_assert!(
        start.prepared.launch_hold.is_none(),
        "EXP-746 (EXP-478): the host takes `prepared.launch_hold` BEFORE engine::start \
         and drops it only after it registered the session"
    );
    let acp = start.prepared.acp.clone();
    let agent = start.prepared.agent;
    let child_exit = ChildExitLink::new();
    let adapter = Adapter::new(AdapterSpec {
        kind: AdapterKind::from_agent(agent),
        agent,
        spawn: start.prepared.spawn.clone(),
        options: acp.options.clone(),
        mcp: acp.mcp.clone(),
        servers: acp.servers.clone(),
        cwd: start.prepared.worktree.clone(),
        session_id: acp.session_id.clone(),
        prompt: acp.prompt.clone(),
        resume: acp.resume.clone().map(ResumeHandle::from),
        replay: false,
        personal_key: start.personal_key.clone(),
        reaper_settings_path: acp.reaper_settings_path.clone(),
        system_append: acp.system_append.clone(),
        context_layers: acp.context_layers.clone(),
        name_conversation: names_conversation(&start.data_dir, &start.prepared),
        aside: is_aside_run(&start.data_dir, &start.prepared.session_id),
        // N-5 / NEW-5: a repo-less run is confined to its scratch dir.
        confine_to: acp.confine_to.clone(),
        exit: child_exit.clone(),
    })?;
    start_with(
        start,
        host,
        EngineParts {
            adapter,
            child_exit,
            sink: None,
            conversation: None,
        },
    )
}

/// [`start`] with the transport handed in. The ONE seam a test drives.
pub fn start_with<A>(
    start: EngineStart,
    host: Arc<dyn EngineHost>,
    parts: EngineParts<A>,
) -> Result<EngineSession, EngineError>
where
    A: ConnectTo<Client> + 'static,
{
    let EngineStart {
        prepared,
        trpc,
        runtime,
        data_dir,
        // EXP-105/EXP-444 identity: carried for the hosts; the connection
        // loop itself never needed it.
        account_id: _,
        own_user_id: _,
        personal_key,
        persistent,
        // EXP-444: the ACP path has no login affordance to suppress.
        foreign_host: _,
        rotation_host,
        publish,
        kill,
        local_sink,
    } = start;
    let agent = prepared.agent;
    let acp = prepared.acp.clone();
    let aside = is_aside_run(&data_dir, &prepared.session_id);
    // EXP-825: the ONE localizer of this run — the seed prompt, a local
    // composer message and (through the lifecycle's publisher) every steered
    // message download into the worktree's steer-images dir.
    let hook = steer::image_localizer(
        Arc::clone(&trpc),
        crate::host::steer_images_dir(&prepared.worktree),
    );
    let embeds = steer::ImageEmbeds::default();
    let attachments = crate::host::steer_localizer(&hook, &embeds);
    let lifecycle = Arc::new(RunLifecycle::new(LifecycleSpec {
        trpc,
        runtime: Arc::clone(&runtime),
        host,
        kill,
        session_id: prepared.session_id.clone(),
        conversation: parts.conversation,
        attachments: Some(hook),
        embeds,
        audience: prepared.audience.clone(),
    }));
    let session = SessionSpec {
        session_id: prepared.session_id.clone(),
        run: RunFacts {
            worktree: prepared.worktree.clone(),
            branch: prepared.branch.clone(),
            base_ref: prepared.base_ref.clone(),
            repository_id: prepared.repository_id.clone(),
            clone: prepared.clone.clone(),
        },
        runtime: runtime.handle().clone(),
        data_dir,
        personal_key,
        persistent,
        seed_message_id: prepared.seed_message_id.clone(),
        rotation_host,
        publish,
        local_sink,
        agent,
        // EXP-792: the team servers' credential values, masked like the key.
        mcp_secrets: acp.mcp_secrets.clone().into_vec(),
        replay: false,
        resume: acp.resume.clone().map(ResumeHandle::from),
        prompt: acp.prompt.clone(),
        aside,
        child_exit: parts.child_exit,
        attachments: Some(attachments),
        host_tools: Arc::new(ExpHostTools),
        sink: parts.sink,
    };
    core::start_with(session, parts.adapter, lifecycle).map(EngineSession)
}

/// EXP-1134/EXP-1292: whether the adapter asks the agent to name this
/// conversation — a FRESH thread session with nothing else to be listed
/// under: no linked issue, no action and no title of its own (its run
/// record says which, as `prepare` wrote it; EXP-1316: an action run's
/// record carries the action name as its title, so it is never named). A resume keeps the name its transcript
/// carries; an agent root is listed under its own name.
/// EXP-1330: the run record `coding::aside` wrote marks a `/btw` aside.
fn is_aside_run(data_dir: &Path, session_id: &str) -> bool {
    coding::run_registry::get(data_dir, session_id).is_some_and(|record| record.is_aside())
}

fn names_conversation(data_dir: &Path, prepared: &coding::PreparedLaunch) -> bool {
    prepared.acp.resume.is_none()
        && coding::run_registry::get(data_dir, &prepared.session_id)
            .is_some_and(|record| record_wants_name(&record))
}

/// EXP-1341: every session whose stored title is blank asks its agent CLI
/// to name the conversation (children, issue and action threads included);
/// the CLI's title is reported through `sessions.report`, which fills only a
/// blank title. NEW-8: a persistent kind (an agent's conversation) never
/// asks: its name is the agent DEFINITION's (`agents`), never a title.
fn record_wants_name(record: &coding::run_registry::RunRecord) -> bool {
    !record.kind.is_persistent() && record.title.as_deref().is_none_or(|title| title.trim().is_empty())
}


/// EXP-1292: the session pickup drives a live engine through this — the
/// database is the bus, so a person's message, an unqueue, an answered card
/// and an ended row all arrive off the synced shapes instead of the relay.
impl coding::pickup::LiveEngine for EngineSession {
    fn deliver(&self, message_id: String, text: String) {
        self.0.deliver(message_id, text);
    }

    fn unqueue(&self, message_id: &str) {
        self.0.unqueue(message_id);
    }

    fn answer_card(&self, question_id: &str, card: &serde_json::Value) {
        self.0.answer_card(question_id, card);
    }

    /// EXP-1280: an answered tool-posted card (`exponential_plan_propose`,
    /// `exponential_sessions_ask`): an approval leaves plan mode SILENTLY
    /// first, then the answer goes in as the next turn — a fresh approval
    /// in a fresh context.
    fn deliver_answer(&self, message_id: String, text: String, approved: bool, fresh: bool) {
        if approved {
            self.set_plan_mode_silent(false);
        }
        if fresh {
            self.deliver_fresh(message_id, text);
        } else {
            self.0.deliver(message_id, text);
        }
    }

    fn open_cards(&self) -> std::collections::HashMap<String, String> {
        self.0.open_cards()
    }

    /// EXP-1292: the row ended on the server — the agent's own
    /// `sessions_end` mid-turn, or a person. EXP-637's graceful stop: a turn
    /// in flight finishes first (≤ `steer::STOP_GRACE`), so its close-out
    /// reply still reaches the conversation; between turns (or before the
    /// session is live) it goes at once.
    fn kill(&self, outcome: &'static str) {
        if self.is_done() {
            return;
        }
        let waiter = self.0.turn_signal().subscribe();
        if self.phase() != Some(EnginePhase::Live) || waiter.try_recv().is_ok() {
            self.0.kill(outcome);
            return;
        }
        let session = self.clone();
        let spawned = std::thread::Builder::new()
            .name("engine-graceful-kill".to_string())
            .spawn(move || {
                let _ = waiter.recv_timeout(steer::STOP_GRACE);
                session.0.kill(outcome);
            });
        if spawned.is_err() {
            self.0.kill(outcome);
        }
    }

    fn is_done(&self) -> bool {
        self.0.is_done()
    }

    /// EXP-849: the turn slot — a switch waits for it.
    fn is_idle(&self) -> bool {
        self.turn_signal().is_idle()
    }

    /// EXP-1252: a move waits for no turn, no background task (the
    /// synced `busy` input) and no open card.
    fn quiescent(&self) -> bool {
        !self.agent_busy() && self.turn_signal().is_idle() && self.0.open_cards().is_empty()
    }

    /// S-4: FEED-25's activity clock — the idle release never counts a
    /// chat quiet from before its last output.
    fn last_activity(&self) -> Option<std::time::Instant> {
        Some(self.0.last_activity())
    }

    /// EXP-1292: the account switch's stop — the row idles, never ends.
    fn suspend(&self) {
        self.0.kill(crate::lifecycle::SUSPEND_OUTCOME);
    }

    /// EXP-1334: a person's `sessions.configure` moved plan mode — the ONE
    /// steerable mode on both adapters. EXP-1280: Exponential's flag (never
    /// the CLI's), and a real move steers the agent one line.
    fn set_plan_mode(&self, on: bool) {
        self.set_mode(plan_mode_id(on));
    }

    fn plan_mode(&self) -> Option<bool> {
        self.0.plan_mode()
    }

    /// EXP-1334: a person's `sessions.configure{model}` — the live `model`
    /// option both adapters take (`/model <alias>`).
    fn set_model(&self, model: &str) {
        self.set_config("model", ConfigValue::ValueId(model.to_string()));
    }

    fn model(&self) -> Option<String> {
        self.0.model()
    }

    fn wait_done(&self, timeout: std::time::Duration) -> bool {
        self.wait_timeout(timeout).is_some()
    }
}

/// `open_transcript` answers to nobody: a replay has no row, no publisher and
/// no host bookkeeping to unwind.
struct NoHost;

impl EngineHost for NoHost {
    fn on_exit(&self, _exit: EngineExit) {}
}

// EXP-1426 part 4c: the resume handle and the start error are the agent
// runtime's (the adapters return them); `coding` converts its `ResumeSeed`
// and `api` its `ApiError`.
pub use eagent::engine::launch::{EngineError, ResumeHandle};

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-1134/1292/1341: any session with a blank stored title asks its
    /// agent for a name (issues aside); a named one never does, nor (NEW-8)
    /// an agent's conversation.
    #[test]
    fn every_untitled_session_is_named_by_its_agent() {
        let record = |kind: &str, issues: serde_json::Value, title: serde_json::Value| {
            let record: coding::run_registry::RunRecord = serde_json::from_value(serde_json::json!({
                "sessionId": "sess-1",
                "accountId": "acct-1",
                "agent": "claude",
                "kind": kind,
                "issues": issues,
                "title": title,
                "cwd": "/tmp/worktree",
                "recordedAt": 1_700_000_000u64,
            }))
            .expect("the record decodes");
            record
        };
        let none = serde_json::Value::Null;
        assert!(record_wants_name(&record("thread", serde_json::json!([]), none.clone())));
        assert!(record_wants_name(&record("thread", serde_json::json!([]), serde_json::json!(" "))));
        assert!(record_wants_name(&record("thread", serde_json::json!(["EXP-1"]), none.clone())));
        assert!(!record_wants_name(&record("thread", serde_json::json!([]), serde_json::json!("Named"))));
        assert!(!record_wants_name(&record("agent", serde_json::json!([]), serde_json::json!("Triage"))));
        assert!(!record_wants_name(&record("agent", serde_json::json!([]), none)));
    }

    #[test]
    fn a_resume_seed_becomes_a_resume_handle() {
        assert_eq!(
            ResumeHandle::from(coding::ResumeSeed::Acp("sess-1".into())),
            ResumeHandle::Acp("sess-1".into())
        );
        assert_eq!(
            ResumeHandle::from(coding::ResumeSeed::Native("uuid-1".into())),
            ResumeHandle::Native("uuid-1".into())
        );
    }
}
