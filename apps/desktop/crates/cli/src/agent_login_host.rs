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
//! EXP-827: the payload may name an account PROFILE (`profileId`, an
//! existing one) or ask for a new one (`newProfileLabel`); the run points
//! the CLI's config-dir variable at that profile's dir for the logout and
//! the login (`coding::agent_login::parse_login_payload` +
//! `resolve_login_profile`), and the published result names the id it
//! signed into. The doctor re-probe on the way out re-reads the profile
//! index, so a freshly created profile rides the next heartbeat by itself.

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

/// How long a sign-in may run before the PTY is killed. A device-auth link
/// expires long before this; a login nobody finishes must not hold an agent
/// CLI open on the machine forever.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// EXP-765: one slot per agent id — the sender a LIVE login polls for the
/// code the requester hands back. Registered by [`run`]'s thread for the
/// login's lifetime, read by [`enter_code`].
pub type CodeInbox = Arc<Mutex<HashMap<String, flume::Sender<String>>>>;

/// The two sentences a code command completes with — the clients show a
/// failed row's `result` verbatim, and the success line is byte-identical on
/// the desktop executor (`ui::agent_login`).
pub const CODE_ENTERED: &str = "Code entered — the machine is finishing the sign-in.";
pub const NO_LOGIN_WAITING: &str = "No sign-in is waiting for a code on this machine.";

/// EXP-765: run one `agent_login_code` command — type the code into the
/// login that is waiting for it. Completes at once, either way: there is
/// nothing to watch after the write (the login's own run reports the exit),
/// and a redelivery of a completed id never reaches this executor.
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
        .and_then(|inbox| inbox.get(agent).cloned());
    let delivered = matches!(sender, Some(sender) if sender.send(code.to_string()).is_ok());
    if delivered {
        complete(ctx, &command.id, true, CODE_ENTERED);
    } else {
        complete(ctx, &command.id, false, NO_LOGIN_WAITING);
    }
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
    // pi's `/login` is a slash command inside its TUI that opens a provider
    // flow with no remote-finishable handle (the server refuses it too —
    // this is the belt to that suspenders); the parser refuses it with the
    // sentence the clients show verbatim.
    let request = match agent_login::parse_login_payload(&command.payload) {
        Ok(request) => request,
        Err(message) => {
            complete(ctx, &command.id, false, &message);
            return;
        }
    };
    let agent = request.agent;
    let switch = request.switch;
    let target = request.target;

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
            // EXP-827: the profile this login lands on, created here when
            // the payload asked for a new one. A refused target (unknown
            // id, pi) is the completion; nothing is spawned.
            let outcome = match agent_login::resolve_login_profile(&data_dir, agent, &target) {
                Err(message) => Some((false, message)),
                Ok(profile_id) => {
                    let env = agent_login::login_env(&data_dir, agent, &profile_id);
                    // EXP-765: the slot the requester's code lands in while
                    // this login runs. Keyed by agent — one login per agent
                    // at a time is what the server's pending-dedupe already
                    // guarantees.
                    let (code_tx, code_rx) = flume::unbounded::<String>();
                    if let Ok(mut inbox) = codes.lock() {
                        inbox.insert(agent.id().to_string(), code_tx.clone());
                    }
                    let outcome = drive(
                        &trpc,
                        &settings,
                        agent,
                        switch,
                        &profile_id,
                        env.as_ref(),
                        &command_id,
                        &code_rx,
                    );
                    if let Ok(mut inbox) = codes.lock() {
                        // Only OUR slot — a login started after this one
                        // exited must keep its own.
                        if inbox.get(agent.id()).is_some_and(|tx| tx.same_channel(&code_tx)) {
                            inbox.remove(agent.id());
                        }
                    }
                    outcome
                }
            };
            if let Some((ok, message)) = outcome {
                complete_with(&trpc, &command_id, ok, &message);
            }
            if let Ok(mut guard) = inflight.lock() {
                guard.remove(&command_id);
            }
            // A switch leaves the OLD account cached (email, plan, numbers)
            // behind its poll backoff — up to 10 minutes of naming the
            // person who just signed out. Drop this agent's entry so the
            // next collect asks afresh.
            coding::usage_cache::forget(&data_dir, agent.id());
            // Whatever happened, what the machine's agents look like just
            // changed (or was meant to) — re-probe on the next tick.
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

/// Run the login to its end. Returns the completion to post, or `None` when
/// the command was already completed early (the URL went out).
///
/// EXP-827: `profile_id` names the account the login lands on and rides the
/// published result; `env` is that profile's config-dir pair (`None` for
/// the ambient login), set on the logout AND the login so both act inside
/// the same profile dir.
#[allow(clippy::too_many_arguments)]
fn drive(
    trpc: &Arc<api::trpc::TrpcClient>,
    settings: &Settings,
    agent: CodingAgent,
    switch: bool,
    profile_id: &str,
    env: Option<&(String, String)>,
    command_id: &str,
    code_rx: &flume::Receiver<String>,
) -> Option<(bool, String)> {
    // A switch signs OUT first — otherwise every agent CLI here would just
    // report the account already signed in and exit.
    if switch {
        if let Err(err) = agent_login::logout_in(settings, agent, env) {
            // Not fatal: the login below may still prompt.
            log::info!("agent_login: sign-out before the switch failed: {err}");
        }
    }

    // Always a remote sign-in here (EXP-695): the daemon must never pop a
    // browser on the machine — the requester opens the published link.
    let mut plan = agent_login::login_plan(settings, agent, true);
    if let Some((key, value)) = env {
        plan.spawn.env.push((key.clone(), value.clone()));
    }
    let mut emulator = Emulator::new(COLS, ROWS);
    let mut pty = match pty::open(&plan.spawn, COLS, ROWS) {
        Ok(pty) => pty,
        Err(err) => {
            log::warn!("agent_login: {} would not start: {err:#}", agent.id());
            return Some((
                false,
                format!("Could not start {} on this machine.", agent.id()),
            ));
        }
    };
    let (wake_tx, wake_rx) = flume::unbounded();
    let read_thread = spawn_read_loop(pty.take_reader(), emulator.term(), wake_tx.clone());
    let wait = pty.spawn_wait_thread(wake_tx);
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
    loop {
        std::thread::sleep(POLL_INTERVAL);
        // Nobody paints this grid — the wakes exist only to keep the read
        // loop's channel from growing.
        while wake_rx.try_recv().is_ok() {}
        // Query replies (DA/DSR) must reach the child or the CLI hangs
        // before it ever prints a link.
        let _ = emulator.drain_events(&mut |reply| pty.writer_write(reply));
        let lines = emulator.screen_lines();

        if !published {
            match observe_login_screen(agent.id(), &lines) {
                LoginObservation::Url { url, code } => {
                    let progress = LoginProgress::url(agent, url, code).with_profile(profile_id);
                    complete_with(trpc, command_id, true, &progress.to_result_text());
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
                    failure = Some(message);
                    break;
                }
                LoginObservation::MethodPicker | LoginObservation::Nothing => {}
            }
        }
        // EXP-765: the code the requester handed back — typed as the one
        // line the CLI's "Paste code here" prompt is waiting for. Written
        // whether or not the URL was observed: the prompt is the CLI's
        // business, and a code with nobody waiting is harmless input.
        while let Ok(code) = code_rx.try_recv() {
            pty.writer_write(format!("{code}\r").as_bytes());
        }

        exited = exit_slot
            .as_ref()
            .and_then(|slot| slot.lock().ok().map(|slot| slot.is_some()))
            .unwrap_or(false);
        if exited {
            break;
        }
        // EOF with no reaped exit: the child closed the PTY (or double-
        // forked) — stop watching a grid nobody writes to any more.
        if read_thread.is_finished() {
            break;
        }
        if started.elapsed() >= LOGIN_TIMEOUT {
            log::info!("agent_login: {} timed out — killing the login", agent.id());
            break;
        }
    }
    // A published link leaves the CLI waiting for the browser, and that is
    // fine WHILE we watch it — but nothing outlives this function.
    if !exited {
        pty.kill();
    }

    if published {
        // Already completed the moment the link appeared.
        return None;
    }
    // A failed row's `result` IS the error caption on every client — plain
    // text, never the JSON the success path publishes.
    Some((
        false,
        failure.unwrap_or_else(|| "The sign-in ended before a link appeared".to_string()),
    ))
}

fn complete(ctx: &Ctx, command_id: &str, ok: bool, message: &str) {
    complete_with(&ctx.trpc, command_id, ok, message);
}

fn complete_with(trpc: &Arc<api::trpc::TrpcClient>, command_id: &str, ok: bool, message: &str) {
    if let Err(err) = api::devices::complete_command(trpc, command_id, ok, Some(message)) {
        log::debug!("completeCommand for the sign-in failed: {err}");
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
        let codes: CodeInbox = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = flume::unbounded::<String>();
        codes.lock().unwrap().insert("claude".to_string(), tx.clone());
        let slot = codes.lock().unwrap().get("claude").cloned().unwrap();
        assert!(slot.send("abc#xyz".to_string()).is_ok());
        assert_eq!(rx.try_recv().unwrap(), "abc#xyz");
        assert!(codes.lock().unwrap().get("codex").is_none());
        // The run's exit removes only ITS slot.
        let (other, _other_rx) = flume::unbounded::<String>();
        assert!(!codes.lock().unwrap().get("claude").unwrap().same_channel(&other));
        assert!(codes.lock().unwrap().get("claude").unwrap().same_channel(&tx));
        // Once the receiver is gone the send fails — the executor answers
        // "no sign-in waiting" instead of a completion that lies.
        drop(rx);
        assert!(tx.send("late".to_string()).is_err());
    }

    /// pi is refused with the sentence the clients show verbatim, and an
    /// unknown agent never reaches a PTY either. EXP-827: the same parse
    /// reads the profile half of the payload: an existing id, a new
    /// label, or neither (the ambient login).
    #[test]
    fn only_claude_and_codex_are_runnable_agents() {
        use coding::agent_login::{parse_login_payload, LoginTarget};
        assert_eq!(
            parse_login_payload(&serde_json::json!({"agent": "pi", "switch": "false"})),
            Err("pi has no remote sign-in".to_string())
        );
        assert!(parse_login_payload(&serde_json::json!({"switch": "false"})).is_err());
        let claude = parse_login_payload(&serde_json::json!({"agent": "claude", "switch": "true"}))
            .unwrap();
        assert_eq!(claude.agent, CodingAgent::Claude);
        assert!(claude.switch);
        assert_eq!(claude.target, LoginTarget::System);
        let codex = parse_login_payload(&serde_json::json!({
            "agent": "codex", "switch": "false", "profileId": "0badf00d"
        }))
        .unwrap();
        assert_eq!(codex.agent, CodingAgent::Codex);
        assert_eq!(codex.target, LoginTarget::Profile("0badf00d".to_string()));
        let fresh = parse_login_payload(&serde_json::json!({
            "agent": "claude", "switch": "false", "newProfileLabel": "Work"
        }))
        .unwrap();
        assert_eq!(fresh.target, LoginTarget::NewProfile("Work".to_string()));
        // pi is refused before its profile half is even looked at.
        assert!(parse_login_payload(&serde_json::json!({
            "agent": "pi", "newProfileLabel": "Work"
        }))
        .is_err());
    }
}
