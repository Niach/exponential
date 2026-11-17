# Vendored gpui-pre-linux (Exponential patch, #ERR-4)

Source: the `gpui-pre-linux` crate 0.3.11 as published on crates.io, a
snapshot of the `gpui_linux` crate (`crates/gpui_linux`) of
https://github.com/zed-industries/zed at rev
`f816e1f9263e3a8ef63c4baaa02e1441ce1c6038`, by Zed Industries and the Zed
contributors, licensed Apache-2.0 (`LICENSE-APACHE`).

Consumed through `[patch.crates-io]` in `apps/desktop/Cargo.toml`; every
other `gpui-pre-*` crate stays a crates.io dependency at the same version.
On a gpui-pre bump, re-vendor the published crate at the new version and
re-apply the changes below, or drop the patch once upstream carries them.

## Changes (Apache-2.0 section 4(b))

Each is marked `Exponential patch (#ERR-4)` in the source.

- **`src/linux/uri_list.rs` (new):** `dropped_paths(data, is_uri_list)`
  turns dropped data into local file paths only for a `text/uri-list`,
  per RFC 2483: CRLF or LF lines, `#` comment lines and empty lines are
  skipped. A line that is not a URI, or a URI that is not a local file,
  logs a warning. Anything else (plain text) yields no paths and logs
  nothing. Before the patch, gpui ran `Url::parse(line).log_err()` on every
  line of any accepted drag, so dropping 34 lines of text logged 34
  "relative URL without a base" errors (GlitchTip DESKTOP-G).
- **`src/linux/x11/client.rs`:** `SelectionNotify` calls `dropped_paths`
  with `drag_type == TextUriList`. The X11 client still accepts the
  TEXT/STRING/UTF8_STRING/text/plain drag types, but they yield no paths.
  `XdndEnter` picks the drag type with the new `xdnd_pick_type`: it takes
  `text/uri-list` when the source offers it, and otherwise the first
  supported type (as before). This applies to the three inline types and to
  `xdnd_get_supported_atom`'s `XdndTypeList`. Before, the first supported
  type in the source's list won, so a source offering text before
  `text/uri-list` dropped no files. The `Url` and `SmallVec` imports this
  code used are removed.
- **`src/linux/wayland/client.rs`:** the drop (always received as
  `text/uri-list`) goes through `dropped_paths(…, true)`, so RFC 2483
  comments and CRLF no longer log errors. `SmallVec` is imported for the
  tests only.
- **`src/linux.rs`:** declares `mod uri_list` (x11 or wayland builds).

## Input shape on X11 (#EXP-1912)

Each is marked `Exponential patch (#EXP-1912)` in the source.

- **`src/linux/x11/input_shape.rs` (new):** `rectangles` (a logical region
  as device-pixel rectangles) and `set` (the X Shape extension's Input shape
  of a window: `None` = the whole window, an empty list = no input).
- **`src/linux/x11/window.rs`:** `PlatformWindow::set_input_region` sets
  that shape. Upstream implements it on Wayland only, so on X11 a window
  could not be made click-through: the computer-use banner (override-redirect,
  above everything) took the clicks meant for the app under it.
- **`src/linux/x11.rs`:** declares `mod input_shape`.
- **`Cargo.toml`:** x11rb's `shape` feature.

Dropped plain text is NOT delivered to the app. gpui has no text-drop input
event (`FileDropEvent` carries paths only), and delivering dropped text would
need a new gpui API, which this patch does not add.

## Popup grab before any press (#EDE-86)

Marked `Exponential patch (#EDE-86)` in the source.

- **`src/linux/wayland/client.rs`:** an `AnchoredPopup` opened with
  `grab: true` sends `xdg_popup.grab` even when the client has seen no key
  or pointer press yet, with serial 0. Before, it sent no grab then. eDE's
  shell opens its start menu from a compositor hotkey (Super), and the
  grab is how the popup asks eDE's compositor for the keyboard. Nothing
  else opens a popup with `grab: true`.

## Packaging

`Cargo.toml` stays the crates.io-normalised manifest. Dropped from the
published crate: `Cargo.lock`, `Cargo.toml.orig`, `.cargo_vcs_info.json`,
`.cargo-ok`. Tests: `cargo test -p gpui-pre-linux --lib -- uri_list input_shape`
from `apps/desktop` (the input shape test starts a private Xvfb and is
skipped without one).
