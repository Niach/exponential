//! EXP-746 — the manual spike driver: run one agent through the engine and
//! print `agent frame → ACP update → ActivityEvent` for every frame. The table
//! it prints becomes the spike report posted on the issue.
//!
//! Owned by lane S1.
//!
//! ```sh
//! cargo run -p engine --example exp746_acp_spike -- claude|codex|pi
//! ```

fn main() {
    let agent = std::env::args().nth(1);
    match agent.as_deref() {
        Some(agent @ ("claude" | "codex" | "pi")) => {
            // EXP-746 S1: fill — spawn the adapter, run the checklist and
            // print one row per frame.
            println!("exp746_acp_spike: {agent} is not driven yet (lane S1 fills this).");
        }
        _ => {
            eprintln!("usage: cargo run -p engine --example exp746_acp_spike -- claude|codex|pi");
            std::process::exit(2);
        }
    }
}
