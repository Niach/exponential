---
title: ePilot guides
description: Depend on epilot and epilot-screen, ask for the OS permissions, and give an agent run the computer MCP server.
---

## Install

Two crates in the Exponential repo, taken as a git dependency on an `epilot-v<version>` tag: the rules are compiled in from the repo's fixtures, which only a checkout carries. The policy: [Install from a release tag](/docs/libraries/install/).

```toml
[dependencies]
epilot = { git = "https://github.com/Niach/exponential", tag = "epilot-v0.1.0" }
epilot-screen = { git = "https://github.com/Niach/exponential", tag = "epilot-v0.1.0" }
```

| Crate | What |
|---|---|
| `epilot` | The tools, the input lock and the MCP endpoint |
| `epilot-screen` | Screen memory and `get_screenshots` |

## Give a run the computer

```rust
// Plan: read readiness without prompting, or ask for every OS permission now.
match epilot::prepare() {
    epilot::Readiness::Ready => {}
    other => return eprintln!("{other:?}"),
}
epilot::set_work_display(None);                // None = Auto; Some(id) = a Display::id

// Act: admit a run; the agent gets the URL + its token.
let grant = epilot::grant("run-1", "EXP-12")?; // label = what the banner names
epilot::on_abort("run-1", Arc::new(|| cancel_the_turn()));
// MCP config: { "computer": { "type": "http", "url": grant.url,
//               "headers": { "Authorization": "Bearer ${EXP_COMPUTER_TOKEN}" } } }

// Observe: the lock engages (Some(label)) and releases (None).
epilot::set_presence_observer(|label| show_or_hide_banner(label));

epilot::turn_started("run-1");                 // the person's next message: an abort is answered
epilot::revoke("run-1");                       // the run ended: token dead, lock released if last
epilot::shutdown();                            // the switch went off: every grant ends
```

`cargo run -p epilot --example host` runs this as a host; `examples/serve.rs` calls every tool by hand.

The public API, the lock and permissions per OS: [Reference](/docs/libraries/epilot/reference/).
