# Vendored gpui-component (Exponential patch, #EXP-1620)

Source: the `gpui-component` crate 0.9.0 as published on crates.io (gpui-kit
`crates/component`, https://github.com/longbridge/gpui-kit), by Longbridge and
the gpui-kit contributors, licensed Apache-2.0 (`LICENSE-APACHE`).

Consumed through `[patch.crates-io]` in `apps/desktop/Cargo.toml`; its sibling
crates (`gpui-base`, `gpui-kit-assets`, `gpui-component-macros`) come from
crates.io unpatched. On a gpui-component bump, re-vendor the published crate
at the new version and re-apply the changes below (or drop the patch once
upstream carries them).

## Changes (Apache-2.0 section 4(b))

- `src/menu/popup_menu.rs`: `PopupMenuItem::Submenu` gains `value:
  Option<SharedString>`, drawn muted (`text_xs`, at most 112px, truncating)
  between the label and the chevron, and `PopupMenu::submenu_with_value`
  builds one (`submenu_with_icon` = no value). This is the trailing value of
  web/iOS/Android THE Menu's submenu row (`Checks   1 failing  ›`).
- #EXP-1739, anchored overlays close when their source is lost: a new
  `src/overlay_anchor.rs` (sources report their bounds in element state;
  `watch_anchor` closes an overlay once its anchor moves, scrolls, is clipped
  away or leaves the tree). `Root` itself lives in gpui-base now, so the
  per-frame `sweep_layer()` is mounted by gpui-component's window-state
  plugin (`src/root.rs`, `WindowState`'s render), which gpui-base's `Root`
  draws after the window's content. It is adopted by the managed tooltip
  (`src/tooltip.rs`), `ContextMenu` (`src/menu/context_menu.rs`: right click
  and long press; a dismissed menu drops its watch), `Popover` and so
  `DropdownMenu` (`src/popover.rs`), and the drawn `NativeMenu` fallback
  (`src/native_menu/fallback.rs`). `PopupMenu::close` is public (Escape's
  dismiss), and the managed tooltip's outer div carries the `tooltip` debug
  selector.
- #EXP-1739, the window frame opt-out: `src/root.rs` gives `WindowState` a
  `bordered` flag (default `true`) and a public `RootExt` trait (re-exported
  from `src/lib.rs`) whose `Root::bordered(false, cx)` sets it on a freshly
  built `Root`; `WindowState`'s `RootPlugin::decorate` then returns the
  surface without the Linux client-side `window_border`. This is the
  pre-0.8 `Root::bordered(false)` the app's own rounded frame
  (`crates/ui/src/window_frame.rs`) and the transparent menu window use.
- #EXP-1488, every button has an accessible name: `src/button/button.rs`
  (upstream now has `Button::accessibility_label()` and names a button by it,
  else its label): with neither, the tooltip text is the name (and is then not
  repeated as the description), so no icon button is an unnamed `Button` node.
- EXP-1676, `src/button/button.rs`: a disabled `Primary` button keeps its
  normal look with the fill and border at half opacity (web's
  `disabled:opacity-50`) instead of upstream's 15% fill and muted label.
- #EXP-1752, an open menu updates live: `src/menu/popup_menu.rs`
  `PopupMenu::refresh` is `rebuild` keeping the selected row (and so the
  open submenu) when the rebuilt menu still has it.
- #EXP-1629, one spinner: `src/spinner.rs` `Spinner::new()` draws the arc
  (`IconName::LoaderCircle`) on a linear 0.8 s turn instead of the spoked
  `Loader` easing in and out, and a host can register its own glyph
  (`set_spinner_glyph`, a `SpinnerGlyph` global) that draws every default
  `Spinner`: `eui_gpui::view::init` registers genUI's `spinner_glyph`, so
  the IDE's loading buttons (`src/button/button_icon.rs`) spin the ONE arc
  genUI's Spinner and `controls::spinner` draw.
- Packaging only: dropped `tests/` (and the manifest's `[[test]]` entry that
  pointed into it), `Cargo.lock`, `Cargo.toml.orig`, `.cargo_vcs_info.json`.
  No other source is modified.
