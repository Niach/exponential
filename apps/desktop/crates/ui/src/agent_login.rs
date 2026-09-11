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
//! * pi has no login command — its `/login` is a slash command inside the
//!   running TUI, typed once its prompt shows (or at the 10s deadline
//!   regardless, so a missed anchor never strands the tab);
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use gpui::{App, Entity, SharedString};
use gpui_component::{button::ButtonVariant, notification::Notification, WindowExt as _};
use terminal::{TabId, TerminalManager, TerminalManagerEvent};

use coding::agent_login::{self, LoginProgress, LoginTarget};
use coding::CodingAgent;

use crate::coding_flow::CodingHub;
use crate::native_dialog::{self, AlertSpec};
use crate::queries;

/// Grid poll cadence — pi's prompt and the sign-in URL both land within a
/// second or two of the spawn, and a quarter-second read of a 120×36 grid is
/// nothing next to the PTY itself.
const POLL: Duration = Duration::from_millis(250);

/// Type pi's `/login` at this deadline even if its prompt never matched (a
/// themed banner, a resized grid): a login that types one line too early is
/// recoverable, one that never types anything is not.
const READY_DEADLINE: Duration = Duration::from_secs(10);

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
const CODE_ENTERED: &str = "Code entered — the machine is finishing the sign-in.";
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
/// immediately for the agents whose sign-out is local (claude; pi has no
/// account at all), behind a confirm for codex, whose `logout` REVOKES the
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
        let agent = self.agent;
        let data_dir = crate::coding_flow::coding_data_dir(cx);
        cx.background_executor()
            .spawn(async move { coding::usage_cache::forget(&data_dir, agent.id()) })
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
            watch_login(plan, manager, tab, run, cx);
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

/// The 250ms foreground grid watch: types pi's `/login` once its prompt is
/// up (or at the deadline), and — for a remote run — publishes the sign-in
/// URL the instant the driver recognizes one.
fn watch_login(
    plan: coding::LoginPlan,
    manager: Entity<TerminalManager>,
    tab: TabId,
    run: Arc<LoginRun>,
    cx: &mut App,
) {
    let typed = plan.typed_after_ready.clone();
    if typed.is_none() && run.remote.is_none() {
        return; // nothing to watch: claude/codex started locally
    }
    let agent = run.agent;
    cx.spawn(async move |cx| {
        let started = std::time::Instant::now();
        let mut typed = typed;
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
            if let Some(text) = typed.clone() {
                let ready = agent_login::pi_prompt_ready(&lines)
                    || started.elapsed() >= READY_DEADLINE;
                if ready {
                    typed = None;
                    let _ = cx.update(|cx| write_input(&manager, tab, text.as_bytes(), cx));
                }
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
            } else if typed.is_none() {
                return; // pi's line is in — nothing left to watch
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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AppContext as _;

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
}
