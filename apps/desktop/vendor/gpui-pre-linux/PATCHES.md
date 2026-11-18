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

## A pointer leave moves the pointer out (#EDE-89)

Marked `Exponential patch (#EDE-89)` in the source.

- **`src/linux/wayland/client.rs`:** on `wl_pointer.leave` the window gets a
  `MouseMove` to `POINTER_OUTSIDE` (far outside any window) before the
  `MouseExited` (`pointer_left`). gpui's window keeps the last position it
  saw and hit-tests every later frame there; `MouseExited` does not move it.
  So a hover listener whose element repainted after the leave turned
  hovered again: eDE's taskbar button kept its hover preview open after the
  pointer left, and a dismissed preview opened again at once.
- **`src/linux/wayland/window.rs`:** `PlatformWindow::mouse_position` is the
  pointer's location only for the window the pointer is over
  (`WaylandClientState::mouse_location_in`), `POINTER_OUTSIDE` for every
  other one. Upstream returned the client's one location, which is in the
  coordinates of whichever surface has the pointer (or (0, 0)). gpui reads it
  on a window's first bounds: the taskbar's hover preview, a popup mapping
  above the button, started with the pointer inside its card at the
  button's coordinates, so its hover was on and held the preview open.
- **Test:** `pointer_left_tests::a_leave_moves_the_pointer_out_of_the_window_first`
  (`client.rs`), in the gate's vendored patch test step (`test.yml`).

## Images on the clipboard (#EXP-1946)

Marked `Exponential patch (#EXP-1946)` in the source. Upstream wrote every
clipboard item as text: Wayland offered only the text MIME types, and X11's
`write_to_clipboard` went through `set_text` (its `set_image` was unused and
put any format under the PNG target). A copied image reached no other
program, so the IDE's "Copy image" shipped on macOS/Windows only.

- **`src/linux/clipboard_formats.rs` (new):** `image_offers` (an item's first
  image in its own format, then a PNG copy when that is not PNG; an SVG or an
  undecodable image offers its own format only) and `has_text`. An
  image-only item offers no text; an item with neither offers empty text, as
  before.
- **`src/linux/x11/clipboard.rs`:** `Clipboard::set_item` replaces the unused
  `set_image`: the image targets first, then the text as `UTF8_STRING`. A
  conversion larger than one request (`maximum_request_bytes`, a PNG copy of a
  photo) is sent in `INCR` chunks (ICCCM 2.7.2, at most 1 MiB each; a
  requestor silent for 5 s is dropped). The exit handover to the clipboard
  manager finishes only once those chunks are out, waiting up to 3 s while
  one runs (100 ms otherwise, as before). The reader waits 1 s (upstream:
  10 ms) for another owner's next `INCR` chunk.
- **`src/linux/x11/client.rs`:** `write_to_clipboard` and `write_to_primary`
  call `set_item`.
- **`src/linux/wayland/clipboard.rs` + `client.rs`:** the data source offers
  the item's MIME types (`mime_types`, `primary_mime_types`) and `send` writes
  the requested type's bytes (any other type, our `pid/` marker, gets the
  text when there is some).
- **`src/linux.rs`:** declares `mod clipboard_formats`. **`Cargo.toml`:** the
  `image` crate (already in the lock through gpui) for the PNG copy.
- **Tests:** the `clipboard` tests in the three files; the X11 round trip
  starts a private Xvfb (skipped without one) and reads from another client,
  including an image larger than a request.

## Packaging

`Cargo.toml` stays the crates.io-normalised manifest. Dropped from the
published crate: `Cargo.lock`, `Cargo.toml.orig`, `.cargo_vcs_info.json`,
`.cargo-ok`. Tests: `cargo test -p gpui-pre-linux --lib -- uri_list input_shape pointer_left clipboard`
from `apps/desktop` (the input shape and clipboard round-trip tests start a
private Xvfb and are skipped without one).
