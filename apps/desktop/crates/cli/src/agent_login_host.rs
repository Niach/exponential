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

use std::collections::HashSet;
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

/// Start one `agent_login` command. Validation and the dedupe claim happen
/// on the caller's (device-worker) thread — everything that can block moves
/// to a thread of its own.
pub fn run(
    ctx: &Ctx,
    settings: Settings,
    command: api::devices::PendingCommand,
    inflight: Arc<Mutex<HashSet<String>>>,
    doctor_soon: Arc<AtomicBool>,
) {
    let raw_agent = command.payload["agent"].as_str().unwrap_or_default();
    let switch = command.payload["switch"].as_str() == Some("true");
    let agent = match CodingAgent::parse(raw_agent) {
        // pi's `/login` is a slash command inside its TUI that opens a
        // provider flow with no remote-finishable handle (the server refuses
        // it too — this is the belt to that suspenders).
        Some(CodingAgent::Pi) | None => {
            let message = if raw_agent == "pi" {
                "pi has no remote sign-in"
            } else {
                "Malformed command payload."
            };
            complete(ctx, &command.id, false, message);
            return;
        }
        Some(agent) => agent,
    };

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
            let outcome = drive(&trpc, &settings, agent, switch, &command_id);
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
fn drive(
    trpc: &Arc<api::trpc::TrpcClient>,
    settings: &Settings,
    agent: CodingAgent,
    switch: bool,
    command_id: &str,
) -> Option<(bool, String)> {
    // A switch signs OUT first — otherwise every agent CLI here would just
    // report the account already signed in and exit.
    if switch {
        if let Err(err) = agent_login::logout(settings, agent) {
            // Not fatal: the login below may still prompt.
            log::info!("agent_login: sign-out before the switch failed: {err}");
        }
    }

    let plan = agent_login::login_plan(settings, agent);
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
                    let progress = LoginProgress::url(agent, url, code);
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

    /// pi is refused with the sentence the clients show verbatim, and an
    /// unknown agent never reaches a PTY either.
    #[test]
    fn only_claude_and_codex_are_runnable_agents() {
        assert_eq!(CodingAgent::parse("pi"), Some(CodingAgent::Pi));
        assert_eq!(CodingAgent::parse(""), None);
        assert_eq!(CodingAgent::parse("claude"), Some(CodingAgent::Claude));
        assert_eq!(CodingAgent::parse("codex"), Some(CodingAgent::Codex));
    }
}
