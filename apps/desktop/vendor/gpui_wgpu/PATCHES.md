# Vendored gpui_wgpu (Exponential patch, #EDE-65)

Source: the `gpui_wgpu` crate (`crates/gpui_wgpu`) of
https://github.com/zed-industries/zed at rev
`cc053a4a6fa2fd0e8793201ed9099466af1be0b1` (the pin in
`apps/desktop/Cargo.lock`), by Zed Industries and the Zed contributors,
licensed Apache-2.0 (`LICENSE-APACHE`).

Consumed through `[patch."https://github.com/zed-industries/zed"]` in
`apps/desktop/Cargo.toml`; every other zed crate (gpui, collections,
gpui_util, …) stays a git dependency on the same rev. On a zed bump,
re-vendor `crates/gpui_wgpu` at the new rev and re-apply the changes below,
or drop the patch once upstream carries it.

## Changes (Apache-2.0 section 4(b))

All in `src/cosmic_text_system.rs` (the Linux/FreeBSD text system), each
marked `Exponential patch (#EDE-65)`:

- `load_family` no longer removes a known colour emoji face
  (`check_is_known_emoji_font`) for lacking an `m` glyph. Upstream, naming
  `Noto Color Emoji` (the IDE's emoji picker, eUI's fallback chain) deleted
  the face from the font database, so every later emoji drew monochrome or
  as a box.
- Shaping prefers the colour emoji face for a grapheme cluster with emoji
  presentation (an `Emoji_Presentation` base or U+FE0F, never U+FE0E):
  `compute_run_spans` takes the face (`CosmicTextSystemState::emoji_face`,
  the first known emoji face in the database, looked up once) as an extra
  slot ahead of the primary face and the user fallback chain, and
  `layout_line_no_separators` builds its span even when the run has no
  fallback chain. Upstream, cosmic-text's per-character fallback took any
  monochrome face covering the codepoint first (Noto Sans Symbols2's 👍 🏳
  👁, DejaVu Sans's ✔ ❤).
- Packaging only: `Cargo.toml` resolves the upstream workspace's inherited
  keys (`*.workspace = true`, edition, lints) to the values of the upstream
  workspace manifest at that rev, takes the zed crates as floating git
  dependencies (the lock pins them), adds `unicode-properties` (emoji tables,
  already in the lock) and drops `benches/` and the dev-dependencies. The
  unit tests in `src/cosmic_text_system.rs` read zed's own assets and do not
  build here; `apps/desktop/crates/eui-gpui/tests/text_clusters.rs` covers
  the changes against the system's fonts. No other source is modified.
