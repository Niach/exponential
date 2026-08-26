//! DEV-ONLY readiness handshake for the screenshot pipeline (EXP-633).
//!
//! `packages/shots` used to sleep a fixed number of seconds before grabbing a
//! desktop window, which is both slow (every view paid the worst case) and
//! flaky (a cold sync blew past it and photographed skeleton rows). Instead
//! the app itself says when it is done: with `EXP_DEV_READY_FILE=<path>` set
//! it polls its own state every [`POLL_INTERVAL`] and, once everything the
//! capture depends on has settled, writes a one-line JSON marker the capturer
//! waits for.
//!
//! Ready means, in this order:
//!
//! 1. a shell window exists,
//! 2. the SESSION is where this run wants it — the signed-in lane
//!    (`EXP_DEV_SERVER` + `EXP_DEV_TOKEN` both set) needs
//!    [`sync::SessionPhase::Synced`] AND every shape past its first
//!    `up-to-date` ([`sync::Store::all_shapes_ready`]); the signed-out lane
//!    needs `SignedOut` with no auth-config fetch in flight
//!    ([`crate::login::login_config_settled`]),
//! 3. the DEV-ONLY dialog hook has finished
//!    ([`crate::screens::dev_dialog_settled`]) — `EXP_DEV_DIALOG` opens its
//!    overlay 1.5s after its precondition resolves, and a capture taken
//!    before that would show the bare screen.
//!
//! While it waits it prints the blocking reason (naming the shapes it is
//! still missing) every [`LOG_INTERVAL`] whenever that reason CHANGES, so a
//! failed capture can be reproduced by hand with the same env. Everything
//! here is a no-op without `EXP_DEV_READY_FILE`. Never document for users.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gpui::App;

/// How often the probe re-evaluates readiness.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// How often the blocking reason may be printed (and only when it changed).
const LOG_INTERVAL: Duration = Duration::from_secs(5);

/// One probe per process, however many windows open.
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// What the probe is still waiting for, plus the ready-file payload numbers.
struct Progress {
    /// `None` = ready.
    blocked_on: Option<String>,
    /// Shapes past their first `up-to-date` (0 on the signed-out lane).
    shapes_ready: usize,
}

/// Install the DEV-ONLY ready probe. No-op unless `EXP_DEV_READY_FILE` names
/// a non-empty path. Call once, right after the first shell window opens.
pub fn install_dev_ready_probe(cx: &mut App) {
    let Some(path) = std::env::var("EXP_DEV_READY_FILE")
        .ok()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
    else {
        return;
    };
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    // Which lane this run is: a capture that hands the app a server AND a
    // token expects a signed-in app; anything else is a signed-out surface
    // (the login/onboarding views).
    let signed_in_lane = env_set("EXP_DEV_SERVER") && env_set("EXP_DEV_TOKEN");
    let dialog = std::env::var("EXP_DEV_DIALOG").unwrap_or_default();

    cx.spawn(async move |cx| {
        let started = Instant::now();
        let mut last_logged: Option<(String, Instant)> = None;
        loop {
            let progress = cx.update(|cx| evaluate(signed_in_lane, cx));
            match progress.blocked_on {
                None => {
                    let elapsed = started.elapsed().as_millis();
                    let payload = format!(
                        "{{\"ready_at_ms\":{elapsed},\"shapes\":{},\"dialog\":\"{}\"}}\n",
                        progress.shapes_ready,
                        dialog.escape_default()
                    );
                    match write_atomically(&path, &payload) {
                        Ok(()) => eprintln!(
                            "[exp-desktop] dev: ready file written after {elapsed}ms"
                        ),
                        Err(err) => eprintln!(
                            "[exp-desktop] dev: writing the ready file {path} failed: {err}"
                        ),
                    }
                    return;
                }
                Some(reason) => {
                    // Only a CHANGED reason logs, and at most every 5s — a
                    // 250ms loop would otherwise bury the real failure.
                    let due = last_logged
                        .as_ref()
                        .is_none_or(|(last, at)| {
                            *last != reason && at.elapsed() >= LOG_INTERVAL
                        });
                    if due {
                        eprintln!("[exp-desktop] dev: waiting: {reason}");
                        last_logged = Some((reason, Instant::now()));
                    }
                }
            }
            cx.background_executor().timer(POLL_INTERVAL).await;
        }
    })
    .detach();
}

/// Whether `name` is set to something non-empty.
fn env_set(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| !value.trim().is_empty())
}

/// One readiness evaluation against the CURRENT app state.
fn evaluate(signed_in_lane: bool, cx: &App) -> Progress {
    let blocked = |reason: String| Progress {
        blocked_on: Some(reason),
        shapes_ready: 0,
    };
    // The store is installed before any window opens, but a probe racing the
    // bootstrap must wait rather than panic.
    let Some(store) = sync::Store::try_global(cx) else {
        return blocked("store: not installed yet".to_string());
    };
    if store.state().read(cx).windows_open == 0 {
        return blocked("window: none open yet".to_string());
    }
    let session = store.session(cx);
    let shapes_ready = if signed_in_lane {
        if !matches!(session, sync::SessionPhase::Synced { .. }) {
            return blocked(format!("session: {session:?}, want Synced"));
        }
        let statuses = store.shape_statuses(cx);
        let missing = store.shapes_not_ready(cx);
        if !missing.is_empty() {
            return blocked(format!("shapes snapshot: {}", missing.join(", ")));
        }
        statuses.len()
    } else {
        if !matches!(session, sync::SessionPhase::SignedOut) {
            return blocked(format!("session: {session:?}, want SignedOut"));
        }
        if !crate::login::login_config_settled() {
            return blocked("auth config: fetch in flight".to_string());
        }
        0
    };
    if !crate::screens::dev_dialog_settled() {
        return blocked("dialog: EXP_DEV_DIALOG has not opened yet".to_string());
    }
    Progress {
        blocked_on: None,
        shapes_ready,
    }
}

/// Write `<path>.tmp` and rename onto `<path>`, so a capturer polling for the
/// file can never read a half-written marker.
fn write_atomically(path: &str, payload: &str) -> std::io::Result<()> {
    let tmp = format!("{path}.tmp");
    std::fs::write(&tmp, payload)?;
    std::fs::rename(&tmp, path)
}
