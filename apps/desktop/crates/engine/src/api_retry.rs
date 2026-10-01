//! FEED-61 — the API-error retry: a turn that died on a transient API
//! failure is picked back up by the engine, not by a person.
//!
//! The reported release run lost four subagents and its own main turn to
//! `API Error: No response from API` / `ENOTFOUND` / dropped streams while
//! the network was fine. The CLI retries a request itself, then gives up,
//! prints the error as the turn's last word and goes idle — and the run sat
//! there until someone typed "continue". Every one of those nudges worked.
//!
//! The rule, fed by the same event stream every viewer gets
//! ([`ApiRetry::observe`], from `SessionCtx::dispatch`) and evaluated on the
//! lifecycle's one-second tick ([`ApiRetry::tick`]):
//!
//! 1. A MAIN-lane turn whose last output is a retryable [`ApiError`] row
//!    (the CLI's `server_error`: outage, timeout, reset, DNS) arms a retry
//!    when the turn ends. A subagent's error does not: its parent turn is
//!    still running and decides what to do with the failed lane.
//! 2. Once the run has stayed idle for the attempt's backoff ([`BACKOFF`]),
//!    is not waiting on a person and is not behind a usage wall, the engine
//!    sends [`retry_message`] as an ordinary steer.
//! 3. A turn that ENDS without an API error resets the attempt count — real
//!    output alone does not: in a flapping outage each retry's turn says one
//!    sentence and dies again, and that must still run out of attempts. A
//!    turn someone else started disarms the pending retry. After
//!    [`BACKOFF`]`.len()` failed attempts in a row the engine stops and says
//!    so ([`give_up_notice`]) — an outage that long needs a person.
//!
//! Never retried: `invalid_request` (the same request fails the same way),
//! auth and model errors, and anything on the rate-limit slot (account
//! rotation owns walls, EXP-1005).
//!
//! [`ApiError`]: steer::ActivityEvent::ApiError

use std::time::{Duration, Instant};

/// Idle time before attempt N (0-based). The CLI already spent its own
/// retries inside the failed turn, so the first wait is short.
pub const BACKOFF: [Duration; 3] = [
    Duration::from_secs(30),
    Duration::from_secs(2 * 60),
    Duration::from_secs(10 * 60),
];

/// What the lifecycle should do after one tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryAction {
    None,
    /// Send [`retry_message`]: `attempt` is 1-based.
    Retry { attempt: u32 },
    /// Every attempt failed the same way: post [`give_up_notice`], once.
    GiveUp,
}

/// One tick's view of the session.
#[derive(Clone, Copy, Debug)]
pub struct RetryInput {
    pub now: Instant,
    pub live: bool,
    pub idle: bool,
    pub needs_input: bool,
    pub blocked: bool,
}

#[derive(Debug, Default)]
pub struct ApiRetry {
    /// A turn is in flight (seen `Turn started`, not yet `ended`).
    in_turn: bool,
    /// The turn's latest main-lane output is a retryable API error.
    errored: bool,
    /// When the errored turn ended: the backoff clock.
    armed_at: Option<Instant>,
    /// Retries sent since a turn last ended without an API error.
    attempts: u32,
}

/// The CLI's `error` word for a failure worth sending again. A frame with no
/// word (an older CLI, or the error only on the turn's `result`) is judged by
/// its text.
pub fn retryable(error_type: Option<&str>, message: &str) -> bool {
    match error_type {
        Some("server_error") => true,
        Some(_) => false,
        None => {
            let text = message.to_ascii_lowercase();
            [
                "no response from api",
                "can't reach the api",
                "enotfound",
                "econnreset",
                "etimedout",
                "connection lost",
                "connection error",
                "timed out",
                "overloaded",
                "internal server error",
            ]
            .iter()
            .any(|hint| text.contains(hint))
        }
    }
}

impl ApiRetry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold in one wire event of a LIVE run (never a history replay).
    pub fn observe(&mut self, event: &steer::ActivityEvent, now: Instant) {
        use steer::ActivityEvent as E;
        match event {
            E::Turn { state: steer::frames::TurnState::Started, .. } => {
                self.in_turn = true;
                self.errored = false;
                self.armed_at = None;
            }
            E::Turn { state: steer::frames::TurnState::Ended, .. } => {
                if self.in_turn && self.errored {
                    self.armed_at = Some(now);
                } else if self.in_turn {
                    // The turn got through to its end: the outage is over.
                    self.attempts = 0;
                }
                self.in_turn = false;
                self.errored = false;
            }
            E::ApiError { message, error_type, subagent_id: None, .. } if self.in_turn => {
                self.errored = retryable(error_type.as_deref(), message);
            }
            // The agent got through: whatever failed before is behind it. The
            // attempt count stays until the turn ENDS clean — a retry that says
            // one sentence and dies again is still a failed attempt.
            E::Narration { subagent_id: None, .. } | E::Tool { subagent_id: None, .. }
                if self.in_turn =>
            {
                self.errored = false;
            }
            _ => {}
        }
    }

    /// A person pressed Stop: whatever was pending is theirs to restart.
    pub fn disarm(&mut self) {
        self.errored = false;
        self.armed_at = None;
    }

    pub fn tick(&mut self, input: RetryInput) -> RetryAction {
        let Some(armed_at) = self.armed_at else {
            return RetryAction::None;
        };
        if !input.live {
            self.armed_at = None;
            return RetryAction::None;
        }
        // A card or a wall holds the retry; the clock keeps running, so it
        // fires as soon as both are gone.
        if !input.idle || input.needs_input || input.blocked {
            return RetryAction::None;
        }
        let Some(backoff) = BACKOFF.get(self.attempts as usize) else {
            self.armed_at = None;
            return RetryAction::GiveUp;
        };
        if input.now.saturating_duration_since(armed_at) < *backoff {
            return RetryAction::None;
        }
        self.armed_at = None;
        self.attempts += 1;
        RetryAction::Retry { attempt: self.attempts }
    }
}

/// The steer the engine sends. Says who is speaking: the agent must not take
/// it for its person, and the transcript must show why the run moved again.
pub fn retry_message(attempt: u32) -> String {
    format!(
        "[Exponential: the last API request failed and the turn stopped. This is automatic retry \
         {attempt} of {}, not a person.] Continue where you left off. If subagents were cut off, \
         check what they left on disk and resume them rather than starting over.",
        BACKOFF.len()
    )
}

pub fn give_up_notice() -> String {
    format!(
        "The API kept failing: {} automatic retries each ended in an API error. Send a message to \
         pick the run back up.",
        BACKOFF.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use steer::frames::TurnState;
    use steer::ActivityEvent as E;

    fn turn(state: TurnState) -> E {
        E::Turn { state, started_at: None, tokens: None, at: None }
    }

    fn api_error(error_type: Option<&str>, subagent: Option<&str>) -> E {
        E::ApiError {
            message: "API Error: No response from API (waited 6m, then 10m on the retry)".into(),
            error_type: error_type.map(str::to_string),
            subagent_id: subagent.map(str::to_string),
            at: None,
        }
    }

    fn idle(now: Instant) -> RetryInput {
        RetryInput { now, live: true, idle: true, needs_input: false, blocked: false }
    }

    fn failed_turn(retry: &mut ApiRetry, at: Instant) {
        retry.observe(&turn(TurnState::Started), at);
        retry.observe(&api_error(Some("server_error"), None), at);
        retry.observe(&turn(TurnState::Ended), at);
    }

    #[test]
    fn a_turn_that_died_on_a_server_error_is_retried_after_its_backoff() {
        let t0 = Instant::now();
        let mut retry = ApiRetry::new();
        failed_turn(&mut retry, t0);
        assert_eq!(retry.tick(idle(t0 + BACKOFF[0] - Duration::from_secs(1))), RetryAction::None);
        assert_eq!(retry.tick(idle(t0 + BACKOFF[0])), RetryAction::Retry { attempt: 1 });
        // Once per failed turn.
        assert_eq!(retry.tick(idle(t0 + BACKOFF[0] * 4)), RetryAction::None);
    }

    #[test]
    fn repeated_failures_back_off_then_give_up_once() {
        let mut at = Instant::now();
        let mut retry = ApiRetry::new();
        for (index, backoff) in BACKOFF.iter().enumerate() {
            failed_turn(&mut retry, at);
            assert_eq!(retry.tick(idle(at + *backoff - Duration::from_secs(1))), RetryAction::None);
            at += *backoff;
            assert_eq!(retry.tick(idle(at)), RetryAction::Retry { attempt: index as u32 + 1 });
        }
        failed_turn(&mut retry, at);
        assert_eq!(retry.tick(idle(at)), RetryAction::GiveUp);
        assert_eq!(retry.tick(idle(at + Duration::from_secs(3600))), RetryAction::None);
    }

    fn narration() -> E {
        E::Narration {
            text: "Picking up.".into(),
            before_question_id: None,
            message_id: None,
            subagent_id: None,
            at: None,
        }
    }

    /// A flapping outage: every retry's turn narrates, then dies on the same
    /// error. The output must not buy another round of attempts.
    #[test]
    fn a_retry_that_speaks_then_fails_again_still_runs_out() {
        let mut at = Instant::now();
        let mut retry = ApiRetry::new();
        failed_turn(&mut retry, at);
        for (index, backoff) in BACKOFF.iter().enumerate() {
            at += *backoff;
            assert_eq!(retry.tick(idle(at)), RetryAction::Retry { attempt: index as u32 + 1 });
            retry.observe(&turn(TurnState::Started), at);
            retry.observe(&narration(), at);
            retry.observe(&api_error(Some("server_error"), None), at);
            retry.observe(&turn(TurnState::Ended), at);
        }
        assert_eq!(retry.tick(idle(at)), RetryAction::GiveUp);
        assert_eq!(retry.tick(idle(at + Duration::from_secs(3600))), RetryAction::None);
    }

    #[test]
    fn a_clean_turn_resets_the_count_and_clears_the_error() {
        let t0 = Instant::now();
        let mut retry = ApiRetry::new();
        failed_turn(&mut retry, t0);
        assert_eq!(retry.tick(idle(t0 + BACKOFF[0])), RetryAction::Retry { attempt: 1 });
        // The retry's turn works, then a LATER turn fails: attempt 1 again.
        retry.observe(&turn(TurnState::Started), t0);
        retry.observe(
            &E::Narration {
                text: "Picking up.".into(),
                before_question_id: None,
                message_id: None,
                subagent_id: None,
                at: None,
            },
            t0,
        );
        retry.observe(&turn(TurnState::Ended), t0);
        assert_eq!(retry.tick(idle(t0 + BACKOFF[2])), RetryAction::None);
        failed_turn(&mut retry, t0);
        assert_eq!(retry.tick(idle(t0 + BACKOFF[0])), RetryAction::Retry { attempt: 1 });
    }

    #[test]
    fn what_is_never_retried() {
        let t0 = Instant::now();
        let late = idle(t0 + BACKOFF[2]);
        // A subagent's failure: the parent turn owns it.
        let mut retry = ApiRetry::new();
        retry.observe(&turn(TurnState::Started), t0);
        retry.observe(&api_error(Some("server_error"), Some("toolu_1")), t0);
        retry.observe(&turn(TurnState::Ended), t0);
        assert_eq!(retry.tick(late), RetryAction::None);
        // A request the API refuses the same way every time.
        let mut retry = ApiRetry::new();
        retry.observe(&turn(TurnState::Started), t0);
        retry.observe(&api_error(Some("invalid_request"), None), t0);
        retry.observe(&turn(TurnState::Ended), t0);
        assert_eq!(retry.tick(late), RetryAction::None);
        // An error outside a live turn (a replayed transcript).
        let mut retry = ApiRetry::new();
        retry.observe(&api_error(Some("server_error"), None), t0);
        retry.observe(&turn(TurnState::Ended), t0);
        assert_eq!(retry.tick(late), RetryAction::None);
        // A turn somebody started in the meantime.
        let mut retry = ApiRetry::new();
        failed_turn(&mut retry, t0);
        retry.observe(&turn(TurnState::Started), t0);
        assert_eq!(retry.tick(RetryInput { idle: false, ..late }), RetryAction::None);
        retry.observe(&turn(TurnState::Ended), t0);
        assert_eq!(retry.tick(late), RetryAction::None);
    }

    #[test]
    fn a_stop_cancels_the_pending_retry() {
        let t0 = Instant::now();
        let mut retry = ApiRetry::new();
        failed_turn(&mut retry, t0);
        retry.disarm();
        assert_eq!(retry.tick(idle(t0 + BACKOFF[2])), RetryAction::None);
    }

    #[test]
    fn a_card_or_a_wall_holds_the_retry_until_it_clears() {
        let t0 = Instant::now();
        let mut retry = ApiRetry::new();
        failed_turn(&mut retry, t0);
        let due = idle(t0 + BACKOFF[0]);
        assert_eq!(retry.tick(RetryInput { needs_input: true, ..due }), RetryAction::None);
        assert_eq!(retry.tick(RetryInput { blocked: true, ..due }), RetryAction::None);
        assert_eq!(retry.tick(due), RetryAction::Retry { attempt: 1 });
    }

    #[test]
    fn an_untyped_error_is_judged_by_its_text() {
        assert!(retryable(None, "API Error: Can't reach the API server, check your internet or DNS (ENOTFOUND)"));
        assert!(retryable(None, "API Error: No response from API (waited 3m)"));
        assert!(retryable(Some("server_error"), "anything"));
        assert!(!retryable(None, "API Error: 400 bad image"));
        assert!(!retryable(Some("authentication_failed"), "API Error: No response from API"));
    }
}
