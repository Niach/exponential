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
//! within a beat.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gpui::{App, Entity, SharedString};
use gpui_component::{button::ButtonVariant, notification::Notification, WindowExt as _};
use terminal::{TabId, TerminalManager, TerminalManagerEvent};

use coding::agent_login::{self, LoginProgress};
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
        confirm_switch_then(agent, cx, move |cx| start(agent, true, None, cx));
        return;
    }
    start(agent, false, None, cx);
}

/// EXP-484 (D): run an `agent_login` device command. The payload was already
/// validated and claimed by [`crate::device_sync`]; this opens the same tab
/// the local button does and answers the command the moment a URL is up.
pub(crate) fn start_remote_login(command: api::devices::PendingCommand, cx: &mut App) {
    let agent = command.payload["agent"]
        .as_str()
        .and_then(CodingAgent::parse)
        .unwrap_or(CodingAgent::Claude);
    let switch = command.payload["switch"].as_str() == Some("true");
    // No local confirm: the requester's own dialog already carried the codex
    // warning, and nobody is necessarily sitting at this machine.
    start(
        agent,
        switch,
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
pub(crate) fn confirm_switch_then(
    agent: CodingAgent,
    cx: &mut App,
    then: impl Fn(&mut App) + 'static,
) {
    if agent_login::warn_on_switch(agent).is_none() {
        then(cx);
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

/// The one sequence: (optional) logout → login tab → grid watch → exit
/// re-probe. Deferred, because the caller is typically inside its own
/// window's update and [`crate::coding_flow::any_terminal_dock`] has to
/// update windows to find the dock.
fn start(agent: CodingAgent, switch: bool, remote: Option<RemoteLogin>, cx: &mut App) {
    let settings = CodingHub::global(cx).read(cx).settings.clone();
    let plan = agent_login::login_plan(&settings, agent);
    cx.spawn(async move |cx| {
        if switch {
            let settings = settings.clone();
            let logout = cx
                .background_executor()
                .spawn(async move { agent_login::logout(&settings, agent) })
                .await;
            if let Err(message) = logout {
                // A failed sign-out still lets the login run (the CLI may
                // simply have been signed out already) — say so and continue.
                log::warn!("[agent-login] {agent:?} logout failed: {message}");
                let _ = cx.update(|cx| notify(Notification::warning(SharedString::from(message)), cx));
            }
        }
        let _ = cx.update(|cx| spawn_login_tab(agent, plan, remote, cx));
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
    remote: Option<RemoteLogin>,
    finished: AtomicBool,
}

impl LoginRun {
    /// The run ended: answer an unanswered remote command, release the
    /// in-flight claim, drop the agent's cached identity, and re-probe so
    /// the row and the Tools pane tell the truth again.
    fn finish(&self, cx: &mut App) {
        if self.finished.swap(true, Ordering::SeqCst) {
            return;
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
    remote: Option<RemoteLogin>,
    cx: &mut App,
) {
    let run = Arc::new(LoginRun {
        agent,
        remote,
        finished: AtomicBool::new(false),
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
    let exit_run = Arc::clone(&run);
    let exit_hook: terminal::tab::ExitHook = Box::new(move |_, _, cx| exit_run.finish(cx));
    let opened = handle.update(cx, |_, window, cx| {
        let panel = crate::coding_flow::window_terminal_dock(window, cx)?;
        let manager = panel.read(cx).manager().clone();
        let tab = panel
            .update(cx, |panel, cx| {
                panel.launch_agent_login(agent, &plan, Some(exit_hook), cx)
            })
            .ok()?;
        Some((manager, tab))
    });
    match opened {
        Ok(Some((manager, tab))) => {
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
