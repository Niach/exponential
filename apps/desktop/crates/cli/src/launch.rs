//! The CLI's launch plumbing, shared by `code`/`run` and the daemon.
//!
//! EXP-1292: the database is the bus. Every session — a person's `code`, an
//! action `run`, a web start, an agent's child — is a `queued` `sessions` row
//! naming a device, and the ONE launch path is `coding::pickup::launch_queued`
//! (claim → brief → prepare → spawn). This module builds what it needs from
//! a CLI context: the launcher collaborators ([`coding_deps`]), the pickup's
//! host half ([`pickup_deps`], whose spawn is `session_host::launch`), and the
//! foreground start `code`/`run` share ([`start_foreground`]) with its own
//! one-session pickup loop ([`spawn_foreground_pickup`]).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context as _};
use api::mcp_tools::{self, RemoteSession, SessionsSendInput};
use coding::pickup::{ClaimLock, LaunchQueued, LiveEngine, LiveRegistry, PickupDeps};
use coding::{CodingDeps, GitWorktrees, LaunchOptions};
use domain::rows::SessionRow;
use serde::Deserialize;

use crate::context::Ctx;
use crate::session_host::{self, LaunchEnv, RunningSession};

/// Agent-related flags shared by `code` and `run`.
#[derive(Debug, Default)]
pub struct AgentFlags {
    pub agent: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub plan: bool,
}

/// Resolve flags over the settings defaults with the desktop's capability
/// masking. Detached (non-tty) runs default plan mode OFF — the F7 rule:
/// an unattended session must never park at the plan-approval TUI unless
/// explicitly asked to (`--plan`; it stays steerable from the web).
pub fn agent_options(
    settings: &coding::Settings,
    flags: &AgentFlags,
    interactive: bool,
) -> anyhow::Result<LaunchOptions> {
    let agent = match flags.agent.as_deref() {
        None => settings.default_agent,
        Some(raw) => coding::CodingAgent::parse(raw)
            .ok_or_else(|| anyhow!("unknown agent `{raw}` (claude or codex)"))?,
    };
    let mut options = LaunchOptions::defaults_for(settings, agent);
    if let Some(model) = &flags.model {
        options.model = model.clone();
    }
    if let Some(effort) = &flags.effort {
        options.effort = effort.clone();
    }
    if !interactive {
        options.plan_mode = false;
    }
    // EXP-1280: plan mode is Exponential's, every agent's.
    if flags.plan {
        options.plan_mode = true;
    }
    Ok(options)
}

/// The injected launcher collaborators.
///
/// EXP-1292: `device_id` is ALWAYS this machine's CLI device — a session row
/// names the device that runs it, the daemon and a foreground `code` alike
/// (the old foreground/daemon split existed only because the launcher
/// created the row).
///
/// `runtime` is this host's steer runtime (`None` = it failed to start): the
/// ACP engine drives its connection on it, so a runtime-less process has
/// nowhere to run a session at all (EXP-773 — `prepare` refuses the launch
/// with [`coding::DisabledReason::AcpUnavailable`] rather than spawning
/// something nothing can drive).
pub fn coding_deps(ctx: &Ctx, runtime: Option<&Arc<steer::SteerRuntime>>) -> CodingDeps {
    CodingDeps {
        trpc: Arc::clone(&ctx.trpc),
        token_store: Arc::clone(&ctx.token_store),
        account_id: ctx.account.id.clone(),
        settings: ctx.settings.clone(),
        worktrees: Arc::new(GitWorktrees),
        codex_sessions_root: None,
        claude_projects_root: None,
        device_id: Some(ctx.device_id()),
        // EXP-746: `session_host::launch` runs the engine on the steer
        // runtime, so "this host can run one" IS "it has a runtime".
        acp_available: runtime.is_some(),
        data_dir: ctx.data_dir.clone(),
    }
}

/// Called once a launched session is up, BEFORE the pickup registers its
/// engine: the host's own registration (the daemon's live list, the
/// foreground's slot), after which it releases the launch gate (EXP-758).
pub type OnLaunched = Arc<dyn Fn(Arc<RunningSession>, Option<coding::RunCleanup>) + Send + Sync>;

/// Everything a host hands [`pickup_deps`].
pub struct PickupHost {
    pub ctx: Arc<Ctx>,
    pub runtime: Option<Arc<steer::SteerRuntime>>,
    pub personal_key: Option<String>,
    /// EXP-1005: the daemon runs the rotation beat; a foreground run does not.
    pub rotation_host: bool,
    /// Fresh settings for this pass (the daemon re-reads them; they move at
    /// runtime through `remote_admin`).
    pub settings: coding::Settings,
    pub options: LaunchOptions,
    pub live: Arc<LiveRegistry>,
    /// The root's linked issue identifiers, link order.
    pub issue_identifiers: Arc<dyn Fn(&str) -> Vec<String> + Send + Sync>,
    /// EXP-444: whether the session's starter is not this account.
    pub foreign_host: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    pub on_launched: OnLaunched,
}

/// The pickup's host half: its spawn is [`session_host::launch`], which
/// hands back the engine handle the pickup drives.
pub fn pickup_deps(host: PickupHost) -> PickupDeps {
    let mut coding = coding_deps(&host.ctx, host.runtime.as_ref());
    coding.settings = host.settings;
    let ctx = Arc::clone(&host.ctx);
    let runtime = host.runtime.clone();
    let personal_key = host.personal_key.clone();
    let rotation_host = host.rotation_host;
    let foreign_host = Arc::clone(&host.foreign_host);
    let on_launched = Arc::clone(&host.on_launched);
    let launch: coding::pickup::LaunchFn = Arc::new(move |prepared: coding::PreparedLaunch| {
        let env = LaunchEnv {
            ctx: &ctx,
            runtime: runtime.as_ref(),
            personal_key: personal_key.clone(),
            rotation_host,
        };
        let foreign = foreign_host(&prepared.session_id);
        // EXP-637: snapshotted before the launch consumes the prepared value.
        let cleanup = prepared.run_cleanup.clone();
        let session = session_host::launch(&env, prepared, foreign)
            .map(Arc::new)
            .map_err(|err| format!("{err:#}"))?;
        log::info!(
            "session {} started ({}, branch {})",
            session.session_id,
            session.issue_identifier,
            session.branch
        );
        let engine: Arc<dyn LiveEngine> = Arc::new(session.engine());
        on_launched(session, cleanup);
        Ok(engine)
    });
    PickupDeps {
        coding: Arc::new(coding),
        device_id: host.ctx.device_id(),
        device_label: coding::default_device_label(),
        options: host.options,
        live: host.live,
        launch,
        issue_identifiers: host.issue_identifiers,
    }
}

/// EXP-1292: the claim-lock id a foreground `code`/`run` holds from BEFORE
/// its `sessions.send` until it claimed the row it queued. The daemon on the
/// same machine skips every launch while it is held (`daemon::pickup_pass`),
/// so it never races the foreground for the row it is about to run — the
/// row names the SAME device, and the server cannot tell the two apart.
pub const FOREGROUND_START_LOCK: &str = "foreground-start";

/// How long a foreground start waits for another one's lock.
const FOREGROUND_LOCK_WAIT: Duration = Duration::from_secs(30);

/// A session `code`/`run` started and runs in THIS process.
pub struct ForegroundSession {
    pub session: Arc<RunningSession>,
    /// Holds the session's claim lock for the process's life.
    pub _live: Arc<LiveRegistry>,
    /// The session's pickup loop (people's messages, answered cards, an
    /// end): stops with the engine.
    pub _pickup: Option<std::thread::JoinHandle<()>>,
}

/// What [`start_foreground`] ended with.
pub enum ForegroundStart {
    Running(ForegroundSession),
    /// The launcher refused (doctor, ACP, …): the reason, already replied
    /// into the conversation and the row settled by the pickup.
    Refused(String),
}

/// EXP-1292: `code`/`run` on THIS machine. Queue the session naming this
/// device (`exponential_sessions_send`), then claim and run it right here
/// through the same pickup the daemon uses. `input` carries the subject
/// (issues / action + inputs / text); this fills the device and the
/// pickers.
pub fn start_foreground(
    ctx: Arc<Ctx>,
    mut input: SessionsSendInput,
    options: LaunchOptions,
) -> anyhow::Result<ForegroundStart> {
    // EXP-746: the shared registry decision for every end this process
    // issues (daemon parity — `registry::install_end_observer`).
    crate::registry::install_end_observer(ctx.data_dir.clone());
    // EXP-746: the runtime is resolved BEFORE `prepare` — the launcher
    // refuses a host that cannot run the engine.
    let runtime = steer::SteerRuntime::new().ok();
    let personal_key = crate::context::ensure_personal_key(&ctx).ok();

    fill_foreground_input(&mut input, &ctx.device_id(), &options);
    let gate = wait_for_foreground_gate(&ctx.data_dir)?;
    let sent = mcp_tools::sessions_send(&ctx.trpc, &input)
        .map_err(crate::commands::devices::tool_error)
        .context("start the session")?;
    let tree = mcp_tools::sessions_get(&ctx.trpc, &sent.session_id)
        .map_err(crate::commands::devices::tool_error)
        .context("load the session")?;
    let remote = tree.session(&sent.session_id).unwrap_or(&tree.root);
    let row = session_row_from_remote(remote, &input);
    let identifiers: Vec<String> = tree
        .issues
        .iter()
        .filter_map(|issue| issue.identifier.clone())
        .collect();

    let live = answer_tapped_registry(Arc::clone(&ctx.trpc));
    let slot: Arc<Mutex<Option<Arc<RunningSession>>>> = Arc::new(Mutex::new(None));
    let deps = Arc::new(pickup_deps(PickupHost {
        ctx: Arc::clone(&ctx),
        runtime,
        personal_key,
        rotation_host: false,
        settings: ctx.settings.clone(),
        options,
        live: Arc::clone(&live),
        issue_identifiers: Arc::new(move |_root: &str| identifiers.clone()),
        // `code`/`run` started it as this account.
        foreign_host: Arc::new(|_session_id: &str| false),
        on_launched: {
            let slot = Arc::clone(&slot);
            Arc::new(move |session: Arc<RunningSession>, _cleanup| {
                // EXP-758 (EXP-478): this process is the run's only registry:
                // there is no session list to push into, so the gate ends here.
                session.release_launch_hold();
                if let Ok(mut slot) = slot.lock() {
                    *slot = Some(session);
                }
            })
        },
    }));
    let outcome = coding::pickup::launch_queued(&row, &deps).map_err(|err| anyhow!(err));
    drop(gate);
    match outcome? {
        LaunchQueued::Launched { .. } => {
            let session = slot
                .lock()
                .ok()
                .and_then(|mut slot| slot.take())
                .ok_or_else(|| anyhow!("the session launched but its handle was lost"))?;
            let pickup = spawn_foreground_pickup(ctx, deps, sent.session_id.clone());
            Ok(ForegroundStart::Running(ForegroundSession { session, _live: live, _pickup: pickup }))
        }
        LaunchQueued::Disabled { reason } => Ok(ForegroundStart::Refused(reason)),
        LaunchQueued::Skipped { reason } => bail!(
            "session {} was not started here ({reason}) — the daemon on this machine may be \
running it; steer it from the web or `exponential sessions log {} --follow`",
            sent.session_id,
            sent.session_id
        ),
    }
}

/// How often the foreground loop re-plans with no delta (a missed edge),
/// and how often it falls back to polling the row while its shapes are not
/// up (cold, or a build whose sync has no spec for them).
const FOREGROUND_TICK: Duration = Duration::from_secs(10);
const FOREGROUND_POLL: Duration = Duration::from_secs(15);

/// The shapes the foreground loop plans over (it plans only once all
/// reached `up-to-date`: a cold store would read the row as gone → Kill).
/// NEW-8: the messages wait on their conversations (a message's session is
/// its conversation's).
const FOREGROUND_SHAPES: &[&str] = &["sessions", "messages", "conversations"];

/// EXP-1292: the foreground `code`/`run` process is the ONLY host of its
/// session — it holds the claim lock, so the daemon on the same machine
/// skips the row — so it runs its own one-session pickup: the person
/// messages sent from the web are delivered, answered question/plan cards
/// reach the engine, and an `ended` row (Stop/end, the agent's
/// `sessions_end`) kills the run. It syncs the `sessions` + `messages` (+ NEW-8 `conversations`)
/// shapes into a private store (deleted at the end) and plans over them
/// like the daemon, acting only on its own session; until those shapes are
/// up it polls the row for an end over MCP. The thread stops with the
/// engine.
fn spawn_foreground_pickup(
    ctx: Arc<Ctx>,
    deps: Arc<PickupDeps>,
    session_id: String,
) -> Option<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("exp-foreground-pickup".to_string())
        .spawn(move || run_foreground_pickup(&ctx, &deps, &session_id))
        .inspect_err(|err| log::warn!("foreground pickup failed to spawn: {err}"))
        .ok()
}

fn run_foreground_pickup(ctx: &Ctx, deps: &PickupDeps, session_id: &str) {
    let store_path = ctx
        .data_dir
        .join("accounts")
        .join(&ctx.account.id)
        .join(format!("sync-foreground-{session_id}.sqlite"));
    let manager = start_foreground_sync(ctx, &store_path);
    let deltas = manager.as_ref().map(|manager| manager.deltas());
    let mut synced: std::collections::HashSet<&'static str> = std::collections::HashSet::new();
    let mut last_plan: Option<Instant> = None;
    let mut last_poll: Option<Instant> = None;
    let mut refused_switch: Option<String> = None;
    let live = || deps.live.get(session_id).is_some_and(|engine| !engine.is_done());
    while live() {
        let mut changed = false;
        match &deltas {
            Some(deltas) => {
                if let Ok(first) = deltas.recv_timeout(Duration::from_secs(1)) {
                    for delta in std::iter::once(first).chain(deltas.try_iter()) {
                        if let sync::ShapeDelta::Applied { shape, up_to_date, .. } = delta {
                            if up_to_date {
                                synced.insert(shape);
                            }
                            changed |= FOREGROUND_SHAPES.contains(&shape);
                        }
                    }
                }
            }
            None => std::thread::sleep(Duration::from_secs(1)),
        }
        let ready = FOREGROUND_SHAPES.iter().all(|shape| synced.contains(shape));
        let store = manager.as_ref().and_then(|manager| manager.store(&ctx.account.id));
        match store {
            Some(store) if ready => {
                if changed || last_plan.is_none_or(|at| at.elapsed() >= FOREGROUND_TICK) {
                    last_plan = Some(Instant::now());
                    foreground_pass(&store, deps, session_id, &mut refused_switch);
                }
            }
            _ => {
                if last_poll.is_none_or(|at| at.elapsed() >= FOREGROUND_POLL) {
                    last_poll = Some(Instant::now());
                    poll_for_end(ctx, deps, session_id);
                }
            }
        }
    }
    if let Some(manager) = manager {
        manager.stop_all();
    }
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut path = store_path.clone().into_os_string();
        path.push(suffix);
        let _ = std::fs::remove_file(path);
    }
}

/// The private store's pipeline (sessions + messages + conversations only). `None` leaves
/// the loop on the MCP end poll.
fn start_foreground_sync(ctx: &Ctx, db_path: &std::path::Path) -> Option<Arc<sync::SyncManager>> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok()?;
    }
    let manager = Arc::new(sync::SyncManager::new());
    let config = sync::AccountSyncConfig {
        account_id: ctx.account.id.clone(),
        base_url: ctx.account.instance_url.clone(),
        db_path: db_path.to_path_buf(),
        token: ctx.auth.token_provider_fn(&ctx.account.id),
        shapes: Some(FOREGROUND_SHAPES),
    };
    match manager.start_account(config) {
        Ok(_) => Some(manager),
        Err(err) => {
            log::warn!("foreground pickup: store failed to open ({err}) — polling for an end only");
            None
        }
    }
}

/// One plan over the private store, acting on THIS session only: deliveries
/// to the live engine, revokes, answers and the kill. Never a launch or a
/// resume — this process runs one engine and exits with it.
fn foreground_pass(
    store: &sync::store::ShapeStore,
    deps: &PickupDeps,
    session_id: &str,
    refused_switch: &mut Option<String>,
) {
    use coding::pickup::PickupAction;
    let rows: Vec<SessionRow> = crate::commands::daemon::read_shape_rows(store, "sessions");
    // NEW-8: keyed through their conversation.
    let messages: Vec<domain::rows::MessageRow> = crate::commands::daemon::read_keyed_messages(store);
    deps.live.settle_handed(&messages);
    let views = deps.live.live_views();
    let in_flight = std::collections::HashSet::new();
    let revive_held = deps.live.revive_held();
    let actions = coding::pickup::plan(coding::pickup::PickupInput {
        device_id: &deps.device_id,
        sessions: &rows,
        messages: &messages,
        live: &views,
        in_flight: &in_flight,
        revive_held: &revive_held,
    });
    for action in actions {
        let ours = match &action {
            PickupAction::Deliver { session_id: id, .. }
            | PickupAction::Revoke { session_id: id, .. }
            | PickupAction::Answer { session_id: id, .. }
            // EXP-1280: an answered tool card is this engine's next turn.
            | PickupAction::DeliverAnswer { session_id: id, .. }
            | PickupAction::Kill { session_id: id }
            // EXP-1252: the row names another device now: this engine stops
            // without a row write.
            | PickupAction::Release { session_id: id } => id == session_id,
            // EXP-1341: a revive is the daemon's (or the desktop's) work —
            // this process runs ONE engine.
            PickupAction::Launch { .. }
            | PickupAction::Resume { .. }
            | PickupAction::Revive { .. } => false,
            // EXP-849: this process runs ONE engine and exits with it, so it
            // never relaunches on another login: the person hears so ONCE
            // per pick, the pick stays UNhonored (the first wake after this
            // run ends makes the switch), and the messages that rode the
            // switch go to this engine meanwhile.
            PickupAction::Switch { session, account, messages } => {
                if session.id == session_id {
                    // EXP-1334: an empty account is a reconfigure, which a
                    // foreground run (no recorded options) is never planned;
                    // nothing to refuse, its messages go to this engine.
                    if !account.is_empty() && note_foreground_switch(refused_switch, account) {
                        log::info!(
                            "foreground pickup: session {session_id} keeps its login — a foreground run cannot switch to {account}"
                        );
                        if let Err(err) = api::session_device::reply(
                            &deps.coding.trpc,
                            session_id,
                            coding::pickup::REASON_FOREGROUND_SWITCH,
                            None,
                        ) {
                            log::warn!("foreground pickup: switch refusal not replied: {err}");
                        }
                    }
                    if !messages.is_empty() {
                        let deliver = PickupAction::Deliver {
                            session_id: session_id.to_string(),
                            messages: messages.clone(),
                        };
                        if let Err(err) = coding::pickup::execute(&deliver, deps) {
                            log::warn!("foreground pickup: {err}");
                        }
                    }
                }
                false
            }
        };
        if ours {
            if let Err(err) = coding::pickup::execute(&action, deps) {
                log::warn!("foreground pickup: {err}");
            }
        }
    }
}

/// EXP-849: whether a foreground run's refusal of the switch to `account`
/// is still to be said — once per pick (a NEW pick says it again).
fn note_foreground_switch(refused: &mut Option<String>, account: &str) -> bool {
    if refused.as_deref() == Some(account) {
        return false;
    }
    *refused = Some(account.to_string());
    true
}

/// The end half without shapes: an `ended` row kills the run.
fn poll_for_end(ctx: &Ctx, deps: &PickupDeps, session_id: &str) {
    match mcp_tools::sessions_get(&ctx.trpc, session_id) {
        Ok(tree)
            if tree
                .session(session_id)
                .is_some_and(|remote| remote.status == domain::contract::SESSION_STATUS_ENDED) =>
        {
            log::info!("session {session_id} was ended — stopping it");
            let kill = coding::pickup::PickupAction::Kill { session_id: session_id.to_string() };
            if let Err(err) = coding::pickup::execute(&kill, deps) {
                log::warn!("foreground pickup: {err}");
            }
        }
        Ok(_) => {}
        Err(err) => log::debug!("foreground pickup: session {session_id} not read: {err}"),
    }
}

/// The device + pickers a foreground start sends: THIS device, the resolved
/// agent and plan mode (the row's pickers win over the launch defaults, so
/// the interactive plan default must ride), model/effort only when typed —
/// absent = the device's own defaults.
fn fill_foreground_input(input: &mut SessionsSendInput, device_id: &str, options: &LaunchOptions) {
    input.device_id = Some(device_id.to_string());
    input.cli = Some(options.agent.id().to_string());
    input.plan_mode = Some(options.plan_mode);
}

/// Take [`FOREGROUND_START_LOCK`], waiting out another foreground start.
fn wait_for_foreground_gate(data_dir: &std::path::Path) -> anyhow::Result<ClaimLock> {
    let deadline = Instant::now() + FOREGROUND_LOCK_WAIT;
    loop {
        if let Some(lock) = ClaimLock::try_acquire(data_dir, FOREGROUND_START_LOCK)
            .map_err(|err| anyhow!(err))?
        {
            return Ok(lock);
        }
        if Instant::now() >= deadline {
            bail!("another `exponential code`/`run` is starting on this machine — try again");
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// The just-queued row as the pickup reads it. The MCP projection carries
/// the server-resolved columns (team, repository, branch, kind, the agent
/// CLI); the pickers it does not project come from what this start sent.
pub fn session_row_from_remote(remote: &RemoteSession, input: &SessionsSendInput) -> SessionRow {
    SessionRow {
        id: remote.id.clone(),
        team_id: remote.team_id.clone(),
        parent_id: remote.parent_id.clone(),
        root_id: remote.root_id.clone(),
        depth: Some(i64::from(remote.depth)),
        kind: remote.kind.clone(),
        user_id: remote.user_id.clone(),
        // Just sent naming this device: a projection that leaves it out
        // (or the status) still reads as the row this start queued.
        device_id: remote.device_id.clone().or_else(|| input.device_id.clone()),
        cli: remote.cli.clone().or_else(|| input.cli.clone()),
        account: remote.account.clone().or_else(|| input.account.clone()),
        model: remote.model.clone().or_else(|| input.model.clone()),
        effort: input.effort.clone(),
        plan_mode: input.plan_mode.unwrap_or(false),
        computer_use: input.computer_use.unwrap_or(false),
        ultracode: input.ultracode.unwrap_or(false),
        // The projection's own flag.
        dispatch: remote.dispatch,
        repository_id: remote.repository_id.clone(),
        branch: remote.branch.clone(),
        status: Some(if remote.status.is_empty() {
            domain::contract::SESSION_STATUS_QUEUED.to_string()
        } else {
            remote.status.clone()
        }),
        busy: remote.busy,
        title: remote.title.clone(),
        name: remote.name.clone(),
        ..SessionRow::default()
    }
}

/// `teams.getDefault` — the non-creating default-team resolver (oldest
/// membership or null); the CLI's team fallback for an action-name lookup.
pub fn default_team_id(trpc: &api::trpc::TrpcClient) -> anyhow::Result<String> {
    #[derive(Deserialize)]
    struct Output {
        #[serde(default)]
        team: Option<TeamRow>,
    }
    #[derive(Deserialize)]
    struct TeamRow {
        id: String,
    }
    let output: Output = trpc
        .query("teams.getDefault")
        .context("resolve the default team")?;
    output
        .team
        .map(|team| team.id)
        .ok_or_else(|| anyhow!("You are not a member of any team yet."))
}

/// EXP-1316: a pickup registry whose sessions' relay answers (the session
/// view) close their synced cards too: the publisher's tap maps the
/// question onto the card at once, `sessions.resolveCard` goes off-thread.
pub fn answer_tapped_registry(trpc: Arc<api::TrpcClient>) -> Arc<LiveRegistry> {
    let live = Arc::new(LiveRegistry::default());
    let registry = Arc::clone(&live);
    steer::answer_tap::add_answer_tap(Arc::new(move |session_id, resolution| {
        if let Some(mut input) = registry.resolve_card_input(
            session_id,
            &resolution.question_id,
            resolution.answer.clone(),
            resolution.decision.clone(),
            resolution.fresh,
            resolution.feedback.clone(),
        ) {
            // EXP-1280: a multi-select's picks ride `answers` too — the
            // closed card highlights off them (the IDE's tap does the same).
            input.answers = resolution.answers.as_ref().map(|labels| serde_json::json!(labels));
            coding::pickup::write_back_card(Arc::clone(&trpc), input);
        }
    }));
    live
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(std::path::PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn temp_ctx(name: &str) -> (TempDir, Ctx) {
        let dir = std::env::temp_dir().join(format!("exp-cli-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let auth = api::accounts::AuthStore::load(dir.clone());
        let account = api::accounts::Account {
            id: "acct-1".to_string(),
            instance_url: "https://example.test".to_string(),
            user_id: "user-1".to_string(),
            email: "dev@example.test".to_string(),
            name: None,
            is_admin: false,
            onboarding_completed_at: None,
        };
        let trpc = Arc::new(api::trpc::TrpcClient::new(
            &account.instance_url,
            auth.token_provider(&account.id),
        ));
        let ctx = Ctx {
            data_dir: dir.clone(),
            auth,
            account,
            trpc,
            token_store: Arc::new(api::token_store::TokenStore::new(dir.clone())),
            settings: coding::Settings::default(),
        };
        (TempDir(dir), ctx)
    }

    /// EXP-1292: a session row names the device that runs it, so every
    /// CLI-hosted launch — daemon or foreground — carries this machine's id.
    #[test]
    fn every_launch_names_this_machines_device() {
        let (_dir, ctx) = temp_ctx("launch-device");
        assert_eq!(
            coding_deps(&ctx, None).device_id.as_deref(),
            Some(ctx.device_id().as_str())
        );
    }

    /// EXP-746: a host that could not start a steer runtime has nowhere to
    /// run the ACP engine, so `prepare` refuses every launch on it.
    #[test]
    fn the_acp_transport_needs_this_hosts_steer_runtime() {
        let (_dir, ctx) = temp_ctx("launch-acp");

        assert!(!coding_deps(&ctx, None).acp_available);

        let Ok(runtime) = steer::SteerRuntime::new() else {
            // No runtime on this machine — the negative half above is still
            // the load-bearing one.
            return;
        };
        assert!(coding_deps(&ctx, Some(&runtime)).acp_available);
    }

    /// EXP-1292: the foreground start names this device and pins the
    /// resolved agent + plan mode; untyped model/effort stay off the wire
    /// (the device's own defaults).
    #[test]
    fn a_foreground_start_names_this_device_and_its_pickers() {
        let settings = coding::Settings::default();
        let options = agent_options(
            &settings,
            &AgentFlags { agent: Some("codex".into()), ..AgentFlags::default() },
            false,
        )
        .unwrap();
        let mut input = SessionsSendInput {
            issue_ids: Some(vec!["i1".into()]),
            ..SessionsSendInput::default()
        };
        fill_foreground_input(&mut input, "cli-dev", &options);
        assert_eq!(
            serde_json::to_value(&input).unwrap(),
            serde_json::json!({
                "deviceId": "cli-dev",
                "cli": "codex",
                "planMode": false,
                "issueIds": ["i1"],
            })
        );
    }

    /// The pickup reads the server's projection, topped up with the pickers
    /// this start sent (the projection does not carry them).
    #[test]
    fn the_queued_row_merges_the_projection_and_the_sent_pickers() {
        let remote = RemoteSession {
            id: "s1".into(),
            team_id: Some("t1".into()),
            kind: Some("thread".into()),
            device_id: Some("cli-dev".into()),
            cli: Some("claude".into()),
            repository_id: Some("r1".into()),
            status: "queued".into(),
            ..RemoteSession::default()
        };
        let input = SessionsSendInput {
            effort: Some("high".into()),
            plan_mode: Some(true),
            ..SessionsSendInput::default()
        };
        let row = session_row_from_remote(&remote, &input);
        assert!(row.status_is(domain::contract::SESSION_STATUS_QUEUED));
        assert!(row.on_device("cli-dev"));
        assert_eq!(row.repository_id.as_deref(), Some("r1"));
        assert_eq!(row.cli.as_deref(), Some("claude"));
        assert_eq!(row.effort.as_deref(), Some("high"));
        assert!(row.plan_mode);
        assert!(!row.is_agent());
        assert_eq!(row.root_id_or_self(), "s1");
    }

    /// Two foreground starts on one machine serialize on the gate the
    /// daemon also checks.
    #[test]
    fn the_foreground_gate_is_exclusive() {
        let (dir, _ctx) = temp_ctx("launch-gate");
        let first = wait_for_foreground_gate(&dir.0).expect("first takes it");
        assert!(ClaimLock::try_acquire(&dir.0, FOREGROUND_START_LOCK)
            .unwrap()
            .is_none());
        drop(first);
        assert!(ClaimLock::try_acquire(&dir.0, FOREGROUND_START_LOCK)
            .unwrap()
            .is_some());
    }

    /// EXP-849: a foreground run says it cannot switch ONCE per pick, and
    /// never stamps the pick honored (the wake after it ends switches).
    #[test]
    fn a_foreground_run_refuses_each_account_pick_once() {
        let mut refused = None;
        assert!(note_foreground_switch(&mut refused, "aaaa"));
        assert!(!note_foreground_switch(&mut refused, "aaaa"));
        assert!(note_foreground_switch(&mut refused, "bbbb"));
        assert!(!note_foreground_switch(&mut refused, "bbbb"));
        assert!(note_foreground_switch(&mut refused, "aaaa"));
    }
}
