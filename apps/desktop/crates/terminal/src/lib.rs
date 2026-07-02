//! `terminal` — the embedded terminal (masterplan-v3 §3.1 / §06).
//!
//! Phase 4 lands: `pty.rs` (portable-pty master), `emulator.rs`
//! (alacritty_terminal `Term` + vte `Processor`), `read_loop.rs` (the steer
//! tee), `keys.rs` (clean reimplementation of `to_esc_str`), `mouse.rs`,
//! `element.rs` (the gpui grid Element), `tab.rs` + `manager.rs`
//! (JetBrains-style multi-tab), `steer.rs` (publisher glue).
//!
//! Licensing boundary (§3.8): alacritty_terminal UPSTREAM (Apache-2.0) only —
//! never Zed's GPL `terminal`/`terminal_view` code.
