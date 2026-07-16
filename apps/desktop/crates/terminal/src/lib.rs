// Clean reimplementation from the VT spec + alacritty_terminal (Apache-2.0). NOT derived from Zed's GPL terminal crates.
//! `terminal` — the embedded terminal (masterplan-v3 §3.1 / §06).
//!
//! Phase 4 lands: `pty.rs` (portable-pty master), `emulator.rs`
//! (alacritty_terminal `Term` + vte `Processor`), `read_loop.rs` (the steer
//! tee), `session.rs` (one `Terminal` = pty + emulator + read loop + writer),
//! `keys.rs` (clean reimplementation of `to_esc_str`), `mouse.rs`,
//! `element.rs` (the gpui grid Element), `tab.rs` + `manager.rs`
//! (JetBrains-style multi-tab), `steer.rs` (publisher glue).
//!
//! The core modules (`pty`/`emulator`/`read_loop`/`session`) are
//! **gpui-free**; `keys`/`mouse` use gpui data types but no `Window` — all
//! six are unit-testable in isolation (§6.2): feed bytes/keystrokes, assert
//! grid/escape output. `element` is the gpui glue (the grid `Element` + the
//! `TerminalView` entity).
//!
//! Licensing boundary (§3.8): alacritty_terminal UPSTREAM (Apache-2.0) only —
//! never Zed's GPL `terminal`/`terminal_view` code.

pub mod element;
pub mod emulator;
pub mod keys;
pub mod manager;
pub mod mouse;
pub mod pty;
pub mod read_loop;
pub mod session;
pub mod tab;

pub use element::{init, GridGeometry, TerminalElement, TerminalView, TerminalViewEvent};
pub use emulator::{
    bracketed_paste_enabled, display_offset, grid_size, screen_lines, Emulator, EmulatorSignal,
    EventProxy, GridSize, TermHandle,
};
pub use keys::to_esc_str;
pub use manager::{TerminalManager, TerminalManagerEvent};
pub use pty::{
    build_command, login_path, open, prewarm_login_path, ChildExit, ExitSlot, Pty, SpawnSpec,
};
pub use read_loop::{spawn_read_loop, CaptureSink, RawSink, SinkSet, Wake};
pub use session::{ResizeObserver, Terminal};
pub use tab::{ExitHook, RunConfigId, TabId, TabKind, TabStatus, TerminalTab};
