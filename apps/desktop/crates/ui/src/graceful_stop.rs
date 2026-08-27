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
use std::time::Duration;

use gpui::App;
use steer::TurnSignal;

/// How long an agent-declared end waits for the turn to finish before it
/// tears down anyway. Generous: the wait costs nothing while the agent is
/// still producing the output the user wants to read, and the fallback only
/// exists for agents whose idle edge never arrives (a hookless claude, a
/// crashed emitter).
pub const STOP_GRACE: Duration = Duration::from_secs(60);

/// Should the teardown proceed NOW? Pure, so the policy is testable without
/// a runtime: the agent is between turns, or the grace period ran out.
pub fn stop_now(idle: bool, elapsed: Duration) -> bool {
    idle || elapsed >= STOP_GRACE
}

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
    use super::*;

    #[test]
    fn stop_now_waits_for_idle_but_never_past_the_grace() {
        // Mid-turn: wait.
        assert!(!stop_now(false, Duration::ZERO));
        assert!(!stop_now(false, STOP_GRACE - Duration::from_millis(1)));
        // Between turns: go, however early.
        assert!(stop_now(true, Duration::ZERO));
        // A turn that never ends must not park the teardown forever.
        assert!(stop_now(false, STOP_GRACE));
        assert!(stop_now(false, STOP_GRACE + Duration::from_secs(60)));
    }

    /// The grace has to be long enough for a real close-out message and
    /// short enough that a hung agent's tab still resolves while someone is
    /// watching it.
    #[test]
    fn stop_grace_is_a_minute() {
        assert_eq!(STOP_GRACE, Duration::from_secs(60));
    }
}
