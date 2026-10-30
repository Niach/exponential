//! EXP-1425: the smallest host that embeds ePilot, the way a third-party
//! app (spezl) does: `cargo run -p epilot --example host`.
//!
//! Plan: check readiness and ask for the OS permissions once. Act: grant one
//! run, hook its Esc abort, print the MCP config an agent CLI takes. Observe:
//! the presence observer says when the person's input is locked. Return
//! ends the run (its token dies, the lock is released).

fn main() {
    // Plan: nothing interrupts a run later (macOS dialogs show here).
    match epilot::prepare() {
        epilot::Readiness::Ready => {}
        epilot::Readiness::MissingPermission(reason) | epilot::Readiness::Unsupported(reason) => {
            eprintln!("computer use is not ready: {reason}");
            return;
        }
    }

    // Observe: the host's banner ("Computer use in progress · Esc to abort").
    epilot::set_presence_observer(|label| match label {
        Some(label) => eprintln!("[banner] {label} drives the screen · Esc to abort"),
        None => eprintln!("[banner] gone"),
    });

    // Act: one run, one bearer token; the person's Esc cancels its turn.
    let session_id = "example-run";
    let grant = epilot::grant(session_id, "Example").expect("grant");
    epilot::on_abort(session_id, std::sync::Arc::new(|| eprintln!("[abort] cancel the agent's turn here")));

    let config = serde_json::json!({
        "mcpServers": {
            epilot::SERVER_NAME: {
                "type": "http",
                "url": grant.url,
                "headers": { "Authorization": format!("Bearer ${{{}}}", epilot::TOKEN_ENV) },
            }
        }
    });
    println!("{}", serde_json::to_string_pretty(&config).unwrap());
    println!("export {}={}", epilot::TOKEN_ENV, grant.token);
    println!("serving; press Return to end the run");

    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    epilot::revoke(session_id);
    epilot::shutdown();
}
