//! The daemon's `agent_login` executor (EXP-484 Phase D) — the headless
//! twin of the desktop's login tab.
//!
//! A machine cannot be useful while its agent CLIs are signed out, and a
//! headless one has nobody at its keyboard: the fix is to run the agent's
//! OWN sign-in command here, on a PTY nobody watches, and hand the sign-in
//! link (plus Codex's device code) back to whoever asked — through the
//! command's `result`, which the requester is already polling. The person
//! finishes the login in a browser on any device; the machine's row flips
//! signed-in on the doctor re-probe this run schedules on its way out
//! ([`crate::commands::daemon`]'s `doctor_soon`).
//!
//! Shape (deliberately the [`crate::session_host`] wiring minus everything
//! a coding session needs — no publisher, no heartbeat, no registry entry):
//! PTY + emulator + read loop + wait thread, then a 250 ms poll of the grid
//! through [`steer::agent_login_driver::observe_login_screen`]. The FIRST
//! observation that carries a URL completes the command `ok: true`; the run
//! keeps going (the login still has to finish on the machine), and the exit
//! or the [`LOGIN_TIMEOUT`] completes `ok: false` if nothing was ever
//! published. Only the SUCCESS path carries JSON ([`LoginProgress`]): every
//! client routes a failed row's `result` straight into an error caption, so
//! a failure completes with a plain human sentence.
//!
//! **Owner-only by construction**: the server authorizes `devices.
//! createCommand` on the device's OWNER (`devices.userId`), so a command
//! that reaches this executor was queued by the person who owns this
//! machine. Nothing here re-checks it — there is no identity on a pulled
//! command to re-check against.
//!
//! Redelivery is the server's idempotency model (a pending command rides
//! EVERY heartbeat until it is completed), so an id already in flight is a
//! silent no-op — never a second `claude auth login`, and never a
//! completion the in-flight run would then race.
//!
//! EXP-765: claude's link carries `code=true` — the browser page ends by
//! showing an authorization CODE the CLI here is still waiting for ("Paste
//! code here if prompted >"), and nobody can type it at a headless machine.
//! The requester hands it back as an `agent_login_code` command; [`enter_code`]
//! drops it into the [`CodeInbox`] slot the live login registered for its
//! agent, and the poll loop types it into the PTY on its next tick.
//!
//! Every login runs in a fresh STAGING dir (`coding::agent_login::
//! begin_login`) and, once the CLI exits signed in, is COMMITTED into the
//! profile of the email it signed in as (`commit_login`): a known address
//! refreshes that profile, a new one adds a profile. The payload's
//! `profileId` is only the INTENDED profile (the duplicate check); the
//! requester learns where the login landed off the heartbeat (`lastLoginAt`
//! moving), since the command already completed with the link. The doctor
//! re-probe on the way out re-reads the profile index, so a new profile
//! rides the next heartbeat by itself.
//!
//! EXP-1533/1626: `longLived: "true"` (claude, with a `profileId`) PERSISTS
//! that profile's login: `claude setup-token` in a fresh STAGING dir (never
//! inside the profile, so the browser login it holds is never touched), the
//! same link and code hand-off, then the one-year token it prints is
//! scraped off the grid and attached to the profile
//! ([`agent_login::commit_setup_token`]) and the staging dir dropped.
//! Nothing is committed by email, and the command's result never carries
//! the token. `longLived` with NO `profileId` (an older client's long-lived
//! Add, EXP-1600) still works: `claude setup-token` in a fresh staging dir,
//! whose token then makes that dir a new profile
//! ([`agent_login::commit_setup_token_add`]); no token = abandoned.
//!
//! The retired `import: "true"` (EXP-1532: nothing ever copies the
//! machine's AMBIENT login into a profile) is refused by the payload parse:
//! the command completes ok:false with `agent_login::IMPORT_REMOVED`.
//!
//! EXP-1626: every run is bounded (`agent_login::LOGIN_RUN_TIMEOUT` after
//! it started, past a fresh code's `CODE_GRACE`) and ended for sure (the
//! PTY child gets SIGTERM, then SIGKILL: `claude setup-token` outlived the
//! plain SIGHUP). A long-lived run answers its code command on the OUTCOME
//! (`agent_login::PendingCode`), a browser run at the keystroke. An
//! `agent_login_cancel` ([`cancel`]) stops the run of the `agent_login` it
//! names unless a code already went in.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use coding::agent_login::{self, LoginProgress};
use coding::{CodingAgent, Settings};
use steer::agent_login_driver::{observe_login_screen, LoginObservation};
use terminal::emulator::Emulator;
use terminal::pty;
use terminal::read_loop::spawn_read_loop;

use crate::context::Ctx;

/// Detached grid size — [`crate::session_host`]'s, for the same reason: the
/// grid watchers want a roomy, realistic terminal.
const COLS: u16 = 120;
const ROWS: u16 = 36;

/// How often the grid is read. Same cadence as the desktop's login poll.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// EXP-765: one code the requester handed back, with the `agent_login_code`
/// command the run answers once it typed it (EXP-1626: a long-lived run
/// answers on the outcome).
pub struct CodeDelivery {
    pub code: String,
    pub command_id: String,
}

/// EXP-1626: what [`cancel`] reaches of a live run.
#[derive(Clone, Default)]
struct RunControl {
    /// Raised by a cancel: the run stops on its next tick.
    cancel: Arc<AtomicBool>,
    /// A code went into the run (a cancel then leaves it alone).
    code_entered: Arc<AtomicBool>,
}

/// The live logins: EXP-765's code slot per agent id (the sender a LIVE
/// login polls for the code the requester hands back) and EXP-1626's runs
/// by `agent_login` command id (what a cancel names). Registered by
/// [`run`]'s thread for the login's lifetime.
#[derive(Default)]
pub struct LoginInbox {
    codes: HashMap<String, flume::Sender<CodeDelivery>>,
    runs: HashMap<String, RunControl>,
}

pub type CodeInbox = Arc<Mutex<LoginInbox>>;

/// The two sentences a code command completes with — the clients show a
/// failed row's `result` verbatim, and the success line is byte-identical on
/// the desktop executor (`ui::agent_login`).
///
/// EXP-940: the success one is the phase the requester is IN — the ×4
/// `SIGNING_IN` string — not a sentence about a machine finishing something.
pub const CODE_ENTERED: &str = "Signing in…";
pub const NO_LOGIN_WAITING: &str = "No sign-in is waiting for a code on this machine.";

/// EXP-765: run one `agent_login_code` command — hand the code to the login
/// that is waiting for it. The RUN answers it (EXP-1626): a browser login
/// once the code is typed, a long-lived one on its outcome; with no login
/// waiting it completes here, at once.
pub fn enter_code(ctx: &Ctx, command: &api::devices::PendingCommand, codes: &CodeInbox) {
    let agent = command.payload["agent"].as_str().unwrap_or_default();
    let code = command.payload["code"].as_str().unwrap_or_default().trim();
    if CodingAgent::parse(agent).is_none() || code.is_empty() {
        complete(ctx, &command.id, false, "Malformed command payload.");
        return;
    }
    let sender = codes
        .lock()
        .ok()
        .and_then(|inbox| inbox.codes.get(agent).cloned());
    let delivered = matches!(sender, Some(sender) if sender
        .send(CodeDelivery { code: code.to_string(), command_id: command.id.clone() })
        .is_ok());
    if !delivered {
        complete(ctx, &command.id, false, NO_LOGIN_WAITING);
    }
}

/// EXP-1626 — what a cancel of `login_command_id` does, and the run it
/// reaches.
fn cancel_lookup(
    inbox: &LoginInbox,
    login_command_id: &str,
) -> (agent_login::CancelDisposition, Option<RunControl>) {
    let run = inbox.runs.get(login_command_id).cloned();
    let disposition = agent_login::CancelDisposition::of(
        run.is_some(),
        run.as_ref().is_some_and(|run| run.code_entered.load(Ordering::SeqCst)),
    );
    (disposition, run)
}

/// EXP-1626: run one `agent_login_cancel` command — the requester closed
/// its sign-in dialog before the login landed. The named run is stopped
/// (its child killed, its staging dropped) unless a code already went in;
/// the cancel completes ok either way, and with no such run.
pub fn cancel(ctx: &Ctx, command: &api::devices::PendingCommand, codes: &CodeInbox) {
    let login_command_id = match agent_login::parse_cancel_payload(&command.payload) {
        Ok(id) => id,
        Err(message) => {
            complete(ctx, &command.id, false, &message);
            return;
        }
    };
    let (disposition, run) = match codes.lock() {
        Ok(inbox) => cancel_lookup(&inbox, &login_command_id),
        Err(_) => (agent_login::CancelDisposition::NoRun, None),
    };
    log::info!("agent_login: cancel {} of sign-in {login_command_id}: {disposition:?}", command.id);
    if let (agent_login::CancelDisposition::Stop, Some(run)) = (disposition, run) {
        run.cancel.store(true, Ordering::SeqCst);
    }
    complete(ctx, &command.id, true, disposition.message());
}

/// Start one `agent_login` command. Validation and the dedupe claim happen
/// on the caller's (device-worker) thread — everything that can block moves
/// to a thread of its own.
pub fn run(
    ctx: &Ctx,
    settings: Settings,
    command: api::devices::PendingCommand,
    inflight: Arc<Mutex<HashSet<String>>>,
    codes: CodeInbox,
    doctor_soon: Arc<AtomicBool>,
) {
    // An unknown agent, a malformed payload or the retired import is refused
    // with the sentence the clients show verbatim.
    let request = match agent_login::parse_login_payload(&command.payload) {
        Ok(request) => request,
        Err(message) => {
            complete(ctx, &command.id, false, &message);
            return;
        }
    };
    let agent = request.agent;
    let target = request.target;
    let long_lived = request.long_lived;

    // The redelivery gate. Claimed here, on the serialized worker, so two
    // pulls of the same id can never both pass it.
    {
        let Ok(mut guard) = inflight.lock() else {
            complete(ctx, &command.id, false, "This machine could not start the sign-in.");
            return;
        };
        if !guard.insert(command.id.clone()) {
            log::debug!("agent_login {} already in flight — ignoring the redelivery", command.id);
            return;
        }
    }

    let trpc = Arc::clone(&ctx.trpc);
    let data_dir = ctx.data_dir.clone();
    let command_id = command.id.clone();
    let claimed = Arc::clone(&inflight);
    let thread = std::thread::Builder::new()
        .name("exp-agent-login".to_string())
        .spawn(move || {
            // EXP-1626: what a cancel of this command reaches.
            let control = RunControl::default();
            if let Ok(mut inbox) = codes.lock() {
                inbox.runs.insert(command_id.clone(), control.clone());
            }
            let outcome = match (long_lived, target.intended()) {
                (true, Some(profile)) => sign_in_long_lived(
                    &trpc, &settings, &data_dir, profile, &command_id, &codes, &control,
                ),
                (true, None) => {
                    add_long_lived(&trpc, &settings, &data_dir, &command_id, &codes, &control)
                }
                _ => sign_in(
                    &trpc,
                    &settings,
                    &data_dir,
                    agent,
                    target.intended(),
                    &command_id,
                    &codes,
                    &control,
                ),
            };
            if let Ok(mut inbox) = codes.lock() {
                inbox.runs.remove(&command_id);
            }
            if let Some((ok, message)) = outcome {
                complete_with(&trpc, &command_id, ok, &message);
            }
            if let Ok(mut guard) = inflight.lock() {
                guard.remove(&command_id);
            }
            // Whatever happened, what the machine's agents look like just
            // changed (or was meant to) — re-probe on the next tick. (A
            // committed login already dropped ITS usage cache entry, so the
            // account is read afresh on the very next pass.)
            doctor_soon.store(true, Ordering::SeqCst);
        });
    if let Err(err) = thread {
        log::warn!("agent_login: could not spawn the login thread: {err}");
        if let Ok(mut guard) = claimed.lock() {
            guard.remove(&command.id);
        }
        complete(ctx, &command.id, false, "This machine could not start the sign-in.");
    }
}

/// One remote sign-in: fetch a managed Codex if needed, run the CLI's login
/// in a fresh staging dir, and COMMIT it by email once the CLI exits signed
/// in. Returns the completion still owed, `None` when the command already
/// completed with the link.
#[allow(clippy::too_many_arguments)]
fn sign_in(
    trpc: &Arc<api::trpc::TrpcClient>,
    settings: &Settings,
    data_dir: &std::path::Path,
    agent: CodingAgent,
    intended: Option<&str>,
    command_id: &str,
    codes: &CodeInbox,
    control: &RunControl,
) -> Option<(bool, String)> {
    // EXP-1232: a managed Codex sign-in fetches the pinned build first; a
    // failed fetch is the completion.
    if let Err(message) = coding::managed_codex::ensure_for(settings, data_dir, agent) {
        return Some((false, message));
    }
    let (staging, env) = match agent_login::begin_login(data_dir, agent) {
        Ok(started) => started,
        Err(message) => return Some((false, message)),
    };
    // EXP-765: the slot the requester's code lands in while this login runs.
    // Keyed by agent — one login per agent at a time is what the server's
    // pending-dedupe already guarantees.
    // Always a remote sign-in here (EXP-695): the daemon must never pop a
    // browser on the machine — the requester opens the published link.
    let mut plan = agent_login::login_plan(settings, agent, true);
    plan.spawn.env.push(env);
    let run = drive_with_inbox(trpc, &plan, agent, command_id, codes, control, false);
    if !run.signed_in {
        agent_login::abandon_login(agent, &staging);
        return run.completion;
    }
    match agent_login::commit_login(settings, data_dir, agent, &staging, intended) {
        Ok(commit) => {
            match commit.duplicate_warning() {
                Some(warning) => log::info!("agent_login: {warning}"),
                None => log::info!("agent_login: {} signed in ({})", agent.id(), commit.profile_id),
            }
            run.completion.map(|(_, message)| (true, message))
        }
        Err(message) => {
            log::warn!("agent_login: the sign-in did not commit: {message}");
            run.completion.map(|_| (false, message))
        }
    }
}

/// EXP-1533/1626 — one remote PERSIST of claude profile `profile`'s login:
/// `claude setup-token` in a fresh staging dir, driven like any sign-in,
/// then the token it printed is attached to the profile and the staging dir
/// dropped (either way). Returns the completion still owed, `None` when the
/// command already completed with the link.
fn sign_in_long_lived(
    trpc: &Arc<api::trpc::TrpcClient>,
    settings: &Settings,
    data_dir: &std::path::Path,
    profile: &str,
    command_id: &str,
    codes: &CodeInbox,
    control: &RunControl,
) -> Option<(bool, String)> {
    let agent = CodingAgent::Claude;
    let (staging, plan) = match agent_login::long_lived_plan(settings, data_dir, profile, true) {
        Ok(started) => started,
        Err(message) => return Some((false, message)),
    };
    let run = drive_with_inbox(trpc, &plan, agent, command_id, codes, control, true);
    let Some(scraped) = run.setup_token else {
        log::info!("agent_login: the persist sign-in ended without a token ({profile})");
        agent_login::abandon_login(agent, &staging);
        return run.completion;
    };
    match agent_login::commit_setup_token(data_dir, profile, &staging, scraped) {
        Ok(expires_at) => {
            log::info!("agent_login: claude login persisted ({profile}, until {expires_at})");
            run.completion.map(|_| (true, "Signed in".to_string()))
        }
        Err(message) => {
            log::warn!("agent_login: {message}");
            run.completion.map(|_| (false, message))
        }
    }
}

/// EXP-1600 — one remote long-lived "Add account": `claude setup-token` in
/// a fresh staging dir, driven like any sign-in; the token it printed makes
/// that dir a new profile, no token abandons it. Returns the completion
/// still owed, `None` when the command already completed with the link.
/// EXP-1626: an older client's only (current clients add with the browser
/// login and persist it afterwards).
fn add_long_lived(
    trpc: &Arc<api::trpc::TrpcClient>,
    settings: &Settings,
    data_dir: &std::path::Path,
    command_id: &str,
    codes: &CodeInbox,
    control: &RunControl,
) -> Option<(bool, String)> {
    let agent = CodingAgent::Claude;
    let (staging, plan) = match agent_login::long_lived_add_plan(settings, data_dir, true) {
        Ok(started) => started,
        Err(message) => return Some((false, message)),
    };
    let run = drive_with_inbox(trpc, &plan, agent, command_id, codes, control, true);
    let Some(scraped) = run.setup_token else {
        log::info!("agent_login: the long-lived Add ended without a token");
        agent_login::abandon_login(agent, &staging);
        return run.completion;
    };
    match agent_login::commit_setup_token_add(data_dir, &staging, scraped) {
        Ok((profile, expires_at)) => {
            log::info!("agent_login: claude long-lived login added ({profile}, until {expires_at})");
            run.completion.map(|_| (true, "Signed in".to_string()))
        }
        Err(message) => {
            log::warn!("agent_login: {message}");
            run.completion.map(|_| (false, message))
        }
    }
}

/// [`drive`] with the agent's code slot registered for the run's lifetime.
fn drive_with_inbox(
    trpc: &Arc<api::trpc::TrpcClient>,
    plan: &agent_login::LoginPlan,
    agent: CodingAgent,
    command_id: &str,
    codes: &CodeInbox,
    control: &RunControl,
    scrape_token: bool,
) -> DriveOutcome {
    // EXP-765: the slot the requester's code lands in while this login runs.
    // Keyed by agent — one login per agent at a time is what the server's
    // pending-dedupe already guarantees.
    let (code_tx, code_rx) = flume::unbounded::<CodeDelivery>();
    if let Ok(mut inbox) = codes.lock() {
        inbox.codes.insert(agent.id().to_string(), code_tx.clone());
    }
    let run = drive(trpc, plan, agent, command_id, &code_rx, control, scrape_token);
    if let Ok(mut inbox) = codes.lock() {
        // Only OUR slot — a login started after this one exited must keep
        // its own.
        if inbox.codes.get(agent.id()).is_some_and(|tx| tx.same_channel(&code_tx)) {
            inbox.codes.remove(agent.id());
        }
    }
    // EXP-1626: a code that arrived as the run ended was never typed — it
    // is answered, never left pending.
    while let Ok(late) = code_rx.try_recv() {
        complete_with(trpc, &late.command_id, false, agent_login::SIGN_IN_ENDED);
    }
    run
}

/// What [`drive`] saw.
struct DriveOutcome {
    /// The completion still owed (`None` = completed early with the link).
    completion: Option<(bool, String)>,
    /// The CLI exited cleanly: there is a login to commit.
    signed_in: bool,
    /// EXP-1533: the setup token the grid showed last (a long-lived run).
    setup_token: Option<(String, Option<i64>)>,
}

/// Run the login `plan` to its end (its env points the CLI's config-dir
/// variable at the run's staging dir — EXP-1626: a persist run's too,
/// never a profile's).
/// `scrape_token` = watch the grid for the token `claude setup-token` prints
/// (a long-lived run: its code is answered on the outcome).
fn drive(
    trpc: &Arc<api::trpc::TrpcClient>,
    plan: &agent_login::LoginPlan,
    agent: CodingAgent,
    command_id: &str,
    code_rx: &flume::Receiver<CodeDelivery>,
    control: &RunControl,
    scrape_token: bool,
) -> DriveOutcome {
    let mut emulator = Emulator::new(COLS, ROWS);
    let mut pty = match pty::open(&plan.spawn, COLS, ROWS) {
        Ok(pty) => pty,
        Err(err) => {
            log::warn!("agent_login: {} would not start: {err:#}", agent.id());
            return DriveOutcome {
                completion: Some((false, format!("Could not start {} on this machine.", agent.id()))),
                signed_in: false,
                setup_token: None,
            };
        }
    };
    let (wake_tx, wake_rx) = flume::unbounded();
    let read_thread = spawn_read_loop(pty.take_reader(), emulator.term(), wake_tx.clone());
    let wait = pty.spawn_wait_thread(move || {
        let _ = wake_tx.try_send(terminal::Wake::ChildExited);
    });
    let exit_slot = match wait {
        Ok((slot, _thread)) => Some(slot),
        Err(err) => {
            log::warn!("agent_login: no wait thread: {err:#}");
            None
        }
    };

    let started = Instant::now();
    let mut published = false;
    let mut method_picker_answered = false;
    let mut failure: Option<String> = None;
    // Dropping a `Pty` does NOT end the child (the read loop holds a dup'd
    // master, `terminal::pty::open`) — every path out of this loop that is
    // not a real child exit has to kill, or a signed-out `codex login` and
    // its two threads linger on the machine forever.
    let mut exited = false;
    let mut clean_exit = false;
    let mut setup_token = None;
    // EXP-1626: a long-lived run's code in flight, and when one last went in.
    let mut pending_code: Option<agent_login::PendingCode> = None;
    let mut last_code_at: Option<Instant> = None;
    let mut cancelled = false;
    loop {
        std::thread::sleep(POLL_INTERVAL);
        // Nobody paints this grid — the wakes exist only to keep the read
        // loop's channel from growing.
        while wake_rx.try_recv().is_ok() {}
        // Query replies (DA/DSR) must reach the child or the CLI hangs
        // before it ever prints a link.
        let _ = emulator.drain_events(&mut |reply| pty.writer_write(reply));
        let lines = emulator.screen_lines();
        if scrape_token {
            if let Some(found) = agent_login::scrape_setup_token(&lines) {
                setup_token = Some(found);
            }
        }
        let observation = observe_login_screen(agent.id(), &lines);

        if !published {
            match &observation {
                LoginObservation::Url { url, code } => {
                    let progress = LoginProgress::url(agent, url.clone(), code.clone());
                    complete_with(trpc, command_id, true, &progress.to_result_text());
                    log::info!("agent_login: {command_id} published the sign-in link");
                    published = true;
                }
                LoginObservation::MethodPicker if !method_picker_answered => {
                    // Defensive: the launch flags pick the method outright,
                    // but a CLI that starts asking would otherwise hang with
                    // nobody at the keyboard.
                    method_picker_answered = true;
                    pty.writer_write(b"\r");
                }
                LoginObservation::Failed(message) => {
                    // Falls through to the kill below — a failed login
                    // screen sits there waiting for a keypress otherwise.
                    failure = Some(message.clone());
                    break;
                }
                LoginObservation::MethodPicker | LoginObservation::Nothing => {}
            }
        }
        // EXP-1626: a long-lived run's code completes on its outcome; a
        // refused code ends the run (the requester's retry starts afresh).
        if let Some(pending) = pending_code.as_mut() {
            let refusal = match &observation {
                LoginObservation::Failed(message) => Some(message.as_str()),
                _ => None,
            };
            if let Some((ok, message)) = pending.observe(setup_token.is_some(), refusal) {
                log::info!("agent_login: code {} settled: ok={ok}", pending.command_id);
                complete_with(trpc, &pending.command_id, ok, &message);
                pending_code = None;
                if !ok {
                    failure = Some(message);
                    break;
                }
            }
        }
        // EXP-765: the code the requester handed back — typed as the one
        // line the CLI's "Paste code here" prompt is waiting for. Written
        // whether or not the URL was observed: the prompt is the CLI's
        // business, and a code with nobody waiting is harmless input.
        while let Ok(delivery) = code_rx.try_recv() {
            let failure_on_screen = matches!(observation, LoginObservation::Failed(_));
            pty.writer_write(format!("{}\r", delivery.code).as_bytes());
            // Never the code itself.
            log::info!("agent_login: code {} typed into {command_id}", delivery.command_id);
            control.code_entered.store(true, Ordering::SeqCst);
            last_code_at = Some(Instant::now());
            if scrape_token {
                let next = agent_login::PendingCode::new(delivery.command_id, failure_on_screen);
                if let Some(replaced) = pending_code.replace(next) {
                    complete_with(trpc, &replaced.command_id, false, "A newer code replaced this one.");
                }
            } else {
                complete_with(trpc, &delivery.command_id, true, CODE_ENTERED);
            }
        }

        let exit = exit_slot
            .as_ref()
            .and_then(|slot| slot.lock().ok().and_then(|slot| slot.clone()));
        if let Some(exit) = exit {
            exited = true;
            clean_exit = exit.success;
            break;
        }
        // EOF with no reaped exit: the child closed the PTY (or double-
        // forked) — stop watching a grid nobody writes to any more.
        if read_thread.is_finished() {
            break;
        }
        // EXP-1626: the requester walked away.
        if control.cancel.load(Ordering::SeqCst) {
            log::info!("agent_login: {command_id} cancelled by the requester — stopping the login");
            cancelled = true;
            break;
        }
        if agent_login::login_run_expired(started.elapsed(), last_code_at.map(|at| at.elapsed())) {
            log::info!("agent_login: {command_id} ({}) timed out — stopping the login", agent.id());
            break;
        }
    }
    // A published link leaves the CLI waiting for the browser, and that is
    // fine WHILE we watch it — but nothing outlives this function. EXP-1626:
    // SIGHUP alone did not end `claude setup-token`: SIGTERM, then SIGKILL.
    if !exited {
        match exit_slot.as_ref() {
            Some(slot) => pty.terminator(Arc::clone(slot)).terminate(),
            None => pty.kill(),
        }
    }
    // EXP-1533: the token is the CLI's LAST output — one more read once the
    // read loop has drained it, or an exit right after printing loses it.
    if scrape_token {
        std::thread::sleep(POLL_INTERVAL);
        if let Some(found) = agent_login::scrape_setup_token(&emulator.screen_lines()) {
            setup_token = Some(found);
        }
    }
    // EXP-1626: a code still in flight is answered by whether a token came.
    if let Some(pending) = pending_code.take() {
        let (ok, message) = pending.ended(setup_token.is_some());
        log::info!("agent_login: code {} settled at the end: ok={ok}", pending.command_id);
        complete_with(trpc, &pending.command_id, ok, &message);
    }

    let signed_in = clean_exit && failure.is_none();
    if published || cancelled {
        // Already completed the moment the link appeared — or the server
        // failed the cancelled row itself.
        return DriveOutcome { completion: None, signed_in, setup_token };
    }
    // A failed row's `result` IS the error caption on every client — plain
    // text, never the JSON the success path publishes. (A clean exit with no
    // link — a login that needed none — is settled by the commit.)
    DriveOutcome {
        completion: Some((
            false,
            failure.unwrap_or_else(|| "The sign-in ended before a link appeared".to_string()),
        )),
        signed_in,
        setup_token,
    }
}

fn complete(ctx: &Ctx, command_id: &str, ok: bool, message: &str) {
    complete_with(&ctx.trpc, command_id, ok, message);
}

/// EXP-1626: a failed call and a row no longer pending both warn.
fn complete_with(trpc: &Arc<api::trpc::TrpcClient>, command_id: &str, ok: bool, message: &str) {
    match api::devices::complete_command(trpc, command_id, ok, Some(message)) {
        Ok(true) => {}
        Ok(false) => log::warn!("agent_login: completeCommand {command_id}: the row was no longer pending"),
        Err(err) => log::warn!("agent_login: completeCommand {command_id} failed: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dedupe gate is the whole defence against the heartbeat's
    /// redelivery starting a second sign-in every 30 seconds.
    #[test]
    fn an_id_in_flight_is_claimed_exactly_once() {
        let inflight: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
        assert!(inflight.lock().unwrap().insert("cmd-1".to_string()));
        assert!(!inflight.lock().unwrap().insert("cmd-1".to_string()));
        inflight.lock().unwrap().remove("cmd-1");
        assert!(inflight.lock().unwrap().insert("cmd-1".to_string()));
    }

    /// EXP-765: a code lands in the live login's slot and nowhere else —
    /// with no login waiting, the send has no receiver.
    #[test]
    fn a_code_reaches_only_the_live_login_slot() {
        let codes = CodeInbox::default();
        let (tx, rx) = flume::unbounded::<CodeDelivery>();
        codes.lock().unwrap().codes.insert("claude".to_string(), tx.clone());
        let slot = codes.lock().unwrap().codes.get("claude").cloned().unwrap();
        let delivery = |code: &str| CodeDelivery { code: code.to_string(), command_id: "c-1".to_string() };
        assert!(slot.send(delivery("abc#xyz")).is_ok());
        let got = rx.try_recv().unwrap();
        assert_eq!((got.code.as_str(), got.command_id.as_str()), ("abc#xyz", "c-1"));
        assert!(codes.lock().unwrap().codes.get("codex").is_none());
        // The run's exit removes only ITS slot.
        let (other, _other_rx) = flume::unbounded::<CodeDelivery>();
        assert!(!codes.lock().unwrap().codes.get("claude").unwrap().same_channel(&other));
        assert!(codes.lock().unwrap().codes.get("claude").unwrap().same_channel(&tx));
        // Once the receiver is gone the send fails — the executor answers
        // "no sign-in waiting" instead of a completion that lies.
        drop(rx);
        assert!(tx.send(delivery("late")).is_err());
    }

    /// EXP-1626: a cancel finds the run by its `agent_login` command id and
    /// stops it unless a code went in; an unknown id is nothing to stop.
    #[test]
    fn a_cancel_reaches_the_run_of_its_login_command() {
        use coding::agent_login::CancelDisposition;
        let mut inbox = LoginInbox::default();
        let run = RunControl::default();
        inbox.runs.insert("login-1".to_string(), run.clone());
        let (disposition, found) = cancel_lookup(&inbox, "login-1");
        assert_eq!(disposition, CancelDisposition::Stop);
        found.unwrap().cancel.store(true, Ordering::SeqCst);
        assert!(run.cancel.load(Ordering::SeqCst), "the live run sees the flag");
        assert_eq!(cancel_lookup(&inbox, "login-2").0, CancelDisposition::NoRun);
        run.code_entered.store(true, Ordering::SeqCst);
        assert_eq!(cancel_lookup(&inbox, "login-1").0, CancelDisposition::CodeEntered);
    }

    /// An unknown agent never reaches a PTY, with the sentence the clients
    /// show verbatim. The same parse reads the target: Add account, an
    /// intended profile; the retired import is refused (EXP-1532).
    #[test]
    fn only_claude_and_codex_are_runnable_agents() {
        use coding::agent_login::{parse_login_payload, LoginTarget};
        // EXP-849: a retired agent id is simply unknown now.
        assert_eq!(
            parse_login_payload(&serde_json::json!({"agent": "pi", "switch": "false"})),
            Err("This machine does not know that agent.".to_string())
        );
        assert!(parse_login_payload(&serde_json::json!({"switch": "false"})).is_err());
        let claude = parse_login_payload(&serde_json::json!({"agent": "claude", "switch": "true"}))
            .unwrap();
        assert_eq!(claude.agent, CodingAgent::Claude);
        assert_eq!(claude.target, LoginTarget::Add);
        let codex = parse_login_payload(&serde_json::json!({
            "agent": "codex", "switch": "false", "profileId": "0badf00d"
        }))
        .unwrap();
        assert_eq!(codex.agent, CodingAgent::Codex);
        assert_eq!(codex.target, LoginTarget::Profile("0badf00d".to_string()));
        assert_eq!(
            parse_login_payload(&serde_json::json!({"agent": "claude", "import": "true"})),
            Err("Import was removed: sign this account in on this device instead.".to_string())
        );
        // EXP-1600: a long-lived Add is a claude sign-in the host runs;
        // codex refuses it.
        let add = parse_login_payload(&serde_json::json!({"agent": "claude", "longLived": "true"}))
            .unwrap();
        assert!(add.long_lived);
        assert_eq!(add.target, LoginTarget::Add);
        assert_eq!(
            parse_login_payload(&serde_json::json!({"agent": "codex", "longLived": "true"})),
            Err("A long-lived login is Claude only.".to_string())
        );
        // An unknown agent is refused before its target is looked at.
        assert!(parse_login_payload(&serde_json::json!({
            "agent": "gemini", "import": "true"
        }))
        .is_err());
    }
}
