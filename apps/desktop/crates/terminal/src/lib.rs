// Clean reimplementation from the VT spec + rio-vt (MIT). NOT derived from Zed's GPL terminal crates.
//! `terminal` — the embedded terminal (masterplan-v3 §3.1 / §06).
//!
//! Phase 4 lands: `pty.rs` (portable-pty master), `emulator.rs`
//! (rio-vt `Crosswords` + its `Processor`, EXP-636), `read_loop.rs` (the
//! single reader), `session.rs` (one `Terminal` = pty + emulator + read loop + writer),
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
//! The default-on `gpui` cargo feature (EXP-403) gates the UI half
//! (`element`/`manager`/`tab`'s view types/`keys`/`mouse`) so the headless
//! `exponential` CLI can consume the core without pulling gpui.
//!
//! Licensing boundary (§3.8): the emulator core is rio-vt (MIT, EXP-636;
//! before that upstream alacritty_terminal, Apache-2.0) — never Zed's GPL
//! `terminal`/`terminal_view` code.

#[cfg(feature = "gpui")]
pub mod element;
pub mod emulator;
#[cfg(feature = "gpui")]
pub mod keys;
#[cfg(feature = "gpui")]
pub mod manager;
#[cfg(feature = "gpui")]
pub mod mouse;
pub mod process;
pub mod pty;
pub mod read_loop;
pub mod session;
pub mod tab;

#[cfg(feature = "gpui")]
pub use element::{init, GridGeometry, TerminalElement, TerminalView, TerminalViewEvent};
pub use emulator::{
    advance_bytes, bracketed_paste_enabled, display_offset, grid_size, screen_lines,
    scroll_to_bottom, scroll_up, Emulator, EmulatorSignal, EventProxy, GraphicsUpdate, Term,
    TermHandle, TermMode, DEFAULT_CELL_PX,
};
#[cfg(feature = "gpui")]
pub use keys::to_esc_str;
#[cfg(feature = "gpui")]
pub use manager::{TerminalManager, TerminalManagerEvent};
pub use process::{background_command, refresh_windows_path};
pub use pty::{
    build_command, login_path, open, prewarm_login_path, ChildExit, ExitSlot, Pty, SpawnSpec,
};
pub use read_loop::{spawn_read_loop, Wake};
pub use session::Terminal;
// The emulator crates, re-exported so integration tests and dependants name
// grid types through this crate instead of pinning their own copies.
pub use rio_graphics;
pub use rio_vt;
#[cfg(feature = "gpui")]
pub use tab::{ExitHook, TerminalTab};
pub use tab::{TabId, TabKind, TabStatus};
