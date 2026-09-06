//! EXP-746 — the engine's integration harness: a SCRIPTED `ConnectTo<Client>`
//! over `Channel::duplex()`, so the whole host can be driven with no CLI, no
//! network and no relay.
//!
//! Owned by lane E1. What it must cover (the plan's list): a prompt turn's
//! recorded events; a permission surfacing as an ANSWERABLE question whose id
//! is the tool-call id, acked and resolved within budget; **a cancel during a
//! turn actually reaching the adapter** (the only mechanical guard against a
//! dispatch-loop deadlock); `set_config`/`set_mode` coming back as a
//! re-emitted `config_state`; child EOF producing exactly ONE `on_exit` with
//! `exit:<code>`; a spawned task's error NOT killing the connection; and the
//! `launch_hold` still being alive when `engine::start` returns (EXP-478).

#[test]
fn the_fake_agent_harness_is_wired_up() {
    // EXP-746 E1: fill — replace with the scripted-adapter cases above.
    assert!(engine::client_capabilities().fs.read_text_file);
}
