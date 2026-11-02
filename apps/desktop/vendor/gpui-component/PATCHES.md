# Vendored gpui-component (Exponential patch, #EXP-1620)

Source: the `gpui-component` crate (`crates/ui`) of
https://github.com/longbridge/gpui-component at rev
`da4f93696dc2b2b4d91bcc42412b9053a3d24de8` (the workspace pin), by Longbridge
and the gpui-component contributors, licensed Apache-2.0 (`LICENSE-APACHE`).

Consumed through `[patch."https://github.com/longbridge/gpui-component"]` in
`apps/desktop/Cargo.toml`; its sibling crates (`gpui-base`,
`gpui-component-assets`, `gpui-component-macros`) stay git dependencies at the
same rev. On a gpui-component bump, re-vendor `crates/ui` at the new rev and
re-apply the change below (or drop the patch once upstream carries it).

## Changes (Apache-2.0 section 4(b))

- `src/menu/popup_menu.rs`: `PopupMenuItem::Submenu` gains `value:
  Option<SharedString>`, drawn muted (`text_xs`, at most 112px, truncating)
  between the label and the chevron, and `PopupMenu::submenu_with_value`
  builds one (`submenu_with_icon` = no value). This is the trailing value of
  web/iOS/Android THE Menu's submenu row (`Checks   1 failing  ›`).
- Packaging only: `Cargo.toml` resolves the upstream workspace's inherited
  keys (`*.workspace = true`, edition, lints) to the values of the upstream
  workspace manifest at that rev; `tests/` dropped. No other source is
  modified.
