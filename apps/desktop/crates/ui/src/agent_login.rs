//! EXP-484 (C1 + D): signing an agent CLI in FROM the IDE — locally from a
//! Sign in button, or remotely off an `agent_login` device command.
//!
//! The product never holds or copies a credential of its own: this opens the
//! agent's OWN login command in a visible terminal tab and lets the CLI do
//! its thing. What the desktop adds is choreography:
//!
//! * every login runs in a fresh STAGING dir (`coding::agent_login::
//!   begin_login`), never inside an existing profile or the ambient login, so
//!   there is nothing to sign out of first;
//! * a CLEAN exit COMMITS the login by email (`commit_login`): the profile
//!   already signed in as that address takes the fresh credential (a
//!   `{email} was already added. Refreshed it.` warning when the person aimed
//!   elsewhere), a new address becomes a new profile;
//! * a REMOTE run watches the grid and completes its device command EARLY,
//!   the moment the sign-in URL (+ codex's device code) is up — the
//!   requester needs the link, not the eventual outcome. Where it landed
//!   reaches the requester through the synced `devices` row (`lastLoginAt`).
//!
//! Every path ends the same way, through [`LoginRun::finish`]: the child
//! exits — OR the user closes the tab, which never fires an exit hook — and
//! the run answers its command, commits (or drops) the staging dir and
//! re-probes, so the machine's row (and the Tools pane) tells the truth
//! within a beat. A CLEAN exit also closes the tab itself (EXP-695): a
//! finished sign-in has nothing left to read, while a failed one keeps its
//! tab so the error stays on screen.
//!
//! EXP-765: claude's link carries `code=true` — the browser page ends by
//! showing an authorization CODE the CLI in the tab is still waiting for
//! ("Paste code here if prompted >"). A requester on another device hands it
//! back as an `agent_login_code` command; [`enter_remote_code`] finds the
//! agent's live login tab in [`LoginTabs`] and types it there.
//!
//! IMPORT (`agent_login {agent, import: "true"}`, or the doctor's Import pill
//! here) moves the agent's AMBIENT login into a profile by the same commit,
//! with no tab at all ([`import_ambient_login`]).

use crate::toast::Toast;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use gpui::{
    div, px, size, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    spinner::Spinner,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use terminal::{TabId, TerminalManager, TerminalManagerEvent};

use coding::agent_login::{self, LoginProgress, LoginTarget};
use coding::CodingAgent;

use crate::agent_login_outcome::{EnterCode, LoginOutcome};
use crate::coding_flow::CodingHub;
use crate::native_dialog;
use crate::queries;

/// Grid poll cadence — the sign-in URL lands within a second or two of the
/// spawn, and a quarter-second read of a 120×36 grid is nothing next to the
/// PTY itself.
const POLL: Duration = Duration::from_millis(250);

/// How long a remote login may run without ever showing a URL before the
/// command is failed back to the requester.
const REMOTE_URL_TIMEOUT: Duration = Duration::from_secs(600);

/// The two sentences a code command completes with — byte-identical to the
/// daemon executor (`cli::agent_login_host`); the clients show a failed
/// row's `result` verbatim.
///
/// EXP-940: the success one is the phase the requester is IN, not a sentence
/// about a machine finishing something. It is the ×4 `SIGNING_IN` string
/// (web `agent-login-dialog.tsx`), so a client that does print the wire text
/// prints exactly what its own spinner says.
const SIGNING_IN: &str = "Signing in…";
const CODE_ENTERED: &str = SIGNING_IN;
const NO_LOGIN_WAITING: &str = "No sign-in is waiting for a code on this machine.";

/// EXP-765: the login tabs currently open, by agent id — where a handed-back
/// authorization code gets typed. Local and remote logins both register (a
/// code can be sent to a machine whose own user opened the tab); a run's
/// finish removes ITS entry only, so a login started after it keeps its own.
#[derive(Default)]
struct LoginTabs {
    by_agent: HashMap<String, (Entity<TerminalManager>, TabId)>,
}

impl gpui::Global for LoginTabs {}

/// EXP-765: run an `agent_login_code` device command — type the code into
/// the agent's waiting login tab. Completes at once either way: the login's
/// own run reports the exit, and a claimed id never comes back here.
pub(crate) fn enter_remote_code(command: api::devices::PendingCommand, cx: &mut App) {
    let agent = command.payload["agent"].as_str().unwrap_or_default().to_string();
    let code = command.payload["code"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    let target = cx
        .default_global::<LoginTabs>()
        .by_agent
        .get(&agent)
        .cloned();
    let typed = match target {
        Some((manager, tab)) if !code.is_empty() => {
            match tab_state(&manager, tab, cx) {
                Some((_, true)) => {
                    write_input(&manager, tab, format!("{code}\r").as_bytes(), cx);
                    true
                }
                _ => false,
            }
        }
        _ => false,
    };
    if typed {
        complete(&command.id, true, CODE_ENTERED.to_string(), cx);
    } else {
        complete(&command.id, false, NO_LOGIN_WAITING.to_string(), cx);
    }
    // The claim is deliberately NOT released: `complete` lands on the
    // background executor, and a beat in between would otherwise type the
    // same code a second time. The set holds one id per code ever entered.
}

/// The remote half of a login: which `device_commands` row to answer.
#[derive(Clone)]
struct RemoteLogin {
    command_id: String,
    /// Flipped once the row has been completed — the exit hook must not
    /// answer a command the poll already answered.
    published: Arc<AtomicBool>,
}

/// Open a login tab for `agent` on this machine. `intended` = the profile
/// the person clicked "Sign in" on (`None` = "Add account"); it only decides
/// whether a login that lands on ANOTHER known email warns.
pub(crate) fn open_login_tab(agent: CodingAgent, intended: Option<String>, cx: &mut App) {
    let target = match intended {
        Some(id) => LoginTarget::Profile(id),
        None => LoginTarget::Add,
    };
    start(agent, target, None, cx);
}

/// Import `agent`'s AMBIENT login on this machine (the doctor's Import pill,
/// after its confirm).
pub(crate) fn import_ambient_login(agent: CodingAgent, cx: &mut App) {
    start(agent, LoginTarget::Import, None, cx);
}

/// EXP-484 (D): run an `agent_login` device command. The payload was already
/// validated and claimed by [`crate::device_sync`]; this opens the same tab
/// the local button does and answers the command the moment a URL is up (an
/// import answers with its outcome sentence).
pub(crate) fn start_remote_login(command: api::devices::PendingCommand, cx: &mut App) {
    // The beat already refused a malformed payload; this is the belt.
    let request = match agent_login::parse_login_payload(&command.payload) {
        Ok(request) => request,
        Err(message) => {
            complete(&command.id, false, message, cx);
            crate::device_sync::release_login(&command.id, cx);
            return;
        }
    };
    start(
        request.agent,
        request.target,
        Some(RemoteLogin {
            command_id: command.id,
            published: Arc::new(AtomicBool::new(false)),
        }),
        cx,
    );
}

/// Answer a remote command that never got a run (or ran no tab) and drop its
/// claim.
fn answer_unstarted(remote: Option<&RemoteLogin>, ok: bool, message: String, cx: &mut App) {
    if let Some(remote) = remote {
        complete(&remote.command_id, ok, message, cx);
        crate::device_sync::release_login(&remote.command_id, cx);
    }
}

/// What a committed sign-in or import says here: the duplicate warning, or
/// the plain line.
fn commit_toast(commit: &agent_login::LoginCommit, fallback: String) -> Toast {
    match commit.duplicate_warning() {
        Some(warning) => Toast::warning(warning),
        None => Toast::info(fallback),
    }
}

/// The one sequence: (import, done) | codex fetch → staging dir → login tab
/// → grid watch → commit on a clean exit → re-probe. Deferred, because the
/// caller is typically inside its own window's update and
/// [`crate::coding_flow::any_terminal_dock`] has to update windows to find
/// the dock.
fn start(agent: CodingAgent, target: LoginTarget, remote: Option<RemoteLogin>, cx: &mut App) {
    let settings = CodingHub::global(cx).read(cx).settings.clone();
    let data_dir = crate::coding_flow::coding_data_dir(cx);
    // EXP-695: a REMOTE sign-in must not pop a browser on this machine —
    // the requester gets the link through the command result instead.
    let mut plan = agent_login::login_plan(&settings, agent, remote.is_some());
    cx.spawn(async move |cx| {
        if target == LoginTarget::Import {
            let imported = {
                let (settings, data_dir) = (settings.clone(), data_dir.clone());
                cx.background_executor()
                    .spawn(async move { agent_login::import_ambient(&settings, &data_dir, agent) })
                    .await
            };
            let _ = cx.update(|cx| {
                match imported {
                    Ok(commit) => {
                        let text = commit.import_result_text();
                        notify(commit_toast(&commit, text.clone()), cx);
                        answer_unstarted(remote.as_ref(), true, text, cx);
                    }
                    Err(message) => {
                        notify(Toast::error(message.clone()), cx);
                        answer_unstarted(remote.as_ref(), false, message, cx);
                    }
                }
                let hub = CodingHub::global(cx);
                CodingHub::refresh_agent_usage(&hub, cx);
            });
            return;
        }
        // EXP-1232: a managed Codex sign-in fetches the pinned build first
        // (one toast; the doctor row reads Downloading… meanwhile). A failed
        // fetch answers the requester and starts nothing.
        if agent == CodingAgent::Codex
            && settings.codex_is_managed()
            && !coding::managed_codex::installed(&data_dir)
        {
            let _ = cx.update(|cx| notify(Toast::info("Downloading Codex…".to_string()), cx));
            let fetch_dir = data_dir.clone();
            let fetched = cx
                .background_executor()
                .spawn(async move { coding::managed_codex::ensure(&fetch_dir, &mut |_, _| {}) })
                .await;
            let _ = cx.update(|cx| {
                let hub = CodingHub::global(cx);
                CodingHub::refresh_doctor(&hub, cx);
            });
            if let Err(message) = fetched {
                log::warn!("[agent-login] codex fetch failed: {message}");
                let _ = cx.update(|cx| {
                    notify(Toast::error(message.clone()), cx);
                    answer_unstarted(remote.as_ref(), false, message, cx);
                });
                return;
            }
        }
        let staged = {
            let data_dir = data_dir.clone();
            cx.background_executor()
                .spawn(async move { agent_login::begin_login(&data_dir, agent) })
                .await
        };
        let (staging, env) = match staged {
            Ok(staged) => staged,
            Err(message) => {
                log::warn!("[agent-login] {agent:?} staging refused: {message}");
                let _ = cx.update(|cx| {
                    notify(Toast::error(message.clone()), cx);
                    answer_unstarted(remote.as_ref(), false, message, cx);
                });
                return;
            }
        };
        plan.spawn.env.push(env);
        let run = LoginRun {
            agent,
            settings,
            staging,
            intended: target.intended().map(str::to_string),
            remote,
            finished: AtomicBool::new(false),
            tab: OnceLock::new(),
        };
        let _ = cx.update(|cx| spawn_login_tab(plan, run, cx));
    })
    .detach();
}

/// One in-flight login run. Its FINISH edge fires exactly once, from
/// whichever of three places reaches it first:
///
/// * the child's exit hook (the CLI finished, or was killed);
/// * a hand-CLOSED tab — [`TerminalManager::close_tab`] removes the tab
///   before it shuts the session down, so the tab's one-shot exit hook goes
///   with it and never fires. Without the `TabClosed` watch a remote
///   `agent_login` would stay pending forever, holding its in-flight claim,
///   and a local login would never re-probe;
/// * the watch loop noticing the tab is gone (belt and braces — the window
///   can be released around it).
struct LoginRun {
    agent: CodingAgent,
    settings: coding::Settings,
    /// The fresh dir the CLI signs in inside; committed (or dropped) on finish.
    staging: PathBuf,
    /// The profile the person clicked "Sign in" on (`None` = Add account).
    intended: Option<String>,
    remote: Option<RemoteLogin>,
    finished: AtomicBool,
    /// EXP-765: the tab this run opened — set once it exists, so `finish`
    /// can drop exactly this entry from [`LoginTabs`].
    tab: OnceLock<TabId>,
}

impl LoginRun {
    /// The run ended: answer an unanswered remote command, release the
    /// in-flight claim, then — on a CLEAN exit — COMMIT the login by email
    /// (the commit drops the landed login's cached usage, so it is read
    /// afresh) and re-probe so the row and the Tools pane tell the truth
    /// again. Anything else drops the staging dir and touches nothing.
    fn finish(&self, clean: bool, cx: &mut App) {
        if self.finished.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(tab) = self.tab.get().copied() {
            let tabs = cx.default_global::<LoginTabs>();
            if tabs.by_agent.get(self.agent.id()).is_some_and(|(_, open)| *open == tab) {
                tabs.by_agent.remove(self.agent.id());
            }
        }
        self.answer_remote("The sign-in ended before a link appeared.", cx);
        let agent = self.agent;
        let staging = self.staging.clone();
        if !clean {
            cx.background_executor()
                .spawn(async move { agent_login::abandon_login(agent, &staging) })
                .detach();
            return;
        }
        let settings = self.settings.clone();
        let intended = self.intended.clone();
        let data_dir = crate::coding_flow::coding_data_dir(cx);
        cx.spawn(async move |cx| {
            let committed = cx
                .background_executor()
                .spawn(async move {
                    agent_login::commit_login(&settings, &data_dir, agent, &staging, intended.as_deref())
                })
                .await;
            let _ = cx.update(|cx| {
                match committed {
                    Ok(commit) => notify(
                        commit_toast(&commit, format!("{} sign-in finished — rechecking.", agent.label())),
                        cx,
                    ),
                    Err(message) => notify(Toast::error(message), cx),
                }
                // The re-probe carries the beat with it: the collector's input
                // IS the doctor report, so `refresh_agent_usage` nudges the
                // beat only once the fresh report has landed.
                let hub = CodingHub::global(cx);
                CodingHub::refresh_agent_usage(&hub, cx);
            });
        })
        .detach();
    }

    /// The run never got off the ground (no window, a failed spawn): answer
    /// the requester with WHY and release the claim, without the re-probe
    /// and the "finished" notification an actual run earns.
    fn abandon(&self, message: &str, cx: &mut App) {
        if self.finished.swap(true, Ordering::SeqCst) {
            return;
        }
        self.answer_remote(message, cx);
        let (agent, staging) = (self.agent, self.staging.clone());
        cx.background_executor()
            .spawn(async move { agent_login::abandon_login(agent, &staging) })
            .detach();
    }

    /// Complete the device command with `message` when the watch loop never
    /// published an answer, then drop the claim either way.
    fn answer_remote(&self, message: &str, cx: &mut App) {
        let Some(remote) = self.remote.as_ref() else {
            return;
        };
        if remote
            .published
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            complete(&remote.command_id, false, message.to_string(), cx);
        }
        crate::device_sync::release_login(&remote.command_id, cx);
    }
}

/// Open the tab in whichever window owns a terminal dock, then start the
/// grid watch.
fn spawn_login_tab(plan: coding::LoginPlan, run: LoginRun, cx: &mut App) {
    let agent = run.agent;
    let run = Arc::new(run);
    let Some(handle) = crate::coding_flow::any_terminal_dock(cx) else {
        notify(
            Toast::error(
                "Open the main window to sign in to an agent.",
            ),
            cx,
        );
        run.abandon("This machine has no window to run the sign-in in.", cx);
        return;
    };
    let opened = handle.update(cx, |_, window, cx| {
        let panel = crate::coding_flow::window_session_bar(window, cx)?;
        let manager = panel.read(cx).manager().clone();
        // EXP-695: a login tab has served its purpose the moment the CLI
        // exits CLEANLY — close it instead of leaving a "finished" strip to
        // collect. A failed exit keeps the tab so the error stays readable.
        // Deferred, because the hook fires inside the manager's own update.
        let exit_run = Arc::clone(&run);
        let exit_manager = manager.clone();
        let exit_hook: terminal::tab::ExitHook = Box::new(move |tab, exit, cx| {
            exit_run.finish(exit.success, cx);
            if exit.success {
                let manager = exit_manager.clone();
                cx.defer(move |cx| {
                    manager.update(cx, |manager, cx| manager.close_tab(tab, cx));
                });
            }
        });
        let tab = panel
            .update(cx, |panel, cx| {
                panel.launch_agent_login(agent, &plan, Some(exit_hook), cx)
            })
            .ok()?;
        Some((manager, tab))
    });
    match opened {
        Ok(Some((manager, tab))) => {
            // EXP-765: this is where a handed-back code gets typed while the
            // tab lives. Latest wins per agent (a second login supersedes).
            let _ = run.tab.set(tab);
            cx.default_global::<LoginTabs>()
                .by_agent
                .insert(agent.id().to_string(), (manager.clone(), tab));
            // Closing the tab by hand never fires the exit hook — watch the
            // manager for it (the `LocalSessions::insert` idiom). Detached:
            // the subscription lives with the manager, and the run's own
            // once-guard makes a late edge a no-op.
            let closed_run = Arc::clone(&run);
            cx.subscribe(&manager, move |_, event: &TerminalManagerEvent, cx| {
                if *event == TerminalManagerEvent::TabClosed(tab) {
                    // Closed by hand before the CLI exited: nothing to commit.
                    closed_run.finish(false, cx);
                }
            })
            .detach();
            watch_login(manager, tab, run, cx);
        }
        _ => {
            notify(
                Toast::error(format!(
                    "Could not start the {} sign-in.",
                    agent.label()
                )),
                cx,
            );
            run.abandon("The machine could not start the sign-in.", cx);
        }
    }
}

/// The 250ms foreground grid watch: for a remote run, publishes the sign-in
/// URL the instant the driver recognizes one.
fn watch_login(
    manager: Entity<TerminalManager>,
    tab: TabId,
    run: Arc<LoginRun>,
    cx: &mut App,
) {
    if run.remote.is_none() {
        return; // nothing to watch: a local claude/codex login
    }
    let agent = run.agent;
    cx.spawn(async move |cx| {
        let started = std::time::Instant::now();
        // The defensive Enter is written ONCE: the picker stays on screen
        // for several polls, and one `\r` per 250ms tick would walk the CLI
        // through every prompt after it.
        let mut picker_answered = false;
        loop {
            cx.background_executor().timer(POLL).await;
            let Some((lines, running)) = cx.update(|cx| tab_state(&manager, tab, cx)) else {
                // The tab is gone (closed by hand, or its window released) —
                // the `TabClosed` watch normally beats us here; finishing
                // again is a no-op.
                let _ = cx.update(|cx| run.finish(false, cx));
                return;
            };
            if !running {
                // Exited but kept open: the exit hook normally finished the
                // run already — stop polling instead of burning the whole
                // 10-minute budget on a dead grid. Should this edge win, it
                // reads the same exit the hook would have.
                let clean = cx.update(|cx| tab_exit_code(&manager, tab, cx) == Some(0));
                let _ = cx.update(|cx| run.finish(clean, cx));
                return;
            }
            if let Some(remote) = run.remote.as_ref() {
                if remote.published.load(Ordering::SeqCst) {
                    return;
                }
                match steer::agent_login_driver::observe_login_screen(agent.id(), &lines) {
                    steer::agent_login_driver::LoginObservation::Url { url, code } => {
                        if remote
                            .published
                            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok()
                        {
                            let progress = LoginProgress::url(agent, url, code);
                            let _ = cx.update(|cx| {
                                complete(&remote.command_id, true, progress.to_result_text(), cx)
                            });
                        }
                        return;
                    }
                    // Defensive: `--claudeai` skips the method picker, but a
                    // CLI that shows one anyway is one Enter from the URL.
                    steer::agent_login_driver::LoginObservation::MethodPicker => {
                        if !picker_answered {
                            picker_answered = true;
                            let _ = cx.update(|cx| write_input(&manager, tab, b"\r", cx));
                        }
                    }
                    steer::agent_login_driver::LoginObservation::Failed(message) => {
                        if remote
                            .published
                            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok()
                        {
                            let _ = cx
                                .update(|cx| complete(&remote.command_id, false, message, cx));
                        }
                        return;
                    }
                    steer::agent_login_driver::LoginObservation::Nothing => {}
                }
                if started.elapsed() >= REMOTE_URL_TIMEOUT {
                    if remote
                        .published
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        let _ = cx.update(|cx| {
                            complete(
                                &remote.command_id,
                                false,
                                "The sign-in did not produce a link in time.".to_string(),
                                cx,
                            )
                        });
                    }
                    return;
                }
            }
        }
    })
    .detach();
}

/// The tab's grid plus whether its child is still running; `None` once the
/// tab is gone.
fn tab_state(
    manager: &Entity<TerminalManager>,
    tab: TabId,
    cx: &App,
) -> Option<(Vec<String>, bool)> {
    let tab = manager.read(cx).tab(tab)?;
    let running = tab.is_running();
    let lines = tab.view.read(cx).session().borrow().screen_lines();
    Some((lines, running))
}

/// The tab's captured exit code, `None` while it runs (or once it is gone).
fn tab_exit_code(manager: &Entity<TerminalManager>, tab: TabId, cx: &App) -> Option<i32> {
    manager.read(cx).tab(tab)?.exit_code()
}

fn write_input(manager: &Entity<TerminalManager>, tab: TabId, bytes: &[u8], cx: &App) {
    if let Some(view) = manager.read(cx).tab(tab).map(|tab| tab.view.clone()) {
        view.read(cx).session().borrow().write(bytes);
    }
}

/// `devices.completeCommand` on the background executor (the only writer of
/// a command's `result`).
fn complete(command_id: &str, ok: bool, message: String, cx: &mut App) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    let command_id = command_id.to_string();
    cx.background_executor()
        .spawn(async move {
            if let Err(err) =
                api::devices::complete_command(&trpc, &command_id, ok, Some(&message))
            {
                log::debug!("[agent-login] completeCommand failed: {err}");
            }
        })
        .detach();
}

fn notify(toast: Toast, cx: &mut App) {
    crate::toast::show_in_active_window(toast, cx);
}

// ---------------------------------------------------------------------------
// EXP-862: "+ Add account" and the sign-in dialog
// ---------------------------------------------------------------------------
//
// One entry point for every sign-in a client can start: the Accounts header's
// "+ Add account", an account row's "+" chip ("Sign in on <device>") and a
// chip whose login is signed out all end in [`sign_in_on_device`]. On THIS
// machine that opens the CLI's login tab; on another of mine it queues the
// `agent_login` command and opens [`open_login_dialog`] — a title and ONE
// status line, which closes itself once the machine reports the login.

/// One machine a sign-in can be queued on right now (the device the Add
/// account row was pressed on, EXP-909):
/// one of MINE, online, on a build that runs `agent_login`, with at least one
/// agent installed there.
#[derive(Clone)]
pub(crate) struct LoginDevice {
    pub device_id: String,
    pub label: SharedString,
    /// This very install — the login runs in a terminal tab right here
    /// instead of riding a heartbeat.
    pub own: bool,
    /// The agents installed there, runnable or signed out, in contract order.
    pub agents: Vec<CodingAgent>,
}

/// The machines a sign-in can be queued on, newest rules first: MINE, online,
/// advertising `agent-login`, running `agent` when one is named, and not among
/// `exclude` (the machines already holding the account — the per-account "+").
pub(crate) fn add_account_devices(
    agent: Option<CodingAgent>,
    exclude: &[String],
    cx: &mut App,
) -> Vec<LoginDevice> {
    // Before the store is borrowed: resolving it caches a global.
    let own_device_id = queries::own_device_id(cx);
    let Some(store) = sync::Store::try_global(cx) else {
        return Vec::new();
    };
    let Some(me) = queries::active_account(cx) else {
        return Vec::new();
    };
    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut out: Vec<LoginDevice> = Vec::new();
    for row in store.collections().devices.read(cx).iter() {
        if row.user_id.as_deref() != Some(me.user_id.as_str()) {
            continue;
        }
        let device_id = row.device_id.clone().unwrap_or_default();
        if device_id.is_empty() || exclude.iter().any(|id| id == &device_id) {
            continue;
        }
        if !crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms) {
            continue;
        }
        if !row.cap_ids().iter().any(|cap| cap == "agent-login") {
            continue;
        }
        let installed = row.agent_ids();
        let unauthed = row.unauthed_agent_ids();
        let agents: Vec<CodingAgent> = CodingAgent::ALL
            .into_iter()
            .filter(|known| {
                installed.iter().any(|id| id == known.id())
                    || unauthed.iter().any(|id| id == known.id())
            })
            .collect();
        if agents.is_empty() || agent.is_some_and(|agent| !agents.contains(&agent)) {
            continue;
        }
        let label = row.label.clone().unwrap_or_default();
        out.push(LoginDevice {
            own: device_id == own_device_id,
            label: SharedString::from(if label.trim().is_empty() {
                device_id.clone()
            } else {
                label
            }),
            agents,
            device_id,
        });
    }
    out.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    out
}

/// EXP-862 — start a sign-in for `agent` on `device`, wherever it is: the CLI's
/// own login tab on THIS machine, an `agent_login` command plus the status
/// dialog on another of mine. The ONE entry point every chip, menu and dialog
/// uses, so "Sign in" means the same thing everywhere. `intended` = the row
/// the person clicked (`None` = "Add account"); the login still lands on the
/// profile of the email it signs in as.
pub(crate) fn sign_in_on_device(
    device_id: String,
    device_label: SharedString,
    own: bool,
    agent: CodingAgent,
    intended: Option<String>,
    window: &mut Window,
    cx: &mut App,
) {
    let intended = intended
        .map(|id| id.trim().to_string())
        .filter(|id| !coding::agent_profiles::is_unpinned(Some(id)));
    if own {
        open_login_tab(agent, intended, cx);
        return;
    }
    open_login_dialog(device_id, device_label, agent, intended, window, cx);
}

/// A remote sign-in's landing (accounts contract E): the dialog captures each
/// profile's `lastLoginAt` when it queues the command, and the login has
/// landed on the first row whose stamp is set and MOVED (or a new row that
/// carries one). `None` = nothing landed yet.
fn landed_login<'a>(
    baseline: &BTreeMap<String, Option<String>>,
    rows: &'a [coding::AgentProfileEntry],
) -> Option<&'a coding::AgentProfileEntry> {
    rows.iter().find(|row| {
        row.last_login_at.is_some()
            && baseline.get(&row.id).is_none_or(|before| *before != row.last_login_at)
    })
}

/// The already-added warning a landing earns: the row EXISTED before the
/// sign-in and is not the one the person aimed at (or they aimed at none).
fn landing_warning(
    baseline: &BTreeMap<String, Option<String>>,
    landed: &coding::AgentProfileEntry,
    intended: Option<&str>,
) -> Option<String> {
    let existed = baseline.contains_key(&landed.id);
    let aimed_here = intended == Some(landed.id.as_str());
    (existed && !aimed_here)
        .then(|| landed.email.as_deref().map(agent_login::already_added))
        .flatten()
}

/// What the device reports for `agent`'s logins, by profile id → its
/// `lastLoginAt` (the baseline [`landed_login`] compares against).
fn login_stamps(rows: &[coding::AgentProfileEntry]) -> BTreeMap<String, Option<String>> {
    rows.iter()
        .map(|row| (row.id.clone(), row.last_login_at.clone()))
        .collect()
}

/// How often the sign-in dialog asks the server what the machine answered.
const LOGIN_DIALOG_POLL: Duration = Duration::from_secs(2);

/// EXP-940 — the two lines the END of a sign-in says, byte-identical ×4 (web
/// `agent-login-dialog.tsx` `SIGNED_IN` / `SIGN_IN_TIMED_OUT`).
const SIGNED_IN: &str = "Signed in";
const SIGN_IN_TIMED_OUT: &str = "The machine did not confirm the sign-in.";

/// EXP-940 — how long the success notice stays up before the dialog closes
/// itself (web `SUCCESS_LINGER_MS`).
const SUCCESS_LINGER: Duration = Duration::from_millis(1_500);

/// EXP-940 — how long a queued login waits for the machine to hand back its
/// sign-in link before the dialog gives up and offers a retry. A probe plus a
/// heartbeat is ~30s, so this is three beats of headroom (web
/// `SIGN_IN_TIMEOUT_MS`). It bounds the WAIT only: once the link is up the
/// person is in the loop, and a browser round-trip has no clock on it.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(120);

/// The ONE status line the sign-in dialog shows.
#[derive(Clone, Debug, PartialEq, Eq)]
enum LoginDialogState {
    /// The command is on its way to the server.
    Queueing,
    /// Queued — the machine has not handed anything back yet.
    Waiting,
    /// The CLI's sign-in link (plus codex's device code) — rendered by the
    /// shared [`LoginOutcome`] in [`LoginDialogView::outcome`], claude's code
    /// field included (EXP-1000).
    Link { url: String, code: Option<String> },
    /// EXP-1000: claude's code went back to the machine (web `signing` /
    /// `codePending`). The machine types it, finishes the login on its own and
    /// reports it on its next heartbeat, which is what lands [`Self::SignedIn`].
    SigningIn,
    /// EXP-940: the machine reported the login. The dialog SAYS so for a beat
    /// and then closes itself, instead of vanishing mid-sentence.
    SignedIn,
    Failed(SharedString),
}

impl LoginDialogState {
    /// Still waiting on the machine's first answer — the phase
    /// [`SIGN_IN_TIMEOUT`] bounds.
    fn waiting(&self) -> bool {
        matches!(self, Self::Queueing | Self::Waiting)
    }
}

/// EXP-862 — "Sign in to <agent>" as its own dialog: a title and ONE status
/// line (waiting → the link → an error), which CLOSES ITSELF the moment the
/// machine reports the login on its synced row. Web parity
/// (`agent-login-dialog.tsx`), minus the stacked pending/result/error blocks
/// that used to say the same thing three times.
///
/// EXP-1000: the link line IS the shared [`LoginOutcome`] — the same block
/// the web, iOS and Android sheets render — so claude's "Code from the
/// browser" field is here too. The EXP-862 rewrite drew the link inline and
/// dropped that field, which left a claude re-login on another machine with a
/// link and nowhere to type the code the browser showed.
pub(crate) fn open_login_dialog(
    device_id: String,
    device_label: SharedString,
    agent: CodingAgent,
    intended: Option<String>,
    window: &mut Window,
    cx: &mut App,
) {
    let title = if intended.is_none() {
        format!("Add a {} account", agent.label())
    } else {
        format!("Sign in to {}", agent.label())
    };
    let spec = native_dialog::DialogSpec::new(title, size(px(460.), px(180.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| {
            LoginDialogView::new(device_id, device_label, agent, intended, window, cx)
        });
        native_dialog::DialogContent::new(view)
    });
}

struct LoginDialogView {
    device_id: String,
    device_label: SharedString,
    agent: CodingAgent,
    /// The profile the person aimed at (`None` = Add account) — kept so "Try
    /// again" re-queues the SAME login, and read by the duplicate warning.
    intended: Option<String>,
    state: LoginDialogState,
    /// EXP-940: which attempt the phases belong to. A retry bumps it, so the
    /// abandoned attempt's poll and its timeout land on a stale generation and
    /// write nothing.
    attempt: u64,
    /// The live attempt's 2 s poll of the command row. HELD, not detached:
    /// assigning the next attempt's task drops (cancels) this one, so a
    /// "Try again" never leaves a poller behind (release review R5); the
    /// loop also bails on its own once the attempt moved on.
    poll: Option<gpui::Task<()>>,
    /// Each profile's `lastLoginAt` when the command was queued. A stamp
    /// MOVING is the success this dialog waits for ([`landed_login`]).
    baseline: BTreeMap<String, Option<String>>,
    /// EXP-1000: the published link, rendered by the SHARED outcome block
    /// (link · device code · claude's code field · caption). Built when the
    /// login's command lands its URL, dropped by a retry.
    outcome: Option<Entity<LoginOutcome>>,
    _subscriptions: Vec<Subscription>,
}

impl LoginDialogView {
    fn new(
        device_id: String,
        device_label: SharedString,
        agent: CodingAgent,
        intended: Option<String>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let devices = sync::Store::global(cx).collections().devices.clone();
        let baseline = login_stamps(&device_logins(&device_id, agent, cx));
        // EXP-940: the landing no longer makes the dialog VANISH mid-sentence.
        // It turns into "Signed in", and the dialog closes a beat later — so
        // the flow ends with an answer.
        let subscriptions = vec![cx.observe_in(
            &devices,
            window,
            |this: &mut Self, _, window, cx| {
                if this.state == LoginDialogState::SignedIn {
                    return;
                }
                let rows = device_logins(&this.device_id, this.agent, cx);
                if let Some(landed) = landed_login(&this.baseline, &rows) {
                    // Signed in as an email the machine already had: that
                    // login was refreshed, the one aimed at was not.
                    if let Some(warning) =
                        landing_warning(&this.baseline, landed, this.intended.as_deref())
                    {
                        notify(Toast::warning(warning), cx);
                    }
                    this.state = LoginDialogState::SignedIn;
                    // Nothing this attempt still has in flight may speak now.
                    this.attempt = this.attempt.wrapping_add(1);
                    cx.notify();
                    let handle = window.window_handle();
                    cx.spawn(async move |_, cx| {
                        cx.background_executor().timer(SUCCESS_LINGER).await;
                        let _ = cx.update(|cx| {
                            let _ = handle.update(cx, |_, window, cx| {
                                native_dialog::close_dialog_window(window, cx);
                            });
                        });
                    })
                    .detach();
                    return;
                }
                cx.notify();
            },
        )];
        let mut view = Self {
            device_id,
            device_label,
            agent,
            intended,
            state: LoginDialogState::Queueing,
            attempt: 0,
            poll: None,
            baseline,
            outcome: None,
            _subscriptions: subscriptions,
        };
        view.request(window, cx);
        view
    }

    /// The sign-in request itself: queue the `agent_login` command, poll the
    /// row until the machine answers, and bound the wait. The dialog OPENING
    /// runs it (web parity), and EXP-940's "Try again" runs it again.
    ///
    /// Window-bound (`spawn_in`): the link's arrival builds the shared
    /// [`LoginOutcome`], whose code field is an `InputState` and needs the
    /// window it lives in.
    fn request(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.attempt = self.attempt.wrapping_add(1);
        let attempt = self.attempt;
        self.state = LoginDialogState::Queueing;
        // A fresh login supersedes whatever the last one handed back (web
        // `queueLogin` drops the code round trip's state the same way).
        self.outcome = None;
        // The baseline is re-read: a retry must not treat the report the LAST
        // attempt already moved as this one's success.
        self.baseline = login_stamps(&device_logins(&self.device_id, self.agent, cx));
        cx.notify();
        if queries::trpc_client(cx).is_none() {
            self.state = LoginDialogState::Failed("Not signed in.".into());
            return;
        }
        let device_id = self.device_id.clone();
        let agent = self.agent;
        let intended = self.intended.clone();
        self.poll = Some(cx.spawn_in(window, async move |this, cx| {
            // A fresh client per call: `TrpcClient` is not shareable, and
            // building one is a token-provider lookup, not a connection.
            let Ok(Some(trpc)) = this.update(cx, |_, cx| queries::trpc_client(cx)) else {
                return;
            };
            let queued = cx
                .background_executor()
                .spawn({
                    let device_id = device_id.clone();
                    async move {
                        api::devices::create_agent_login_command(
                            &trpc,
                            &device_id,
                            agent.id(),
                            intended.as_deref(),
                            false,
                        )
                        .map(|created| created.id)
                    }
                })
                .await;
            let command_id = match queued {
                Ok(id) => id,
                Err(err) => {
                    let _ = this.update(cx, |this, cx| {
                        this.settle(attempt, LoginDialogState::Failed(err.user_message().into()), cx)
                    });
                    return;
                }
            };
            if this
                .update(cx, |this, cx| this.settle(attempt, LoginDialogState::Waiting, cx))
                .is_err()
            {
                return;
            }
            loop {
                cx.background_executor().timer(LOGIN_DIALOG_POLL).await;
                // A retry (or the landing) moved the attempt on: this poll
                // belongs to a dead generation and stops here.
                let Ok(live) = this.read_with(cx, |this, _| this.attempt == attempt) else {
                    return;
                };
                if !live {
                    return;
                }
                let Ok(Some(trpc)) = this.update(cx, |_, cx| queries::trpc_client(cx)) else {
                    return;
                };
                let row = cx
                    .background_executor()
                    .spawn({
                        let command_id = command_id.clone();
                        async move { api::devices::get_command(&trpc, &command_id) }
                    })
                    .await;
                let Ok(row) = row else {
                    continue; // transient — the next tick asks again
                };
                if !row.is_terminal() {
                    continue;
                }
                let state = login_dialog_result(&row);
                let _ = this.update_in(cx, |this, window, cx| {
                    let link = match &state {
                        LoginDialogState::Link { url, code } => Some((url.clone(), code.clone())),
                        _ => None,
                    };
                    if this.settle(attempt, state, cx) {
                        if let Some((url, code)) = link {
                            this.show_link(url, code, window, cx);
                        }
                    }
                });
                return;
            }
        }));
        // EXP-940: a wait that never lands is a failure, not progress. The
        // machine answers its commands on the beat, so silence past the bound
        // gets the short error and a retry instead of an endless spinner.
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SIGN_IN_TIMEOUT).await;
            let _ = this.update(cx, |this, cx| {
                if this.attempt == attempt && this.state.waiting() {
                    this.state = LoginDialogState::Failed(SIGN_IN_TIMED_OUT.into());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Write a phase that belongs to the LIVE attempt; an abandoned one (a
    /// retry ran, or the login already landed) writes nothing. Returns
    /// whether it wrote.
    fn settle(&mut self, attempt: u64, state: LoginDialogState, cx: &mut gpui::Context<Self>) -> bool {
        if self.attempt != attempt {
            return false;
        }
        self.state = state;
        cx.notify();
        true
    }

    /// EXP-1000: the machine handed back its link — build the SHARED outcome
    /// block for it. Its code field calls back into [`Self::enter_code`]; the
    /// callback runs inside the outcome's own update, so it only touches this
    /// view (never the outcome entity, which is leased at that moment).
    fn show_link(
        &mut self,
        url: String,
        code: Option<String>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let dialog = cx.entity().downgrade();
        let on_enter_code: EnterCode = Rc::new(move |code, _window, cx| {
            let _ = dialog.update(cx, |this, cx| this.enter_code(code, cx));
        });
        let agent = self.agent;
        self.outcome = Some(cx.new(|cx| {
            LoginOutcome::new(agent, url, code, on_enter_code, window, cx)
        }));
        cx.notify();
    }

    /// EXP-1000 (EXP-765's round trip, in this dialog): hand the code claude's
    /// browser page showed back to the machine as an `agent_login_code`
    /// command. The machine types it into its waiting login tab and completes
    /// the command at once; the dialog spins on "Signing in…" until the
    /// machine's re-probe lands the login on its synced row (→ "Signed in"),
    /// a refusal ("No sign-in is waiting…") ends in the error + "Try again",
    /// and silence past [`SIGN_IN_TIMEOUT`] is the same failure as a link
    /// that never came (web `signing` + `SIGN_IN_TIMEOUT_MS`).
    fn enter_code(&mut self, code: String, cx: &mut gpui::Context<Self>) {
        if !matches!(self.state, LoginDialogState::Link { .. }) {
            return;
        }
        // A new generation: the link poll is long done (the row was terminal),
        // and the link wait's timeout must not count this phase as its own.
        self.attempt = self.attempt.wrapping_add(1);
        let attempt = self.attempt;
        self.state = LoginDialogState::SigningIn;
        cx.notify();
        if queries::trpc_client(cx).is_none() {
            self.state = LoginDialogState::Failed("Not signed in.".into());
            cx.notify();
            return;
        }
        let device_id = self.device_id.clone();
        let agent = self.agent;
        self.poll = Some(cx.spawn(async move |this, cx| {
            let Ok(Some(trpc)) = this.update(cx, |_, cx| queries::trpc_client(cx)) else {
                return;
            };
            let queued = cx
                .background_executor()
                .spawn({
                    let device_id = device_id.clone();
                    async move {
                        api::devices::create_agent_login_code_command(
                            &trpc,
                            &device_id,
                            agent.id(),
                            &code,
                        )
                    }
                })
                .await;
            let command_id = match queued {
                Ok(created) => created.id,
                Err(err) => {
                    let _ = this.update(cx, |this, cx| {
                        this.settle(attempt, LoginDialogState::Failed(err.user_message().into()), cx)
                    });
                    return;
                }
            };
            loop {
                cx.background_executor().timer(LOGIN_DIALOG_POLL).await;
                let Ok(live) = this.read_with(cx, |this, _| this.attempt == attempt) else {
                    return;
                };
                if !live {
                    return;
                }
                let Ok(Some(trpc)) = this.update(cx, |_, cx| queries::trpc_client(cx)) else {
                    return;
                };
                let row = cx
                    .background_executor()
                    .spawn({
                        let command_id = command_id.clone();
                        async move { api::devices::get_command(&trpc, &command_id) }
                    })
                    .await;
                let Ok(row) = row else {
                    continue; // transient — the next tick asks again
                };
                if !row.is_terminal() {
                    continue;
                }
                if row.status == "failed" {
                    let message = row
                        .result
                        .clone()
                        .unwrap_or_else(|| "The device reported a failure.".to_string());
                    let _ = this.update(cx, |this, cx| {
                        this.settle(attempt, LoginDialogState::Failed(message.into()), cx)
                    });
                }
                // `done` = the code went in. Nothing to say yet: the login
                // itself lands on the synced row, which flips SignedIn.
                return;
            }
        }));
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SIGN_IN_TIMEOUT).await;
            let _ = this.update(cx, |this, cx| {
                if this.attempt == attempt && this.state == LoginDialogState::SigningIn {
                    this.state = LoginDialogState::Failed(SIGN_IN_TIMED_OUT.into());
                    // EXP-1000: the generation moves on, so the
                    // `get_command` poll above stops ticking (the devices
                    // observer's landing does the same).
                    this.attempt = this.attempt.wrapping_add(1);
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

/// What a finished `agent_login` command says. A login completes EARLY, the
/// moment its sign-in URL is up (that link IS the result); the signed-in flip
/// follows on the synced row, which is what closes the dialog.
fn login_dialog_result(row: &api::devices::CommandRow) -> LoginDialogState {
    if row.status == "failed" {
        return LoginDialogState::Failed(SharedString::from(
            row.result
                .clone()
                .unwrap_or_else(|| "The device reported a failure.".to_string()),
        ));
    }
    match row.result.as_deref().and_then(LoginProgress::parse) {
        Some(progress) => match progress.url {
            Some(url) => LoginDialogState::Link {
                url,
                code: progress.code,
            },
            None => LoginDialogState::Failed(SharedString::from(
                progress
                    .message
                    .unwrap_or_else(|| "The machine handed back no sign-in link.".to_string()),
            )),
        },
        None => LoginDialogState::Failed(SharedString::from(
            row.result
                .clone()
                .unwrap_or_else(|| "The machine handed back no sign-in link.".to_string()),
        )),
    }
}

/// What the device currently reports about `agent`'s logins: its profile
/// rows (the only rows there are — no ambient login is ever one).
fn device_logins(device_id: &str, agent: CodingAgent, cx: &App) -> Vec<coding::AgentProfileEntry> {
    let Some(store) = sync::Store::try_global(cx) else {
        return Vec::new();
    };
    let devices = store.collections().devices.read(cx);
    let Some(row) = devices
        .iter()
        .find(|row| row.device_id.as_deref() == Some(device_id))
    else {
        return Vec::new();
    };
    crate::device_settings::parse_agent_map::<coding::AgentAccount>(row.agent_accounts.as_ref())
        .remove(agent.id())
        .map(|account| account.profiles)
        .unwrap_or_default()
}

impl Render for LoginDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let danger = theme.danger;
        let foreground = theme.foreground;
        let line = match &self.state {
            // EXP-940: the wait SPINS. A line that never moves reads the same
            // as a line that is stuck, which is what this dialog used to be.
            LoginDialogState::Queueing | LoginDialogState::Waiting => h_flex()
                .w_full()
                .items_center()
                .gap_2()
                .text_sm()
                .text_color(muted)
                .child(
                    div()
                        .flex_shrink_0()
                        .child(Spinner::new().icon(crate::icons::registry::UI_LOADING)),
                )
                .child(SharedString::from(format!(
                    "Waiting for the sign-in link from {}. Open it on any device.",
                    self.device_label
                )))
                .into_any_element(),
            // EXP-1000: the code is in; the machine finishes on its own and
            // reports the login on its next heartbeat (web `SIGNING_IN`).
            LoginDialogState::SigningIn => h_flex()
                .w_full()
                .items_center()
                .gap_2()
                .text_sm()
                .text_color(muted)
                .child(
                    div()
                        .flex_shrink_0()
                        .child(Spinner::new().icon(crate::icons::registry::UI_LOADING)),
                )
                .child(SIGNING_IN)
                .into_any_element(),
            // EXP-940: the landing SAYS so before the dialog goes.
            LoginDialogState::SignedIn => h_flex()
                .w_full()
                .items_center()
                .gap_2()
                .text_sm()
                .text_color(foreground)
                .child(
                    div().flex_shrink_0().child(
                        Icon::new(crate::icons::registry::UI_CHECK)
                            .xsmall()
                            .text_color(theme::tokens::GREEN.to_hsla()),
                    ),
                )
                .child(SIGNED_IN)
                .into_any_element(),
            // EXP-940: a failure ENDS in a control, never in a sentence the
            // reader can only close the window on.
            LoginDialogState::Failed(message) => v_flex()
                .w_full()
                .gap_2()
                .child(div().text_sm().text_color(danger).child(message.clone()))
                .child(
                    h_flex().child(
                        Button::new("agent-login-retry")
                            .outline()
                            .label("Try again")
                            .on_click(cx.listener(|this, _, window, cx| this.request(window, cx))),
                    ),
                )
                .into_any_element(),
            // EXP-1000: the SHARED outcome block — link, device code, claude's
            // code field, caption — never a link drawn by hand here again.
            LoginDialogState::Link { url, .. } => match self.outcome.clone() {
                Some(outcome) => outcome.into_any_element(),
                // Unreachable in practice (`show_link` runs with `settle`);
                // the bare link beats an empty dialog if it ever is.
                None => div()
                    .w_full()
                    .truncate()
                    .text_xs()
                    .font_family(theme::terminal::FONT_FAMILY)
                    .child(SharedString::from(url.clone()))
                    .into_any_element(),
            },
        };
        // EXP-862: a title and ONE status line. The dialog used to stack a
        // pending line, the link, an error line and a caption that all said
        // the same thing in turn; what a reader needs is the one fact that is
        // true right now.
        v_flex().w_full().gap_2().child(line)
    }
}

/// EXP-909 — "Add account" ON A DEVICE: the dialog is DEVICE-BOUND now. It is
/// only ever opened from a device's own login rows ([`crate::machines`]), so
/// the machine is already chosen and only the agent is left to pick; the
/// device picker went with the cross-device Accounts section that used to be
/// the other way in.
pub(crate) fn open_add_account_dialog_for(device_id: String, window: &mut Window, cx: &mut App) {
    let spec = native_dialog::DialogSpec::new("Add account", size(px(460.), px(280.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let device_id = device_id.clone();
        let view = cx.new(|cx| AddAccountDialogView::new(device_id, window, cx));
        native_dialog::DialogContent::new(view)
    });
}

struct AddAccountDialogView {
    devices: Vec<LoginDevice>,
    /// The machine the dialog was opened ON — fixed for its lifetime.
    device_id: String,
    agent: Option<CodingAgent>,
    _subscriptions: Vec<Subscription>,
}

impl AddAccountDialogView {
    fn new(device_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let devices = add_account_devices(None, &[], cx);
        let agent = devices
            .iter()
            .find(|device| device.device_id == device_id)
            .and_then(|device| device.agents.first().copied());
        let collection = sync::Store::global(cx).collections().devices.clone();
        // The machine going offline mid-dialog must not leave a Sign in that
        // cannot land: the list is live, and the empty state takes over.
        let subscriptions = vec![cx.observe_in(&collection, window, |this: &mut Self, _, _, cx| {
            this.devices = add_account_devices(None, &[], cx);
            this.reconcile();
            cx.notify();
        })];
        Self {
            devices,
            device_id,
            agent,
            _subscriptions: subscriptions,
        }
    }

    /// Keep the agent pick on one the bound machine still runs (the list is
    /// live). The MACHINE never moves: this dialog belongs to it.
    fn reconcile(&mut self) {
        let agents = self.selected().map(|device| device.agents.clone()).unwrap_or_default();
        if !self.agent.is_some_and(|agent| agents.contains(&agent)) {
            self.agent = agents.first().copied();
        }
    }

    fn selected(&self) -> Option<&LoginDevice> {
        self.devices
            .iter()
            .find(|device| device.device_id == self.device_id)
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let (Some(device), Some(agent)) = (self.selected().cloned(), self.agent) else {
            return;
        };
        native_dialog::close_then(window, cx, move |window, cx| {
            sign_in_on_device(
                device.device_id.clone(),
                device.label.clone(),
                device.own,
                agent,
                None,
                window,
                cx,
            );
        });
    }
}

impl AddAccountDialogView {
    /// The agent picker — the SHARED one (`coding_selects::agent_picker`), so
    /// the composer, the device editor and this dialog all pick an agent the
    /// same way.
    fn agent_picker(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let agents = self
            .selected()
            .map(|device| device.agents.clone())
            .unwrap_or_default();
        let Some(agent) = self.agent else {
            return div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("No agent installed")
                .into_any_element();
        };
        let view = cx.entity().downgrade();
        crate::coding_selects::agent_picker(
            "add-account-agent",
            &agents,
            agent,
            move |picked, _window, cx| {
                if let Some(view) = view.upgrade() {
                    view.update(cx, |this, cx| {
                        this.agent = Some(picked);
                        cx.notify();
                    });
                }
            },
            cx,
        )
    }
}

impl Render for AddAccountDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        // EXP-909: the machine is the one the row belongs to. It dropping off
        // the list mid-dialog (offline, the daemon stopped) is the only empty
        // state left.
        if self.selected().is_none() {
            // Web parity, word for word — and it keeps a way out: the window's
            // ✕ is not the only affordance in an empty dialog.
            return v_flex()
                .w_full()
                .gap_4()
                .child(div().text_sm().text_color(muted).child(
                    "None of your devices is online with an agent that can sign in remotely. \
                     Open the desktop app or start the daemon there first.",
                ))
                .child(
                    h_flex().w_full().justify_end().child(
                        Button::new("add-account-close")
                            .outline()
                            .label("Close")
                            .on_click(|_, window, cx| {
                                native_dialog::close_dialog_window(window, cx)
                            }),
                    ),
                );
        }
        let agent_picker = self.agent_picker(cx);
        let caption: SharedString = match self.selected() {
            Some(device) => format!(
                "The sign-in runs on {}. Sign in with the account you want to add.",
                device.label
            )
            .into(),
            None => "Sign in with another account on one of your devices.".into(),
        };
        v_flex()
            .w_full()
            .gap_4()
            .child(crate::surface::glass_group_rows(vec![
                crate::surface::glass_picker_row("Agent", None, agent_picker, cx),
            ]))
            .child(div().text_xs().text_color(muted).child(caption))
            .child(
                h_flex()
                    .w_full()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("add-account-cancel")
                            .outline()
                            .label("Cancel")
                            .on_click(|_, window, cx| {
                                native_dialog::close_dialog_window(window, cx)
                            }),
                    )
                    .child(
                        Button::new("add-account-continue")
                            .primary()
                            .label("Continue")
                            .disabled(self.agent.is_none())
                            .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                                this.submit(window, cx);
                            })),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-1000: what the machine hands back for each agent, as the dialog
    /// reads it. Claude's link carries NO device code — the browser shows one
    /// at the end, and the outcome block has to offer the field that returns
    /// it; codex's link carries its code and needs nothing back.
    #[test]
    fn a_claude_link_wants_its_code_back_and_a_codex_link_does_not() {
        let row = |progress: LoginProgress| api::devices::CommandRow {
            id: "cmd".into(),
            kind: "agent_login".into(),
            payload: serde_json::Value::Null,
            status: "done".into(),
            result: Some(serde_json::to_string(&progress).unwrap()),
            completed_at: None,
            created_at: None,
        };
        let claude = login_dialog_result(&row(LoginProgress::url(
            CodingAgent::Claude,
            "https://claude.ai/oauth/authorize?code=true",
            None,
        )));
        match claude {
            LoginDialogState::Link { url, code } => {
                assert!(url.contains("code=true"));
                assert!(crate::agent_login_outcome::wants_code_back(code.as_deref()));
            }
            other => panic!("claude's link did not parse: {other:?}"),
        }
        let codex = login_dialog_result(&row(LoginProgress::url(
            CodingAgent::Codex,
            "https://auth.openai.com/device",
            Some("WXYZ-ABCD".into()),
        )));
        match codex {
            LoginDialogState::Link { code, .. } => {
                assert!(!crate::agent_login_outcome::wants_code_back(code.as_deref()));
            }
            other => panic!("codex's link did not parse: {other:?}"),
        }
    }

    fn row(id: &str, stamp: Option<&str>, email: &str) -> coding::AgentProfileEntry {
        coding::AgentProfileEntry {
            id: id.into(),
            signed_in: true,
            email: Some(email.into()),
            last_login_at: stamp.map(str::to_string),
            ..coding::AgentProfileEntry::default()
        }
    }

    /// Accounts contract E: a remote sign-in lands on the row whose
    /// `lastLoginAt` moved (or a new row carrying one); nothing moving is
    /// nothing landed. A landing on a row that EXISTED and is not the one
    /// aimed at — or any existing row for "Add account" — warns.
    #[test]
    fn a_remote_sign_in_lands_where_last_login_at_moved() {
        let before = vec![
            row("aaaa0001", Some("T1"), "a@acme.test"),
            row("bbbb0002", None, "b@acme.test"),
            row("cccc0003", Some("T0"), "c@acme.test"),
        ];
        let baseline = login_stamps(&before);
        assert_eq!(landed_login(&baseline, &before), None, "nothing moved yet");

        // "Sign in" on C, signed in as A: A's stamp moves, B and C stay.
        let mut after = before.clone();
        after[0].last_login_at = Some("T2".into());
        let landed = landed_login(&baseline, &after).expect("A landed");
        assert_eq!(landed.id, "aaaa0001");
        assert_eq!(
            landing_warning(&baseline, landed, Some("cccc0003")).as_deref(),
            Some("a@acme.test was already added. Refreshed it.")
        );
        // Aimed at A: no warning. Add account onto A: a warning.
        assert_eq!(landing_warning(&baseline, landed, Some("aaaa0001")), None);
        assert!(landing_warning(&baseline, landed, None).is_some());

        // A NEW email: a new row with a stamp, never a warning.
        let mut grown = before.clone();
        grown.push(row("dddd0004", Some("T3"), "d@acme.test"));
        let landed = landed_login(&baseline, &grown).expect("the new row landed");
        assert_eq!(landed.id, "dddd0004");
        assert_eq!(landing_warning(&baseline, landed, None), None);
        // A new row with no stamp yet (an older build) is not a landing.
        let mut unstamped = before.clone();
        unstamped.push(row("eeee0005", None, "e@acme.test"));
        assert_eq!(landed_login(&baseline, &unstamped), None);
    }
}
