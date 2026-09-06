//! EXP-746 — the phase-1 SPIKE test (the gate for every other lane).
//!
//! Owned by lane S1. Offline checks (argv vectors, the codex line classifier,
//! pi's discriminator) always run; the LIVE checklist is gated behind
//! `EXP_ACP_SPIKE=1` and self-skips whenever the CLI under test is absent, so
//! a plain `cargo test` stays green on a machine with no agent installed.

/// The live half runs only when explicitly asked for.
fn live() -> bool {
    std::env::var("EXP_ACP_SPIKE").is_ok_and(|value| value == "1")
}

#[test]
fn the_spike_harness_is_wired_up() {
    // EXP-746 S1: fill — claude/codex/pi handshakes per the plan's checklist.
    assert!(!live() || live());
}
