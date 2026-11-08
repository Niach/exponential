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
//! EXP-1533/1626: "Persist login" (an OPTION on an existing signed-in claude
//! login, and its renewal) runs `claude setup-token` in a fresh STAGING dir
//! ([`start_long_lived`], locally or off an `agent_login {profileId,
//! longLived}`), never inside the profile, so the browser login the profile
//! holds is never touched: the same tab, link and code hand-off; the
//! one-year token it prints is scraped off the grid and attached to that
//! profile (`agent_login::commit_setup_token`), the staging dir dropped, and
//! the tab closes on a clean exit. Nothing commits by email.
//!
//! EXP-1626 (reverting EXP-1600's long-lived default): every sign-in (Add
//! account, Sign in, the menu; a tab here or the dialog for another of
//! mine) is the CLI's normal BROWSER login. An older client's long-lived
//! Add (no profile) still lands as a new profile
//! (`agent_login::commit_setup_token_add`).
//!
//! There is no import (EXP-1532): an old client's `agent_login {agent,
//! import: "true"}` is refused by the payload parse
//! (`agent_login::IMPORT_REMOVED`); every profile signs in on its own.
//!
//! EXP-1626: every remote run, and every local long-lived one, is BOUNDED:
//! [`REMOTE_URL_TIMEOUT`] after it started (past a code's
//! `agent_login::CODE_GRACE`) the machine closes its tab, which kills the
//! child for sure (`Terminator`: SIGTERM, then SIGKILL) and drops staging.
//! A long-lived run's code command completes on its OUTCOME (the token, or
//! setup-token's refusal), never at the keystroke. An `agent_login_cancel`
//! ([`cancel_remote_login`]) stops the run of the login command it names
//! unless a code already went in.

use crate::toast::Toast;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

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

/// How long a remote login (or a local long-lived one) may run before the
/// machine stops it (EXP-1626: whether or not its link was published; a
/// code typed in the last `agent_login::CODE_GRACE` holds it that long
/// more). A run that never showed a URL fails its command with
/// [`NO_LINK_IN_TIME`].
const REMOTE_URL_TIMEOUT: Duration = agent_login::LOGIN_RUN_TIMEOUT;
const NO_LINK_IN_TIME: &str = "The sign-in did not produce a link in time.";

/// The two sentences a code command completes with — byte-identical to the
/// daemon executor (`cli::agent_login_host`); the clients show a failed
/// row's `result` verbatim.
///
/// EXP-940: the success one is the phase the requester is IN, not a sentence
/// about a machine finishing something. It is the ×4 `SIGNING_IN` string
/// (web `agent-login-dialog.tsx`), so a client that does print the wire text
/// prints exactly what its own spinner says.
const SIGNING_IN: &str = agent_login::SIGNING_IN;

/// EXP-1627: the sign-in dialog's waiting line, `copy.signInWaiting`.
fn sign_in_waiting(device: &str, agent: &str) -> String {
    crate::machines::COPY_SIGN_IN_WAITING
        .replace("{device}", device)
        .replace("{agent}", agent)
}
const CODE_ENTERED: &str = SIGNING_IN;
const NO_LOGIN_WAITING: &str = "No sign-in is waiting for a code on this machine.";

/// EXP-765: the login tabs currently open, by agent id — where a handed-back
/// authorization code gets typed. Local and remote logins both register (a
/// code can be sent to a machine whose own user opened the tab); a run's
/// finish removes ITS entry only, so a login started after it keeps its own.
/// EXP-1626: remote runs also by their `agent_login` command id — what an
/// `agent_login_cancel` names.
#[derive(Default)]
struct LoginTabs {
    by_agent: HashMap<String, Arc<LoginRun>>,
    by_command: HashMap<String, Arc<LoginRun>>,
}

/// EXP-1626 — what an `agent_login_cancel` of `login_command_id` does, and
/// to which run.
fn cancel_lookup(
    by_command: &HashMap<String, Arc<LoginRun>>,
    login_command_id: &str,
) -> (agent_login::CancelDisposition, Option<Arc<LoginRun>>) {
    let run = by_command.get(login_command_id).cloned();
    let disposition = agent_login::CancelDisposition::of(
        run.is_some(),
        run.as_ref().is_some_and(|run| run.code_entered()),
    );
    (disposition, run)
}

/// EXP-1626: run an `agent_login_cancel` device command — the requester
/// closed its sign-in dialog before the login landed. The run of the named
/// `agent_login` is stopped (its tab closed: the child killed, staging
/// dropped) unless a code already went in; either way, and with no such
/// run, the cancel completes ok.
pub(crate) fn cancel_remote_login(command: api::devices::PendingCommand, cx: &mut App) {
    let login_command_id = match agent_login::parse_cancel_payload(&command.payload) {
        Ok(id) => id,
        Err(message) => {
            complete(&command.id, false, message, cx);
            return;
        }
    };
    let (disposition, run) = cancel_lookup(&cx.default_global::<LoginTabs>().by_command, &login_command_id);
    log::info!(
        "[agent-login] cancel {} of sign-in {login_command_id}: {disposition:?}",
        command.id
    );
    if let (agent_login::CancelDisposition::Stop, Some(run)) = (disposition, run) {
        // The server already failed a still-pending row with
        // `LOGIN_CANCELLED`; nothing this run says may answer it again.
        if let Some(remote) = run.remote.as_ref() {
            remote.published.store(true, Ordering::SeqCst);
        }
        stop_run(&run, "cancelled by the requester", cx);
    }
    complete(&command.id, true, disposition.message().to_string(), cx);
    // Like a code, the claim stays: a redelivery must not run it twice.
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
        Some(run) if !code.is_empty() => match run.opened.get().cloned() {
            Some((manager, tab)) => match tab_state(&manager, tab, cx) {
                Some((lines, true)) => {
                    let failure_on_screen = matches!(
                        steer::agent_login_driver::observe_login_screen(&agent, &lines),
                        steer::agent_login_driver::LoginObservation::Failed(_)
                    );
                    write_input(&manager, tab, format!("{code}\r").as_bytes(), cx);
                    // Never the code itself.
                    log::info!(
                        "[agent-login] code {} typed into the {agent} sign-in tab ({})",
                        command.id,
                        run.label()
                    );
                    Some((run, failure_on_screen))
                }
                _ => None,
            },
            None => None,
        },
        _ => None,
    };
    match typed {
        // EXP-1626: a long-lived run answers the code on its OUTCOME.
        Some((run, failure_on_screen)) if run.long_lived => {
            run.note_code(Some(&command.id), failure_on_screen, cx);
        }
        Some((run, _)) => {
            run.note_code(None, false, cx);
            complete(&command.id, true, CODE_ENTERED.to_string(), cx);
        }
        None => complete(&command.id, false, NO_LOGIN_WAITING.to_string(), cx),
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
/// whether a login that lands on ANOTHER known email warns. EXP-1626: the
/// CLI's browser login, for claude too (Persist login is its own entry,
/// [`persist_login_on_device`]).
pub(crate) fn open_login_tab(agent: CodingAgent, intended: Option<String>, cx: &mut App) {
    let target = match intended {
        Some(id) => LoginTarget::Profile(id),
        None => LoginTarget::Add,
    };
    start(agent, target, None, cx);
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
    let remote = RemoteLogin {
        command_id: command.id.clone(),
        published: Arc::new(AtomicBool::new(false)),
    };
    // EXP-1533/1626: Persist login on a profile, or (no profile) an older
    // client's long-lived Add.
    if request.long_lived {
        start_long_lived(request.target.intended().map(str::to_string), Some(remote), cx);
        return;
    }
    start(request.agent, request.target, Some(remote), cx);
}

/// Answer a remote command that never got a run (or ran no tab) and drop its
/// claim.
fn answer_unstarted(remote: Option<&RemoteLogin>, ok: bool, message: String, cx: &mut App) {
    if let Some(remote) = remote {
        complete(&remote.command_id, ok, message, cx);
        crate::device_sync::release_login(&remote.command_id, cx);
    }
}

/// What a committed sign-in says here: the duplicate warning, or
/// the plain line.
fn commit_toast(commit: &agent_login::LoginCommit, fallback: String) -> Toast {
    match commit.duplicate_warning() {
        Some(warning) => Toast::warning(warning),
        None => Toast::info(fallback),
    }
}

/// The one sequence: codex fetch → staging dir → login tab
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
                    notify(
                        crate::action_error::failed_toast("sign in", agent.label(), Some(message.clone())),
                        cx,
                    );
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
                    notify(
                        crate::action_error::failed_toast("sign in", agent.label(), Some(message.clone())),
                        cx,
                    );
                    answer_unstarted(remote.as_ref(), false, message, cx);
                });
                return;
            }
        };
        plan.spawn.env.push(env);
        let run = LoginRun::new(
            agent,
            settings,
            Some(staging),
            target.intended().map(str::to_string),
            remote,
            false,
        );
        let _ = cx.update(|cx| spawn_login_tab(plan, run, cx));
    })
    .detach();
}

/// EXP-1533/1626 — PERSIST claude login `profile` on THIS machine:
/// `claude setup-token` in a fresh STAGING dir (never the profile's, so its
/// browser login is never touched), in a login tab like any sign-in
/// (`remote` = answer that `agent_login` command with the link); the token
/// is attached to the profile once the run printed it. `profile: None` = an
/// older client's long-lived Add (EXP-1600): the staging dir becomes a new
/// profile once the token is in.
fn start_long_lived(profile: Option<String>, remote: Option<RemoteLogin>, cx: &mut App) {
    let agent = CodingAgent::Claude;
    let settings = CodingHub::global(cx).read(cx).settings.clone();
    let data_dir = crate::coding_flow::coding_data_dir(cx);
    let planned = match profile.as_deref() {
        Some(profile) => agent_login::long_lived_plan(&settings, &data_dir, profile, remote.is_some()),
        None => agent_login::long_lived_add_plan(&settings, &data_dir, remote.is_some()),
    };
    let (staging, plan) = match planned {
        Ok(planned) => planned,
        Err(message) => {
            notify(
                crate::action_error::failed_toast("sign in", agent.label(), Some(message.clone())),
                cx,
            );
            answer_unstarted(remote.as_ref(), false, message, cx);
            return;
        }
    };
    let run = LoginRun::new(agent, settings, Some(staging), profile, remote, true);
    // Deferred like [`start`]: the caller is inside a window's update.
    cx.defer(move |cx| spawn_login_tab(plan, run, cx));
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
    /// The fresh dir the CLI signs in inside; committed (or dropped) on
    /// finish. EXP-1626: a persist run has one too (it never runs in the
    /// profile's dir); always `Some` for a run that opened a tab.
    staging: Option<PathBuf>,
    /// The profile the person clicked "Sign in" on (`None` = Add account);
    /// a long-lived run's profile.
    intended: Option<String>,
    remote: Option<RemoteLogin>,
    /// EXP-1533: a `claude setup-token` run for `intended`.
    long_lived: bool,
    /// EXP-1533: the token the grid showed last (never logged).
    setup_token: Mutex<Option<(String, Option<i64>)>>,
    finished: AtomicBool,
    /// EXP-765: the tab this run opened — set once it exists, so `finish`
    /// can drop exactly this entry from [`LoginTabs`] (and EXP-1626's stop
    /// can close it).
    opened: OnceLock<(Entity<TerminalManager>, TabId)>,
    /// EXP-1626: ends the tab's child for sure, even once the tab is gone.
    terminator: OnceLock<terminal::pty::Terminator>,
    /// EXP-1626: the run's clock ([`REMOTE_URL_TIMEOUT`]).
    started: Instant,
    /// EXP-1626: when a code last went into the tab (`CODE_GRACE`, and a
    /// cancel leaves a run that took one alone).
    last_code_at: Mutex<Option<Instant>>,
    /// EXP-1626: a long-lived run's code command, answered on its outcome.
    pending_code: Mutex<Option<agent_login::PendingCode>>,
}

impl LoginRun {
    fn new(
        agent: CodingAgent,
        settings: coding::Settings,
        staging: Option<PathBuf>,
        intended: Option<String>,
        remote: Option<RemoteLogin>,
        long_lived: bool,
    ) -> Self {
        Self {
            agent,
            settings,
            staging,
            intended,
            remote,
            long_lived,
            setup_token: Mutex::new(None),
            finished: AtomicBool::new(false),
            opened: OnceLock::new(),
            terminator: OnceLock::new(),
            started: Instant::now(),
            last_code_at: Mutex::new(None),
            pending_code: Mutex::new(None),
        }
    }

    /// What the log calls this run: its command id, or `local`.
    fn label(&self) -> String {
        match self.remote.as_ref() {
            Some(remote) => format!("sign-in {}", remote.command_id),
            None => "local sign-in".to_string(),
        }
    }

    /// EXP-1626: a code went into this run's tab.
    fn code_entered(&self) -> bool {
        self.last_code_at.lock().map(|at| at.is_some()).unwrap_or(true)
    }

    /// EXP-1626: a code just went in. `command_id` = a long-lived run's
    /// code command, answered on the outcome ([`Self::settle_code`]); one
    /// still pending from an earlier code is answered as superseded.
    fn note_code(&self, command_id: Option<&str>, failure_on_screen: bool, cx: &mut App) {
        if let Ok(mut at) = self.last_code_at.lock() {
            *at = Some(Instant::now());
        }
        let Some(command_id) = command_id else {
            return;
        };
        let replaced = self.pending_code.lock().ok().and_then(|mut slot| {
            slot.replace(agent_login::PendingCode::new(command_id, failure_on_screen))
        });
        if let Some(replaced) = replaced {
            complete(&replaced.command_id, false, "A newer code replaced this one.".to_string(), cx);
        }
    }

    /// The time since the last code, if one went in.
    fn since_code(&self) -> Option<Duration> {
        self.last_code_at.lock().ok().and_then(|at| *at).map(|at| at.elapsed())
    }

    fn has_token(&self) -> bool {
        self.setup_token.lock().map(|slot| slot.is_some()).unwrap_or(false)
    }

    /// EXP-1626: one look at a long-lived run with a code in flight — the
    /// token answers it ok, setup-token's refusal (`failure`) answers it
    /// failed with that sentence.
    fn settle_code(&self, failure: Option<&str>, cx: &mut App) {
        let token = self.has_token();
        let settled = self.pending_code.lock().ok().and_then(|mut slot| {
            let outcome = slot.as_mut()?.observe(token, failure)?;
            slot.take().map(|code| (code.command_id, outcome))
        });
        if let Some((command_id, (ok, message))) = settled {
            log::info!("[agent-login] code {command_id} settled ({}): ok={ok}", self.label());
            // A refused code protects nothing: the requester's retry may
            // cancel this run, and the grace no longer applies.
            if !ok {
                if let Ok(mut at) = self.last_code_at.lock() {
                    *at = None;
                }
            }
            complete(&command_id, ok, message, cx);
        }
    }

    /// EXP-1626: the run ended — a code still in flight is answered by
    /// whether a token came out.
    fn settle_code_on_end(&self, cx: &mut App) {
        let token = self.has_token();
        let pending = self.pending_code.lock().ok().and_then(|mut slot| slot.take());
        if let Some(code) = pending {
            let (ok, message) = code.ended(token);
            log::info!("[agent-login] code {} settled at the end ({}): ok={ok}", code.command_id, self.label());
            complete(&code.command_id, ok, message, cx);
        }
    }

    /// EXP-1533: keep the setup token `lines` show, if any.
    fn note_screen(&self, lines: &[String]) {
        if !self.long_lived {
            return;
        }
        if let Some(found) = agent_login::scrape_setup_token(lines) {
            if let Ok(mut slot) = self.setup_token.lock() {
                *slot = Some(found);
            }
        }
    }

    /// The run ended: answer an unanswered remote command, release the
    /// in-flight claim, then — on a CLEAN exit — COMMIT the login by email
    /// (the commit drops the landed login's cached usage, so it is read
    /// afresh) and re-probe so the row and the Tools pane tell the truth
    /// again. Anything else drops the staging dir and touches nothing.
    fn finish(&self, clean: bool, cx: &mut App) {
        if self.finished.swap(true, Ordering::SeqCst) {
            return;
        }
        // EXP-1626: a hand-closed tab only SIGHUPs its child; whatever
        // ended the run, its child ends too (a no-op once it exited).
        if let Some(terminator) = self.terminator.get() {
            terminator.terminate();
        }
        if let Some((_, tab)) = self.opened.get() {
            let tab = *tab;
            let tabs = cx.default_global::<LoginTabs>();
            if tabs
                .by_agent
                .get(self.agent.id())
                .is_some_and(|run| run.opened.get().is_some_and(|(_, open)| *open == tab))
            {
                tabs.by_agent.remove(self.agent.id());
            }
            if let Some(remote) = self.remote.as_ref() {
                tabs.by_command.remove(&remote.command_id);
            }
        }
        self.answer_remote("The sign-in ended before a link appeared.", cx);
        let agent = self.agent;
        if self.long_lived {
            // The exit hook read the grid once more: the token is in by now.
            self.settle_code_on_end(cx);
            self.finish_long_lived(cx);
            return;
        }
        let Some(staging) = self.staging.clone() else {
            return;
        };
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
                    Err(message) => notify(
                        crate::action_error::failed_toast("sign in", agent.label(), Some(message)),
                        cx,
                    ),
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

    /// EXP-1533: store the token the run printed in its profile (whatever
    /// the exit said: a printed token is a minted one) and re-probe.
    /// EXP-1600: a long-lived Add's token makes its staging dir a new
    /// profile; no token drops the staging dir.
    fn finish_long_lived(&self, cx: &mut App) {
        let agent = self.agent;
        let scraped = self.setup_token.lock().ok().and_then(|mut slot| slot.take());
        let Some(scraped) = scraped else {
            log::info!("[agent-login] the persist sign-in ended without a token");
            if let Some(staging) = self.staging.clone() {
                cx.background_executor()
                    .spawn(async move { agent_login::abandon_login(agent, &staging) })
                    .detach();
            }
            return;
        };
        let (profile, staging) = (self.intended.clone(), self.staging.clone());
        let data_dir = crate::coding_flow::coding_data_dir(cx);
        cx.spawn(async move |cx| {
            let stored = cx
                .background_executor()
                .spawn(async move {
                    match (profile, staging) {
                        // EXP-1626: Persist login — the token goes onto the
                        // profile, the run's staging dir goes.
                        (Some(profile), Some(staging)) => {
                            agent_login::commit_setup_token(&data_dir, &profile, &staging, scraped)
                        }
                        (Some(profile), None) => agent_login::store_setup_token(&data_dir, &profile, scraped),
                        (None, Some(staging)) => agent_login::commit_setup_token_add(&data_dir, &staging, scraped)
                            .map(|(_, expires_at)| expires_at),
                        (None, None) => Err(agent_login::LONG_LIVED_NEEDS_PROFILE.to_string()),
                    }
                })
                .await;
            let _ = cx.update(|cx| {
                match stored {
                    Ok(_) => notify(
                        Toast::info(format!("{} sign-in finished — rechecking.", agent.label())),
                        cx,
                    ),
                    Err(message) => notify(
                        crate::action_error::failed_toast("sign in", agent.label(), Some(message)),
                        cx,
                    ),
                }
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
        let (agent, Some(staging)) = (self.agent, self.staging.clone()) else {
            return;
        };
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
            crate::action_error::failed_toast(
                "sign in",
                agent.label(),
                Some("Open the main window to sign in to an agent.".into()),
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
            // EXP-1533: the token is the CLI's LAST output — read the grid a
            // beat after the exit (outside the manager's update), then
            // finish and close the tab so the token does not linger on it.
            if exit_run.long_lived {
                let (run, manager, success) = (Arc::clone(&exit_run), exit_manager.clone(), exit.success);
                cx.spawn(async move |cx| {
                    cx.background_executor().timer(POLL).await;
                    let _ = cx.update(|cx| {
                        if let Some((lines, _)) = tab_state(&manager, tab, cx) {
                            run.note_screen(&lines);
                        }
                        run.finish(success, cx);
                        if success {
                            manager.update(cx, |manager, cx| manager.close_tab(tab, cx));
                        }
                    });
                })
                .detach();
                return;
            }
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
        // EXP-1626: the handle that ends this child for sure.
        let terminator = manager
            .read(cx)
            .tab(tab)
            .map(|open| open.view.read(cx).session().borrow().terminator());
        Some((manager, tab, terminator))
    });
    match opened {
        Ok(Some((manager, tab, terminator))) => {
            // EXP-765: this is where a handed-back code gets typed while the
            // tab lives. Latest wins per agent (a second login supersedes).
            let _ = run.opened.set((manager.clone(), tab));
            if let Some(terminator) = terminator {
                let _ = run.terminator.set(terminator);
            }
            let tabs = cx.default_global::<LoginTabs>();
            tabs.by_agent.insert(agent.id().to_string(), Arc::clone(&run));
            // EXP-1626: what an `agent_login_cancel` names.
            if let Some(remote) = run.remote.as_ref() {
                tabs.by_command.insert(remote.command_id.clone(), Arc::clone(&run));
            }
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
            // EXP-1626: a local Persist login says what it does (the
            // dialog's one muted line; a tab has no dialog, so a toast does).
            if run.remote.is_none() && run.long_lived && run.intended.is_some() {
                notify(Toast::info(crate::machines::COPY_PERSIST_NOTE), cx);
            }
            watch_login(manager, tab, run, cx);
        }
        _ => {
            notify(crate::action_error::failed_toast("sign in", agent.label(), None), cx);
            run.abandon("The machine could not start the sign-in.", cx);
        }
    }
}

/// EXP-1626 — stop a run: its child is ended for sure (SIGTERM, then
/// SIGKILL) and its tab closed, which finishes the run unclean (staging
/// dropped, nothing committed). A run with no tab yet has nothing to stop.
fn stop_run(run: &Arc<LoginRun>, reason: &str, cx: &mut App) {
    log::info!("[agent-login] stopping the {} ({reason})", run.label());
    if let Some(terminator) = run.terminator.get() {
        terminator.terminate();
    }
    if let Some((manager, tab)) = run.opened.get().cloned() {
        manager.update(cx, |manager, cx| manager.close_tab(tab, cx));
    }
    // Belt: the `TabClosed` watch normally finished it already.
    run.finish(false, cx);
}

/// The 250ms foreground grid watch: for a remote run, publishes the sign-in
/// URL the instant the driver recognizes one; for a long-lived run, keeps
/// the token it prints and answers a code on its outcome. EXP-1626: every
/// watched run is stopped [`REMOTE_URL_TIMEOUT`] after it started.
fn watch_login(
    manager: Entity<TerminalManager>,
    tab: TabId,
    run: Arc<LoginRun>,
    cx: &mut App,
) {
    if run.remote.is_none() && !run.long_lived {
        return; // nothing to watch: a local browser login
    }
    let agent = run.agent;
    cx.spawn(async move |cx| {
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
            // EXP-1533: a long-lived run watches for its token to the end
            // (its exit hook reads the grid once more and finishes it).
            run.note_screen(&lines);
            if !running && run.long_lived {
                return;
            }
            if !running {
                // Exited but kept open: the exit hook normally finished the
                // run already — stop polling instead of burning the whole
                // 10-minute budget on a dead grid. Should this edge win, it
                // reads the same exit the hook would have.
                let clean = cx.update(|cx| tab_exit_code(&manager, tab, cx) == Some(0));
                let _ = cx.update(|cx| run.finish(clean, cx));
                return;
            }
            let observation = steer::agent_login_driver::observe_login_screen(agent.id(), &lines);
            // EXP-1626: a code in a long-lived run completes on its outcome.
            if run.long_lived {
                let failure = match &observation {
                    steer::agent_login_driver::LoginObservation::Failed(message) => Some(message.as_str()),
                    _ => None,
                };
                let _ = cx.update(|cx| run.settle_code(failure, cx));
            }
            if let Some(remote) = run.remote.as_ref().filter(|remote| !remote.published.load(Ordering::SeqCst)) {
                match observation {
                    steer::agent_login_driver::LoginObservation::Url { url, code } => {
                        if remote
                            .published
                            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok()
                        {
                            log::info!("[agent-login] {}: published the sign-in link", run.label());
                            let progress = LoginProgress::url(agent, url, code);
                            let _ = cx.update(|cx| {
                                complete(&remote.command_id, true, progress.to_result_text(), cx)
                            });
                        }
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
                            log::info!("[agent-login] {}: the CLI failed before a link: {message}", run.label());
                            let _ = cx
                                .update(|cx| complete(&remote.command_id, false, message, cx));
                        }
                    }
                    steer::agent_login_driver::LoginObservation::Nothing => {}
                }
            }
            // EXP-1626: nothing outlives the bound — published or not, a
            // run nobody finished is stopped (a fresh code buys its grace).
            if agent_login::login_run_expired(run.started.elapsed(), run.since_code()) {
                let _ = cx.update(|cx| {
                    run.answer_remote(NO_LINK_IN_TIME, cx);
                    stop_run(&run, "timed out", cx);
                });
                return;
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
/// a command's `result`). EXP-1626: a failed call and a row that was no
/// longer pending (`{ok:false}`) both warn with the command id.
fn complete(command_id: &str, ok: bool, message: String, cx: &mut App) {
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("[agent-login] completeCommand {command_id}: not signed in, the answer is lost");
        return;
    };
    let command_id = command_id.to_string();
    cx.background_executor()
        .spawn(async move {
            match api::devices::complete_command(&trpc, &command_id, ok, Some(&message)) {
                Ok(true) => {}
                Ok(false) => log::warn!(
                    "[agent-login] completeCommand {command_id}: the row was no longer pending"
                ),
                Err(err) => log::warn!("[agent-login] completeCommand {command_id} failed: {err}"),
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
/// one of MINE, online, with at least one agent installed there.
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
/// running `agent` when one is named, and not among
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
    // EXP-1626: the browser login, claude's included.
    open_login_dialog(device_id, device_label, agent, intended, false, window, cx);
}

/// EXP-1533/1626 — "Persist login" / "Renew persisted login" on claude
/// login `profile_id` of `device`, IN PLACE: a login tab here, or the remote
/// sign-in dialog queuing `agent_login {profileId, longLived}` on another
/// of mine. The machine attaches the token to that very profile and leaves
/// its browser login untouched.
pub(crate) fn persist_login_on_device(
    device_id: String,
    device_label: SharedString,
    own: bool,
    profile_id: String,
    window: &mut Window,
    cx: &mut App,
) {
    if own {
        start_long_lived(Some(profile_id), None, cx);
        return;
    }
    open_login_dialog(device_id, device_label, CodingAgent::Claude, Some(profile_id), true, window, cx);
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

/// EXP-862 — the sign-in as its own dialog (EXP-1627: titled "Sign in"): a title and ONE status
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
///
/// EXP-1626: `persist` (claude, `intended` set) = Persist login of that
/// profile, whose one muted line is `copy.persistNote`; otherwise the
/// browser login, with no mode line and no mode button.
pub(crate) fn open_login_dialog(
    device_id: String,
    device_label: SharedString,
    agent: CodingAgent,
    intended: Option<String>,
    persist: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let persist = persist && agent == CodingAgent::Claude && intended.is_some();
    // EXP-1627: the title is `copy.signInTitle`, never the agent's name, and
    // the panel HUGS its content (no fixed height, no empty band under the
    // waiting line): `hug` ignores the height, and the view pads itself
    // (a padded body scrolls, which measures 0 in a hugging panel).
    let spec = native_dialog::DialogSpec::new(
        crate::machines::COPY_SIGN_IN_TITLE,
        size(px(460.), px(180.)),
    )
    .hug();
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| {
            LoginDialogView::new(device_id, device_label, agent, intended, persist, window, cx)
        });
        native_dialog::DialogContent::new(view).padless()
    });
}

struct LoginDialogView {
    device_id: String,
    device_label: SharedString,
    agent: CodingAgent,
    /// The profile the person aimed at (`None` = Add account) — kept so "Try
    /// again" re-queues the SAME login, and read by the duplicate warning.
    intended: Option<String>,
    /// EXP-1533/1626: the request is Persist login of claude profile
    /// `intended` (`agent_login {profileId, longLived}`); otherwise the
    /// browser login.
    long_lived: bool,
    /// EXP-1626: the `agent_login` command the live attempt queued — what
    /// closing the dialog (or a retry) cancels before the login landed.
    login_command: Option<String>,
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
        long_lived: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        // EXP-1626: closing the dialog before the login landed stops the
        // run on the machine (`agent_login_cancel`).
        cx.on_release(|this: &mut Self, cx: &mut App| this.cancel_login(cx)).detach();
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
                    // Landed: there is nothing left to cancel.
                    this.login_command = None;
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
            long_lived,
            login_command: None,
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
        // EXP-1626: a retry (or a mode switch) abandons the last attempt's
        // run: stop it, or the machine holds two sign-ins at once.
        self.cancel_login(cx);
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
            self.fail("Not signed in.".into(), cx);
            return;
        }
        let device_id = self.device_id.clone();
        let agent = self.agent;
        let intended = self.intended.clone();
        let long_lived = self.long_lived;
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
                        match (long_lived, intended.as_deref()) {
                            // EXP-1626: Persist login of that profile.
                            (true, Some(profile)) => {
                                api::devices::create_long_lived_login_command(&trpc, &device_id, profile)
                            }
                            _ => api::devices::create_agent_login_command(
                                &trpc,
                                &device_id,
                                agent.id(),
                                intended.as_deref(),
                            ),
                        }
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
            let queued_id = command_id.clone();
            let live = this.update(cx, |this, cx| {
                // EXP-1626: remembered for the cancel — unless the attempt
                // already moved on, in which case this run is cancelled now.
                if this.attempt == attempt {
                    this.login_command = Some(queued_id);
                } else {
                    queue_login_cancel(&this.device_id, this.agent, &queued_id, cx);
                }
                this.settle(attempt, LoginDialogState::Waiting, cx)
            });
            if !matches!(live, Ok(true)) {
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
                    this.fail(SIGN_IN_TIMED_OUT.into(), cx);
                }
            });
        })
        .detach();
    }

    /// EXP-1626: the live attempt's login has not landed — cancel its
    /// `agent_login` on the machine (once).
    fn cancel_login(&mut self, cx: &mut App) {
        if self.state == LoginDialogState::SignedIn {
            return;
        }
        if let Some(command_id) = self.login_command.take() {
            queue_login_cancel(&self.device_id, self.agent, &command_id, cx);
        }
    }

    /// Write a phase that belongs to the LIVE attempt; an abandoned one (a
    /// retry ran, or the login already landed) writes nothing. Returns
    /// whether it wrote.
    fn settle(&mut self, attempt: u64, state: LoginDialogState, cx: &mut gpui::Context<Self>) -> bool {
        if self.attempt != attempt {
            return false;
        }
        match state {
            LoginDialogState::Failed(message) => self.fail(message, cx),
            state => self.state = state,
        }
        cx.notify();
        true
    }

    /// FEED-79: a failed sign-in is the error toast (Retry re-queues the
    /// SAME login); the dialog keeps only its Try again control.
    fn fail(&mut self, message: SharedString, cx: &mut gpui::Context<Self>) {
        self.state = LoginDialogState::Failed(message.clone());
        let dialog = cx.entity().downgrade();
        crate::action_error::failed_action_with_retry(
            "sign in",
            self.agent.label(),
            Some(message.to_string()),
            move |window, cx| {
                let _ = dialog.update(cx, |this, cx| this.request(window, cx));
            },
            cx,
        );
        cx.notify();
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
            self.fail("Not signed in.".into(), cx);
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
                    this.fail(SIGN_IN_TIMED_OUT.into(), cx);
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

/// EXP-1626 — queue `agent_login_cancel` for `login_command_id` on
/// `device_id` (fire and forget: a failure only warns).
fn queue_login_cancel(device_id: &str, agent: CodingAgent, login_command_id: &str, cx: &mut App) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    let (device_id, login_command_id) = (device_id.to_string(), login_command_id.to_string());
    log::info!("[agent-login] cancelling sign-in {login_command_id} on {device_id}");
    cx.background_executor()
        .spawn(async move {
            if let Err(err) =
                api::devices::create_login_cancel_command(&trpc, &device_id, agent.id(), &login_command_id)
            {
                log::warn!("[agent-login] cancel of sign-in {login_command_id} not queued: {err}");
            }
        })
        .detach();
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
        let _span = tracing::trace_span!("render", view = "LoginDialogView").entered();
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let foreground = theme.foreground;
        let line = match &self.state {
            // EXP-940: the wait SPINS. A line that never moves reads the same
            // as a line that is stuck, which is what this dialog used to be.
            // EXP-1627: ONE muted line, `copy.signInWaiting`, in the web's
            // small size (`text-xs`), WRAPPING — never cut to one line.
            LoginDialogState::Queueing | LoginDialogState::Waiting => h_flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap_2()
                .text_xs()
                .text_color(muted)
                .child(
                    div()
                        .flex_shrink_0()
                        .child(Spinner::new().icon(crate::icons::registry::UI_LOADING)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .whitespace_normal()
                        .child(SharedString::from(sign_in_waiting(
                            &self.device_label,
                            self.agent.label(),
                        ))),
                )
                .into_any_element(),
            // EXP-1000: the code is in; the machine finishes on its own and
            // reports the login on its next heartbeat (web `SIGNING_IN`).
            LoginDialogState::SigningIn => h_flex()
                .w_full()
                .items_center()
                .gap_2()
                .text_xs()
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
                .text_xs()
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
            // reader can only close the window on; the sentence itself was the
            // toast (FEED-79).
            LoginDialogState::Failed(_) => v_flex()
                .w_full()
                .gap_2()
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
        // EXP-1626: a Persist login dialog says what it does as ONE muted
        // line; a browser sign-in has no mode line and no mode button.
        let note = (self.long_lived && self.state != LoginDialogState::SignedIn).then(|| {
            div()
                .w_full()
                .text_xs()
                .text_color(muted)
                .child(crate::machines::COPY_PERSIST_NOTE)
        });
        v_flex().w_full().px_4().pb_4().gap_2().child(line).children(note)
    }
}

/// EXP-909 — "Add account" ON A DEVICE: the dialog is DEVICE-BOUND now. It is
/// only ever opened from a device's own login rows ([`crate::machines`]), so
/// the machine is already chosen and only the agent is left to pick; the
/// device picker went with the cross-device Accounts section that used to be
/// the other way in.
pub(crate) fn open_add_account_dialog_for(device_id: String, window: &mut Window, cx: &mut App) {
    // EXP-1626: one row (the agent) again — the Name row is gone.
    let spec = native_dialog::DialogSpec::new("Add account", size(px(460.), px(212.)));
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
            sign_in_on_device(device.device_id.clone(), device.label.clone(), device.own, agent, None, window, cx);
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
        let _span = tracing::trace_span!("render", view = "AddAccountDialogView").entered();
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

    /// EXP-1627: the waiting line is the fixture's `copy.signInWaiting`,
    /// byte for byte, with the device and the agent filled in.
    #[test]
    fn the_waiting_line_is_the_fixture_copy() {
        assert_eq!(
            sign_in_waiting("mint", CodingAgent::Claude.label()),
            format!("Waiting for mint to publish the {} sign-in link…", CodingAgent::Claude.label())
        );
    }

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

    /// EXP-1626: a cancel names the run by its `agent_login` command id and
    /// stops it unless a code already went in; no run = nothing to stop.
    #[test]
    fn a_cancel_finds_the_run_of_its_login_command() {
        use agent_login::CancelDisposition;
        let remote = |id: &str| RemoteLogin {
            command_id: id.to_string(),
            published: Arc::new(AtomicBool::new(true)),
        };
        let run = Arc::new(LoginRun::new(
            CodingAgent::Claude,
            coding::Settings::default(),
            None,
            None,
            Some(remote("login-1")),
            true,
        ));
        let mut by_command = HashMap::new();
        by_command.insert("login-1".to_string(), Arc::clone(&run));
        let (disposition, found) = cancel_lookup(&by_command, "login-1");
        assert_eq!(disposition, CancelDisposition::Stop);
        assert!(Arc::ptr_eq(&found.unwrap(), &run));
        assert_eq!(cancel_lookup(&by_command, "login-2").0, CancelDisposition::NoRun);
        assert_eq!(run.label(), "sign-in login-1");
        // A code went in: the run is left to finish.
        if let Ok(mut at) = run.last_code_at.lock() {
            *at = Some(Instant::now());
        }
        assert!(run.code_entered());
        assert_eq!(cancel_lookup(&by_command, "login-1").0, CancelDisposition::CodeEntered);
        assert!(run.since_code().is_some_and(|since| since < agent_login::CODE_GRACE));
    }

    /// EXP-1626: every run is bounded at ten minutes.
    #[test]
    fn a_login_run_is_bounded_at_ten_minutes() {
        assert_eq!(REMOTE_URL_TIMEOUT, Duration::from_secs(600));
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
