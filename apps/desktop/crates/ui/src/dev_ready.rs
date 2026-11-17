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
//!    before that would show the bare screen,
//! 4. (#EXP-1654) on a `session:` screen, or (#EXP-1732) with
//!    `EXP_DEV_WAIT_FEED=1` (an agent's or an app's screen whose Chat pane is
//!    a recorded root), the steer feed has PAINTED its first replay
//!    ([`note_feed_painted`]): the viewer joins the device over eNet seconds
//!    after the shapes are Live, and a capture taken before that photographed
//!    "No live transcript".
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

/// The sessions whose steer view has painted a non-empty feed (#EXP-1654).
static FEED_PAINTED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// The steer view of `session_id` painted transcript rows: the feed drives'
/// last readiness condition. Cheap enough to call every frame.
pub(crate) fn note_feed_painted(session_id: &str) {
    let Ok(mut painted) = FEED_PAINTED.lock() else { return };
    if !painted.iter().any(|id| id == session_id) {
        painted.push(session_id.to_string());
    }
}

/// The session whose feed a `session:<id>` / `chat:<id>` screen waits for
/// (`None` = an agent's or an app's screen: any feed).
fn feed_target(screen: &str) -> Option<String> {
    let rest = screen.trim().strip_prefix("session:").or_else(|| screen.trim().strip_prefix("chat:"))?;
    let id = rest.split(':').next().unwrap_or_default();
    (!id.is_empty()).then(|| id.to_string())
}

/// #EXP-1886: has the feed this capture waits for painted? The shell opens
/// other session views too (the active rows, a restored tab); one of THEIR
/// feeds painting first wrote the ready file while the pictured chat still
/// read "Connecting…".
fn feed_ready(target: Option<&str>, painted: &[String]) -> bool {
    match target {
        Some(target) => painted.iter().any(|id| id == target),
        None => !painted.is_empty(),
    }
}

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
    let screen = std::env::var("EXP_DEV_SCREEN").unwrap_or_default();
    let wants_feed = screen.trim().starts_with("session:") || env_set("EXP_DEV_WAIT_FEED");
    let target = feed_target(&screen);

    cx.spawn(async move |cx| {
        let started = Instant::now();
        let mut last_logged: Option<(String, Instant)> = None;
        loop {
            let progress = cx.update(|cx| evaluate(signed_in_lane, wants_feed, target.as_deref(), cx));
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
fn evaluate(signed_in_lane: bool, wants_feed: bool, target: Option<&str>, cx: &App) -> Progress {
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
    if wants_feed && !FEED_PAINTED.lock().is_ok_and(|painted| feed_ready(target, &painted)) {
        return blocked(match target {
            Some(target) => format!("feed: session {target}'s steer feed has not replayed yet"),
            None => "feed: the steer feed has not replayed yet".to_string(),
        });
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

#[cfg(test)]
mod tests {
    use super::{feed_ready, feed_target};

    /// #EXP-1886: a `chat:` / `session:` capture is ready only once ITS
    /// session's feed painted; another session's feed never counts.
    #[test]
    fn the_ready_feed_is_the_pictured_sessions() {
        assert_eq!(feed_target("chat:5d7a2c90").as_deref(), Some("5d7a2c90"));
        assert_eq!(feed_target(" session:abc:plan ").as_deref(), Some("abc"));
        assert_eq!(feed_target("agent:triage"), None);
        assert_eq!(feed_target("chat"), None);

        let other = vec!["5c2d8a1e".to_string()];
        assert!(!feed_ready(Some("5d7a2c90"), &other));
        assert!(feed_ready(Some("5d7a2c90"), &["5c2d8a1e".to_string(), "5d7a2c90".to_string()]));
        // An agent's or an app's screen names no session: any feed.
        assert!(feed_ready(None, &other));
        assert!(!feed_ready(None, &[]));
    }
}
