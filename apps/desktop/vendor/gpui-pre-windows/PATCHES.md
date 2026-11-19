# Vendored gpui-pre-windows (Exponential patch, #ERR-2 / #ERR-3 / #EXP-1923)

Source: the `gpui-pre-windows` crate 0.3.11 as published on crates.io, a
snapshot of the `gpui_windows` crate (`crates/gpui_windows`) of
https://github.com/zed-industries/zed at rev
`f816e1f9263e3a8ef63c4baaa02e1441ce1c6038`, by Zed Industries and the Zed
contributors, licensed Apache-2.0 (`LICENSE-APACHE`).

Consumed through `[patch.crates-io]` in `apps/desktop/Cargo.toml`; every
other `gpui-pre-*` crate stays a crates.io dependency at the same version.
On a gpui-pre bump, re-vendor the published crate at the new version and
re-apply the changes below, or drop the patch once upstream carries them.

## Changes (Apache-2.0 section 4(b))

Each is marked `Exponential patch (#ERR-…)` in the source.

- **#ERR-2, `src/system_notifications.rs`:** every WinRT toast call
  (`ToastNotificationManager::CreateToastNotifier*`, `ToastNotifier::Show` /
  `Hide`, the toast document) runs on a worker thread of its own
  (`gpui-toasts`, a multithreaded COM apartment) fed by a channel
  (`CommandWorker`). Upstream made them on the UI thread. They are
  cross-process calls, and in the UI thread's single-threaded apartment COM's
  modal loop pumps window messages while it waits. gpui's frame callbacks then
  ran while the caller of `App::show_system_notification` (an
  `AsyncApp::update`) still held the App borrow, and every toast logged
  "RefCell already borrowed" for "reading power state for frame request",
  "scheduling follow-up frame" and "drawing and presenting frame" (and lost
  those frames). Responses (`Activated`) still reach the app through the same
  channel and foreground task.
- **#ERR-3, `src/util.rs`, `src/window.rs`, `src/events.rs`,
  `src/direct_manipulation.rs`:** a window closed natively (the caption's X,
  Alt+F4) is destroyed (`WM_DESTROY`) before gpui drops it. Upstream's
  teardown then hid, revoked drag and drop on, and destroyed the dead handle,
  deactivated Direct Manipulation on it, and a frame or bounds observer
  in between read its placement and cursor position. Each of those logged
  "Invalid window handle" (0x80070578, and 0x80040102 =
  `DRAGDROP_E_INVALIDHWND` from `RevokeDragDrop`). Now `RevokeDragDrop` runs
  in `handle_destroy_msg`, while the handle is valid on every path, and the
  drop, `calculate_window_bounds`, `mouse_position` and the Direct
  Manipulation drop skip a handle `is_live_window` (`IsWindow`) reports gone.

- **#EXP-1923, `src/util.rs`, `src/window.rs`:** `set_input_region` is
  implemented. Upstream ignored it off Linux. An empty region (the
  computer-use banner) makes the window click-through and never active:
  `WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_NOACTIVATE`, with layered
  attributes at full opacity so it looks the same (`set_click_through`).
  Any other region, or none, takes those styles back off. The X11 twin is
  in `vendor/gpui-pre-linux` (#EXP-1915); macOS is the `ui` crate's
  `macos_window::ignore_mouse`.

- **#VAPP-166 / #EXP-1889, `src/direct_write.rs`, tests only:**
  `exponential_variable_font_tests` loads `Nunito[wght]` from the genUI
  conformance fonts and checks that DirectWrite lays out and paints it at
  the requested weight (400 to 800 against the static instances). No
  source change: upstream's `GetMatchingFonts` already picks the variable
  font's named instance.

## Packaging

`Cargo.toml` stays the crates.io-normalised manifest. Dropped from the
published crate: `Cargo.lock`, `Cargo.toml.orig`, `.cargo_vcs_info.json`,
`.cargo-ok`. Tests: `system_notifications::tests`,
`util::exponential_patch_tests` and
`direct_write::exponential_variable_font_tests` (Windows only; `check-desktop-windows.yml`
runs them on pull requests touching this directory).
