//! EXP-484 (C1 + D): signing an agent CLI in FROM the IDE — locally from a
//! Login button, or remotely off an `agent_login` device command.
//!
//! The product never holds, copies or refreshes a credential: this opens the
//! agent's OWN login command in a visible terminal tab and lets the CLI do
//! its thing. What the desktop adds is choreography:
//!
//! * a switch signs OUT first (`coding::agent_login::logout`); codex's logout
//!   revokes the token server-side, so a LOCAL switch confirms first (a
//!   REMOTE one was already confirmed by the requester);
//! * a REMOTE run watches the grid and completes its device command EARLY,
//!   the moment the sign-in URL (+ codex's device code) is up — the
//!   requester needs the link, not the eventual outcome. The signed-in flip
//!   itself arrives through the synced `devices` row after the exit re-probe.
//!
//! Every path ends the same way, through [`LoginRun::finish`]: the child
//! exits — OR the user closes the tab, which never fires an exit hook — and
//! the run answers its command, drops the agent's cached identity and
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
//! EXP-827: a remote login may target an account PROFILE (`profileId`, an
//! existing one) or ask for a new one (`newProfileLabel`). The run points
//! the CLI's config-dir variable at that profile's dir for the sign-out and
//! the login tab it spawns (`coding::agent_login::resolve_login_profile` +
//! `login_env`), and the published result names the id it signed into. The
//! exit re-probe re-reads the profile index, so a fresh profile rides the
//! next heartbeat by itself.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use gpui::{
    div, px, size, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _, ButtonVariant},
    h_flex,
    notification::Notification,
    spinner::Spinner,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
};
use terminal::{TabId, TerminalManager, TerminalManagerEvent};

use coding::agent_login::{self, LoginProgress, LoginTarget};
use coding::CodingAgent;

use crate::agent_login_outcome::{EnterCode, LoginOutcome};
use crate::coding_flow::CodingHub;
use crate::native_dialog::{self, AlertSpec};
use crate::queries;

/// Grid poll cadence — the sign-in URL lands within a second or two of the
/// spawn, and a quarter-second read of a 120×36 grid is nothing next to the
/// PTY itself.
const POLL: Duration = Duration::from_millis(250);

/// How long a remote login may run without ever showing a URL before the
/// command is failed back to the requester.
const REMOTE_URL_TIMEOUT: Duration = Duration::from_secs(600);

/// Copy shared with the web dialog (`device-settings-dialog.tsx`) — the
/// codex switch warning, byte-identical on both.
const CODEX_SWITCH_TITLE: &str = "Switch Codex account";
const CODEX_SWITCH_BODY: &str =
    "Codex logout revokes the token server-side; you'll sign in again on that machine.";
const CODEX_SWITCH_OK: &str = "Sign out and sign in";

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

/// Open a login tab for `agent` on this machine (the local Login / Switch
/// account buttons). `switch` signs out first.
pub(crate) fn open_login_tab(agent: CodingAgent, switch: bool, cx: &mut App) {
    if switch {
        confirm_switch_then(agent, cx, move |cx| {
            start(agent, true, LoginTarget::System, None, cx)
        });
        return;
    }
    start(agent, false, LoginTarget::System, None, cx);
}

/// EXP-849 — "+ Add account": create a FRESH account profile for `agent` on
/// this machine and sign into it, in one go.
///
/// No sign-OUT is involved, which is the whole point of a profile: the new
/// login lands in its own `CLAUDE_CONFIG_DIR`/`CODEX_HOME`, so the accounts
/// already on this machine (and, for codex, every other machine sharing the
/// ambient login) are untouched. The profile is created by
/// `agent_login::resolve_login_profile` from [`LoginTarget::NewProfile`], so
/// a login the person abandons leaves one empty directory and nothing else.
pub(crate) fn open_add_account_tab(agent: CodingAgent, label: String, cx: &mut App) {
    start(agent, false, LoginTarget::NewProfile(label), None, cx);
}

/// EXP-849 — sign in to an EXISTING profile (a `needs_relogin` repair, or a
/// profile created and abandoned). Never a switch: the target profile holds
/// its own credential, so there is nothing to sign out of.
pub(crate) fn open_profile_login_tab(agent: CodingAgent, profile_id: String, cx: &mut App) {
    start(agent, false, LoginTarget::Profile(profile_id), None, cx);
}

/// EXP-484 (D): run an `agent_login` device command. The payload was already
/// validated and claimed by [`crate::device_sync`]; this opens the same tab
/// the local button does and answers the command the moment a URL is up.
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
    // No local confirm: the requester's own dialog already carried the codex
    // warning, and nobody is necessarily sitting at this machine.
    start(
        request.agent,
        request.switch,
        request.target,
        Some(RemoteLogin {
            command_id: command.id,
            published: Arc::new(AtomicBool::new(false)),
        }),
        cx,
    );
}

/// Run `then` once the user has confirmed switching `agent`'s account —
/// immediately for the agents whose sign-out is local (claude), behind a
/// confirm for codex, whose `logout` REVOKES the
/// session with OpenAI so every other machine signed in with it loses
/// access. The copy is byte-identical to the web dialog's.
///
/// The device-settings dialog uses this for REMOTE switches too, so both
/// clients warn with the same words before the same act.
///
/// `then` NEVER runs on the caller's stack (FEED-39). The confirmed path
/// runs it from the alert's OK, long after the click handler returned; the
/// unconfirmed one used to call it inline, and a caller sitting inside its
/// own entity's update (the device-settings dialog handing itself a weak
/// handle) then re-entered that entity — gpui's double lease, a hard panic
/// that took the Linux app down on a remote claude "Switch account". Deferred,
/// both paths reach `then` with every entity released.
pub(crate) fn confirm_switch_then(
    agent: CodingAgent,
    cx: &mut App,
    then: impl Fn(&mut App) + 'static,
) {
    if agent_login::warn_on_switch(agent).is_none() {
        cx.defer(move |cx| then(cx));
        return;
    }
    crate::navigation::on_active_window(cx, move |window, cx| {
        let spec = AlertSpec::new(CODEX_SWITCH_TITLE, CODEX_SWITCH_BODY, CODEX_SWITCH_OK)
            .ok_variant(ButtonVariant::Danger)
            .on_ok(move |_, cx| {
                then(cx);
                true
            });
        native_dialog::open_alert(window, cx, spec);
    });
}

/// The one sequence: profile resolve → (optional) logout → login tab → grid
/// watch → exit re-probe. Deferred, because the caller is typically inside
/// its own window's update and [`crate::coding_flow::any_terminal_dock`]
/// has to update windows to find the dock.
///
/// EXP-827: `target` picks the account profile. It resolves first (a new
/// label creates the profile dir); a refused target answers the requester
/// and starts nothing. The profile's config-dir pair goes on the sign-out
/// AND the login tab, so both act inside the same profile dir.
fn start(
    agent: CodingAgent,
    switch: bool,
    target: LoginTarget,
    remote: Option<RemoteLogin>,
    cx: &mut App,
) {
    let settings = CodingHub::global(cx).read(cx).settings.clone();
    let data_dir = crate::coding_flow::coding_data_dir(cx);
    // EXP-695: a REMOTE sign-in must not pop a browser on this machine —
    // the requester gets the link through the command result instead.
    let mut plan = agent_login::login_plan(&settings, agent, remote.is_some());
    cx.spawn(async move |cx| {
        let resolved = {
            let data_dir = data_dir.clone();
            cx.background_executor()
                .spawn(async move {
                    agent_login::resolve_login_profile(&data_dir, agent, &target).map(|id| {
                        let env = agent_login::login_env(&data_dir, agent, &id);
                        (id, env)
                    })
                })
                .await
        };
        let (profile_id, env) = match resolved {
            Ok(resolved) => resolved,
            Err(message) => {
                log::warn!("[agent-login] {agent:?} profile refused: {message}");
                let _ = cx.update(|cx| {
                    notify(Notification::error(SharedString::from(message.clone())), cx);
                    // Never started: answer the requester and drop the claim
                    // (no run exists to do it on exit).
                    if let Some(remote) = remote.as_ref() {
                        complete(&remote.command_id, false, message, cx);
                        crate::device_sync::release_login(&remote.command_id, cx);
                    }
                });
                return;
            }
        };
        if let Some((key, value)) = env.as_ref() {
            plan.spawn.env.push((key.clone(), value.clone()));
        }
        if switch {
            // EXP-849 (interface E): a codex switch may only sign out a
            // PROFILE — `codex logout` on the ambient login revokes it with
            // OpenAI for every machine sharing it. Refuse rather than do it.
            if let Some(message) = agent_login::switch_logout_blocker(agent, &profile_id) {
                let _ = cx.update(|cx| {
                    notify(Notification::error(SharedString::from(message.clone())), cx);
                    if let Some(remote) = remote.as_ref() {
                        complete(&remote.command_id, false, message, cx);
                        crate::device_sync::release_login(&remote.command_id, cx);
                    }
                });
                return;
            }
            let settings = settings.clone();
            let env = env.clone();
            let logout = cx
                .background_executor()
                .spawn(async move { agent_login::logout_in(&settings, agent, env.as_ref()) })
                .await;
            if let Err(message) = logout {
                // A failed sign-out still lets the login run (the CLI may
                // simply have been signed out already) — say so and continue.
                log::warn!("[agent-login] {agent:?} logout failed: {message}");
                let _ = cx.update(|cx| notify(Notification::warning(SharedString::from(message)), cx));
            }
        }
        let _ = cx.update(|cx| spawn_login_tab(agent, plan, profile_id, remote, cx));
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
    /// EXP-827: the profile the login lands on (`system` for the ambient
    /// login), named in the published result.
    profile_id: String,
    remote: Option<RemoteLogin>,
    finished: AtomicBool,
    /// EXP-765: the tab this run opened — set once it exists, so `finish`
    /// can drop exactly this entry from [`LoginTabs`].
    tab: OnceLock<TabId>,
}

impl LoginRun {
    /// The run ended: answer an unanswered remote command, release the
    /// in-flight claim, drop the agent's cached identity, and re-probe so
    /// the row and the Tools pane tell the truth again.
    fn finish(&self, cx: &mut App) {
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
        // EXP-484: a SWITCH leaves the cache naming the previous account
        // (and its numbers) — drop the entry so the next beat polls afresh
        // instead of re-reporting the identity the user just replaced.
        //
        // EXP-862: the LOGIN's entry, not the agent's: a sign-in into a named
        // profile must not blank its siblings' numbers and health. Dropping
        // it is also what re-reads it at once — a login with no cache entry
        // skips the rotation queue (`coding::agent_usage`), so the account
        // this sign-in just created shows its usage on the very next pass
        // instead of after a stagger window.
        let agent = self.agent;
        let profile = self.profile_id.clone();
        let data_dir = crate::coding_flow::coding_data_dir(cx);
        cx.background_executor()
            .spawn(async move {
                coding::usage_cache::forget_profile(&data_dir, agent.id(), &profile)
            })
            .detach();
        // The re-probe carries the beat with it: the collector's input IS
        // the doctor report, so `refresh_agent_usage` nudges the beat only
        // once the fresh report has landed.
        let hub = CodingHub::global(cx);
        CodingHub::refresh_agent_usage(&hub, cx);
        notify(
            Notification::info(SharedString::from(format!(
                "{} sign-in finished — rechecking.",
                self.agent.label()
            ))),
            cx,
        );
    }

    /// The run never got off the ground (no window, a failed spawn): answer
    /// the requester with WHY and release the claim, without the re-probe
    /// and the "finished" notification an actual run earns.
    fn abandon(&self, message: &str, cx: &mut App) {
        if self.finished.swap(true, Ordering::SeqCst) {
            return;
        }
        self.answer_remote(message, cx);
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
fn spawn_login_tab(
    agent: CodingAgent,
    plan: coding::LoginPlan,
    profile_id: String,
    remote: Option<RemoteLogin>,
    cx: &mut App,
) {
    let run = Arc::new(LoginRun {
        agent,
        profile_id,
        remote,
        finished: AtomicBool::new(false),
        tab: OnceLock::new(),
    });
    let Some(handle) = crate::coding_flow::any_terminal_dock(cx) else {
        notify(
            Notification::error(SharedString::from(
                "Open the main window to sign in to an agent.",
            )),
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
            exit_run.finish(cx);
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
                    closed_run.finish(cx);
                }
            })
            .detach();
            watch_login(manager, tab, run, cx);
        }
        _ => {
            notify(
                Notification::error(SharedString::from(format!(
                    "Could not start the {} sign-in.",
                    agent.label()
                ))),
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
                let _ = cx.update(|cx| run.finish(cx));
                return;
            };
            if !running {
                // Exited but kept open: the exit hook already finished the
                // run — stop polling instead of burning the whole 10-minute
                // budget on a dead grid.
                let _ = cx.update(|cx| run.finish(cx));
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
                            let progress = LoginProgress::url(agent, url, code)
                                .with_profile(run.profile_id.clone());
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

fn notify(note: Notification, cx: &mut App) {
    crate::navigation::on_active_window(cx, move |window, cx| {
        window.push_notification(note, cx);
    });
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
    /// What the machine reported about those agents — where a NEW login lands.
    accounts: std::collections::BTreeMap<String, coding::AgentAccount>,
}

impl LoginDevice {
    /// Whether the machine's AMBIENT login for `agent` is already taken — a
    /// new account then lands in a profile of its own.
    fn ambient_signed_in(&self, agent: CodingAgent) -> bool {
        let Some(account) = self.accounts.get(agent.id()) else {
            return false;
        };
        match account
            .profiles
            .iter()
            .find(|profile| profile.id == coding::SYSTEM_PROFILE)
        {
            Some(ambient) => ambient.signed_in,
            None => account.signed_in,
        }
    }

    /// `Claude Code account 2` — the smallest N ≥ 2 whose label the machine
    /// does not already report for the agent (the ambient login is the
    /// unnamed first), clamped at the server's 64 (web `nextProfileLabel`).
    /// Counting profiles instead would re-mint "account 3" after "account 2"
    /// was removed, and the login-landed rule then closes the sheet at open.
    fn next_profile_label(&self, agent: CodingAgent) -> String {
        let taken: std::collections::HashSet<&str> = self
            .accounts
            .get(agent.id())
            .map(|account| {
                account
                    .profiles
                    .iter()
                    .filter_map(|profile| profile.label.as_deref())
                    .collect()
            })
            .unwrap_or_default();
        (2u32..)
            .map(|n| format!("{} account {}", agent.label(), n))
            .find(|label| !taken.contains(label.as_str()))
            .expect("an unbounded range always yields a free label")
            .chars()
            .take(64)
            .collect()
    }

    /// Where a new login lands on the machine (web `addAccountLoginTarget`):
    /// the ambient login while it is still free — nothing to keep beside it —
    /// otherwise a new profile the machine creates.
    pub(crate) fn add_account_target(&self, agent: CodingAgent) -> LoginTarget {
        if self.ambient_signed_in(agent) {
            LoginTarget::NewProfile(self.next_profile_label(agent))
        } else {
            LoginTarget::System
        }
    }
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
            accounts: crate::device_settings::parse_agent_map::<coding::AgentAccount>(
                row.agent_accounts.as_ref(),
            ),
            device_id,
        });
    }
    out.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    out
}

/// EXP-862 — start a sign-in for `agent` on `device`, wherever it is: the CLI's
/// own login tab on THIS machine, an `agent_login` command plus the status
/// dialog on another of mine. The ONE entry point every chip, menu and dialog
/// uses, so "Sign in" means the same thing everywhere.
pub(crate) fn sign_in_on_device(
    device_id: String,
    device_label: SharedString,
    own: bool,
    agent: CodingAgent,
    target: LoginTarget,
    window: &mut Window,
    cx: &mut App,
) {
    if own {
        // The named helpers, so "add an account" and "repair this profile"
        // stay one call each on this machine too.
        match normalize_own_target(target) {
            LoginTarget::System => open_login_tab(agent, false, cx),
            LoginTarget::Profile(profile_id) => open_profile_login_tab(agent, profile_id, cx),
            LoginTarget::NewProfile(label) => open_add_account_tab(agent, label, cx),
        }
        return;
    }
    open_login_dialog(device_id, device_label, agent, target, window, cx);
}

/// The ambient login is the ABSENCE of a profile, on this machine exactly as
/// it is on the wire (`queue_login_command` drops a `system`/blank id): every
/// chip hands over the row's raw `profile_id`, which IS `system` for the
/// ambient account, and `coding::agent_login::resolve_login_profile` has no
/// directory for that id — so a raw pass-through would refuse the most common
/// sign-in there is ("This machine has no Claude Code profile system.").
fn normalize_own_target(target: LoginTarget) -> LoginTarget {
    match target {
        LoginTarget::Profile(id) if coding::agent_profiles::is_system(Some(&id)) => {
            LoginTarget::System
        }
        other => other,
    }
}

/// `devices.createCommand` for an `agent_login` that names WHERE the login
/// lands: an existing profile, or a fresh one the machine creates
/// (`newProfileLabel`, EXP-827).
///
/// The payload is built here rather than in `api::devices` because that
/// crate's `create_agent_login_command` has no `newProfileLabel` parameter
/// yet (iOS and Android grew one in this wave); folding this into
/// `api::devices::create_agent_login_command` is a follow-up.
fn queue_login_command(
    trpc: &api::TrpcClient,
    device_id: &str,
    agent: CodingAgent,
    target: &LoginTarget,
) -> Result<String, api::ApiError> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        device_id: &'a str,
        kind: &'a str,
        agent: &'a str,
        switch: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        profile_id: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        new_profile_label: Option<&'a str>,
    }
    // The ambient login is the ABSENCE of a profile on this wire (the device's
    // `resolve_login_profile` reads `system`/blank the same way).
    let (profile_id, new_profile_label) = match target {
        LoginTarget::System => (None, None),
        LoginTarget::Profile(id) => (
            Some(id.trim()).filter(|id| !id.is_empty() && *id != coding::SYSTEM_PROFILE),
            None,
        ),
        LoginTarget::NewProfile(label) => (None, Some(label.trim()).filter(|l| !l.is_empty())),
    };
    let created: api::devices::CreatedCommand = trpc.mutation(
        "devices.createCommand",
        &Input {
            device_id,
            kind: "agent_login",
            agent: agent.id(),
            switch: false,
            profile_id,
            new_profile_label,
        },
    )?;
    Ok(created.id)
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
    target: LoginTarget,
    window: &mut Window,
    cx: &mut App,
) {
    let adds_account = matches!(target, LoginTarget::NewProfile(_));
    let title = if adds_account {
        format!("Add a {} account", agent.label())
    } else {
        format!("Sign in to {}", agent.label())
    };
    let spec = native_dialog::DialogSpec::new(title, size(px(460.), px(180.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| {
            LoginDialogView::new(device_id, device_label, agent, target, window, cx)
        });
        native_dialog::DialogContent::new(view)
    });
}

struct LoginDialogView {
    device_id: String,
    device_label: SharedString,
    agent: CodingAgent,
    /// EXP-940: kept so "Try again" can re-queue the SAME login.
    target: LoginTarget,
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
    /// What the device reported about the agent's logins when the dialog
    /// opened. The report MOVING is the success this dialog waits for — a new
    /// profile appearing, or an expired one going healthy again.
    baseline: Vec<(String, bool, coding::agent_accounts::Health)>,
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
        target: LoginTarget,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let devices = sync::Store::global(cx).collections().devices.clone();
        let baseline = login_fingerprint(&device_id, agent, cx);
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
                if login_fingerprint(&this.device_id, this.agent, cx) != this.baseline {
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
            target,
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
        self.baseline = login_fingerprint(&self.device_id, self.agent, cx);
        cx.notify();
        if queries::trpc_client(cx).is_none() {
            self.state = LoginDialogState::Failed("Not signed in.".into());
            return;
        }
        let device_id = self.device_id.clone();
        let agent = self.agent;
        let target = self.target.clone();
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
                    async move { queue_login_command(&trpc, &device_id, agent, &target) }
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

/// What the device currently reports about `agent`'s logins: one entry per
/// profile, each with its signed-in flag and its health. A device that reported
/// no profiles (an older build) yields its single ambient account.
///
/// The sign-in dialog watches this: a SIGN-IN lands as a new entry (a fresh
/// profile) or as an existing one turning healthy (a `needs_relogin` repair),
/// and either is the moment the dialog has nothing left to say.
fn login_fingerprint(
    device_id: &str,
    agent: CodingAgent,
    cx: &App,
) -> Vec<(String, bool, coding::agent_accounts::Health)> {
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
    let accounts = crate::device_settings::parse_agent_map::<coding::AgentAccount>(
        row.agent_accounts.as_ref(),
    );
    let Some(account) = accounts.get(agent.id()) else {
        return Vec::new();
    };
    if account.profiles.is_empty() {
        return vec![(
            coding::SYSTEM_PROFILE.to_string(),
            account.signed_in,
            account.health(),
        )];
    }
    account
        .profiles
        .iter()
        .map(|profile| (profile.id.clone(), profile.signed_in, profile.health()))
        .collect()
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
        let target = device.add_account_target(agent);
        native_dialog::close_then(window, cx, move |window, cx| {
            sign_in_on_device(
                device.device_id.clone(),
                device.label.clone(),
                device.own,
                agent,
                target,
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

    /// EXP-862: every chip on the Devices and Accounts pages hands
    /// [`sign_in_on_device`] the row's RAW `profile_id`, and the ambient
    /// login's is `system` — which has no profile directory, so the own-device
    /// branch has to read it as "the ambient login" exactly like the wire
    /// does, or "Sign in" on the commonest chip there is dies in
    /// `resolve_login_profile`.
    #[test]
    fn the_ambient_profile_id_is_the_ambient_login() {
        assert!(matches!(
            normalize_own_target(LoginTarget::Profile(coding::SYSTEM_PROFILE.to_string())),
            LoginTarget::System
        ));
        assert!(matches!(
            normalize_own_target(LoginTarget::Profile("  ".to_string())),
            LoginTarget::System
        ));
        assert!(matches!(
            normalize_own_target(LoginTarget::Profile(String::new())),
            LoginTarget::System
        ));
        // A real profile still signs into ITSELF.
        assert!(matches!(
            normalize_own_target(LoginTarget::Profile("0a1b2c3d".to_string())),
            LoginTarget::Profile(id) if id == "0a1b2c3d"
        ));
        assert!(matches!(
            normalize_own_target(LoginTarget::NewProfile("Work".to_string())),
            LoginTarget::NewProfile(label) if label == "Work"
        ));
    }

    /// A stand-in for the device-settings dialog: an entity whose click
    /// handler starts a switch and, in the callback, updates ITSELF through
    /// a weak handle (the real dialog queues the `agent_login` command that
    /// way).
    struct Requester {
        queued: usize,
    }

    /// FEED-39: a remote claude "Switch account" crashed the requesting app.
    /// claude needs no confirm, so `confirm_switch_then` ran the callback
    /// inline — inside the dialog's own update — and the callback's
    /// `view.update` double-leased the entity (gpui panics, the app dies).
    /// The callback must reach the entity only once the handler has returned.
    #[gpui::test]
    async fn unconfirmed_switch_callback_runs_off_the_callers_stack(
        cx: &mut gpui::TestAppContext,
    ) {
        let requester = cx.new(|_| Requester { queued: 0 });
        requester.update(cx, |this, cx| {
            let view = cx.entity().downgrade();
            confirm_switch_then(CodingAgent::Claude, cx, move |cx| {
                let _ = view.update(cx, |this, _| this.queued += 1);
            });
            // Still inside the handler: nothing may have touched the entity.
            assert_eq!(this.queued, 0);
        });
        cx.run_until_parked();
        assert_eq!(
            requester.read_with(cx, |this, _| this.queued),
            1,
            "the deferred callback queues exactly one login"
        );
    }

    fn device(profiles: Vec<coding::AgentProfileEntry>, ambient_signed_in: bool) -> LoginDevice {
        let mut accounts = std::collections::BTreeMap::new();
        accounts.insert(
            CodingAgent::Claude.id().to_string(),
            coding::AgentAccount {
                signed_in: ambient_signed_in,
                profiles,
                ..coding::AgentAccount::default()
            },
        );
        LoginDevice {
            device_id: "dev-1".to_string(),
            label: "Studio".into(),
            own: false,
            agents: vec![CodingAgent::Claude],
            accounts,
        }
    }

    fn profile(id: &str, signed_in: bool) -> coding::AgentProfileEntry {
        coding::AgentProfileEntry {
            id: id.to_string(),
            signed_in,
            checked_at: "2026-09-12T10:00:00.000Z".to_string(),
            ..coding::AgentProfileEntry::default()
        }
    }

    fn labelled(id: &str, label: &str) -> coding::AgentProfileEntry {
        coding::AgentProfileEntry {
            label: Some(label.to_string()),
            ..profile(id, true)
        }
    }

    /// The next label is the smallest FREE "account N" (N ≥ 2), not one past
    /// the profile count: after "account 2" was removed, `[system, account 3]`
    /// must mint "account 2" again — never a label that already exists, which
    /// the login-landed rule would treat as done before the login ran.
    #[test]
    fn a_new_profile_takes_the_smallest_free_label() {
        let gap = device(
            vec![
                profile(coding::SYSTEM_PROFILE, true),
                labelled("0a1b2c3d", "Claude Code account 3"),
            ],
            true,
        );
        match gap.add_account_target(CodingAgent::Claude) {
            LoginTarget::NewProfile(label) => assert_eq!(label, "Claude Code account 2"),
            other => panic!("expected a new profile, got {other:?}"),
        }

        let full = device(
            vec![
                profile(coding::SYSTEM_PROFILE, true),
                labelled("0a1b2c3d", "Claude Code account 2"),
                labelled("4e5f6a7b", "Claude Code account 3"),
            ],
            true,
        );
        match full.add_account_target(CodingAgent::Claude) {
            LoginTarget::NewProfile(label) => assert_eq!(label, "Claude Code account 4"),
            other => panic!("expected a new profile, got {other:?}"),
        }

        // Exact, case-sensitive: a differently-cased label does not reserve N.
        let cased = device(
            vec![
                profile(coding::SYSTEM_PROFILE, true),
                labelled("0a1b2c3d", "claude code account 2"),
            ],
            true,
        );
        match cased.add_account_target(CodingAgent::Claude) {
            LoginTarget::NewProfile(label) => assert_eq!(label, "Claude Code account 2"),
            other => panic!("expected a new profile, got {other:?}"),
        }
    }

    /// EXP-862 — where a new login lands (web `addAccountLoginTarget`): the
    /// AMBIENT login while it is still free (nothing to keep beside it),
    /// otherwise a profile of its own, named one past the logins the device
    /// already reports.
    #[test]
    fn a_new_login_takes_the_ambient_slot_only_while_it_is_free() {
        let free = device(vec![profile(coding::SYSTEM_PROFILE, false)], false);
        assert!(matches!(
            free.add_account_target(CodingAgent::Claude),
            LoginTarget::System
        ));
        // A device that reported no profiles at all, signed out: same thing.
        let bare = device(Vec::new(), false);
        assert!(matches!(
            bare.add_account_target(CodingAgent::Claude),
            LoginTarget::System
        ));

        let taken = device(vec![profile(coding::SYSTEM_PROFILE, true)], true);
        match taken.add_account_target(CodingAgent::Claude) {
            LoginTarget::NewProfile(label) => assert_eq!(label, "Claude Code account 2"),
            other => panic!("expected a new profile, got {other:?}"),
        }
        // The first free N past the labels on record (the ambient one is the
        // unnamed first).
        let two = device(
            vec![
                profile(coding::SYSTEM_PROFILE, true),
                labelled("0a1b2c3d", "Claude Code account 2"),
            ],
            true,
        );
        match two.add_account_target(CodingAgent::Claude) {
            LoginTarget::NewProfile(label) => assert_eq!(label, "Claude Code account 3"),
            other => panic!("expected a new profile, got {other:?}"),
        }

        // An agent the device never reported has a free ambient login.
        assert!(matches!(
            two.add_account_target(CodingAgent::Codex),
            LoginTarget::System
        ));
    }
}
