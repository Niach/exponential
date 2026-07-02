//! App-level actions + global keymap (masterplan-v3 §3.6 "Actions & keymap").
//!
//! Phase-1 scope: window/app lifecycle only. The workspace-chrome actions
//! (`ui::actions`) get their handlers in Phases 2/3; view-scoped bindings
//! (`.key_context("Workspace")` + predicate-style `KeyBinding`s) land with the
//! screens that own them.

use gpui::{actions, App, KeyBinding};

actions!(
    exp,
    [
        /// Open another workspace window sharing the global `Store` (§3.6
        /// multi-window).
        NewWindow,
        /// Quit the app.
        Quit,
    ]
);

pub fn init(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &NewWindow, cx| crate::windows::open_workspace_window(cx));

    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-shift-n", NewWindow, None),
    ]);

    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        KeyBinding::new("ctrl-q", Quit, None),
        KeyBinding::new("ctrl-shift-n", NewWindow, None),
    ]);
}
