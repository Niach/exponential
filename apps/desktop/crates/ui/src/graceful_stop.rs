//! EXP-637 — wait out the agent's current turn before tearing a run down.
//!
//! `exponential_sessions_end` reaches the server MID-TURN: the agent calls it
//! as its last tool, then keeps writing (its final message, its transcript
//! flush). The row flips to `ended` immediately, and the kill-watch fires
//! while the CLI is still mid-sentence — killing it right there truncates
//! exactly the close-out the tool call was about.
//!
//! So an agent-declared end waits for the next idle edge
//! ([`steer::TurnSignal`]), bounded by [`STOP_GRACE`]: a stuck or hookless
//! agent never parks the teardown forever.

use std::sync::Arc;

use gpui::App;
use steer::TurnSignal;

// EXP-746: the bound and the policy moved into `steer` so the gpui-free ACP
// engine — which hosts the same teardown for the desktop AND the CLI daemon —
// obeys ONE definition instead of a hand-synced copy. Only `after_turn`, the
// gpui half, stays here. `stop_now` has no in-crate caller yet (the timer path
// below waits on the signal rather than polling), but it keeps this path so a
// UI caller finds the policy and its bound together.
#[allow(unused_imports)]
pub use steer::{stop_now, STOP_GRACE};

/// Run `then` on the gpui foreground once the agent is between turns — or
/// after [`STOP_GRACE`], whichever comes first. Fires IMMEDIATELY when the
/// agent is already idle (the common case: it called `sessions_end` and
/// stopped).
///
/// `signal` is the session's shared turn state; `None` (no emitter, an old
/// session) degrades to running `then` at once — never to waiting forever.
pub fn after_turn(
    session_id: &str,
    signal: Option<Arc<TurnSignal>>,
    then: impl FnOnce(&mut App) + 'static,
    cx: &mut App,
) {
    let Some(signal) = signal else {
        then(cx);
        return;
    };
    if signal.is_idle() {
        then(cx);
        return;
    }
    let waiter = signal.subscribe();
    let session_id = session_id.to_string();
    cx.spawn(async move |cx| {
        match waiter.recv_async().await {
            Ok(()) => log::info!("graceful stop [{session_id}]: turn finished"),
            Err(_) => log::debug!("graceful stop [{session_id}]: signal dropped"),
        }
        let _ = cx.update(|cx| then(cx));
    })
    .detach();
    // The bound: a turn that never ends must not park the teardown. The
    // timer races the waiter above; whichever lands first runs `then`, and
    // the loser finds the work already done (every consumer is idempotent).
    let signal_for_timeout = signal.clone();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(STOP_GRACE).await;
        // Fire the waiters as if the turn had ended: the branch above then
        // proceeds, and nothing has to be written twice.
        signal_for_timeout.set_idle(true);
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// The policy itself is locked in `steer::activity`
    /// (`stop_now_waits_for_idle_but_never_past_the_grace`); this only
    /// asserts the re-export still names it, so a desktop caller of
    /// `graceful_stop::STOP_GRACE` keeps compiling and keeps meaning the
    /// same minute.
    #[test]
    fn the_grace_is_the_steer_one() {
        assert_eq!(STOP_GRACE, Duration::from_secs(60));
        assert!(stop_now(true, Duration::ZERO));
        assert!(!stop_now(false, Duration::ZERO));
    }
}
