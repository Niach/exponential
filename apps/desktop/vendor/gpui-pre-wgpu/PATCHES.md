# Vendored gpui-pre-wgpu (Exponential patches, #EDE-65, #VAPP-166)

Source: the `gpui-pre-wgpu` crate 0.3.11 as published on crates.io, a
snapshot of the `gpui_wgpu` crate (`crates/gpui_wgpu`) of
https://github.com/zed-industries/zed at rev
`f816e1f9263e3a8ef63c4baaa02e1441ce1c6038`, by Zed Industries and the Zed
contributors, licensed Apache-2.0 (`LICENSE-APACHE`).

Consumed through `[patch.crates-io]` in `apps/desktop/Cargo.toml`; every
other `gpui-pre-*` crate stays a crates.io dependency at the same version.
On a gpui-pre bump, re-vendor the published crate at the new version and
re-apply the change below, or drop the patch once upstream carries it.

## Changes (Apache-2.0 section 4(b))

All in `src/cosmic_text_system.rs` (the Linux/FreeBSD text system), each
marked `Exponential patch (#EDE-65)` or `(#VAPP-166)`:

- Shaping prefers the colour emoji face for a grapheme cluster with emoji
  presentation (an `Emoji_Presentation` base or U+FE0F, never U+FE0E):
  `compute_run_spans` takes the face (`CosmicTextSystemState::emoji_face`,
  the first known emoji face in the database, looked up once and refreshed
  when fonts are added) as an extra slot ahead of the primary face and the
  user fallback chain, and `layout_line_no_separators` builds its span even
  when the run has no fallback chain. Upstream, cosmic-text's per-character
  fallback took any monochrome face covering the codepoint first (Noto Sans
  Symbols2's 👍 🏳 👁, DejaVu Sans's ✔ ❤).

- #VAPP-166, marked `Exponential patch (#VAPP-166)`: a variable face lays
  out and paints at the weight asked for. `font_id` turns the best match
  into its instance at the CSS weight (clamped to the `wght` axis; the
  axis default stays the loaded face): `variable_instance` loads it through
  cosmic-text's `get_font(id, weight)` (the shaper at that location) under
  its own `FontId`, with the normalized `wght` coordinates the metrics,
  advances and the swash rasteriser use; `font_match_properties` and
  `font_weight_and_style` report the instance's weight, and a fallback face
  resolved during layout is taken at the weight the run shaped with.
  Upstream, every request got the face's default instance (Google Fonts'
  `Nunito[wght]` painted everything at wght 200).
  `apps/desktop/crates/eui-gpui/tests/variable_fonts.rs` covers it.

The earlier fix to `load_family` (it removed a known colour emoji face
lacking an `m` glyph from the font database) is upstream since zed #65269:
the loop now skips such a face without removing it, so it is not patched
here.

## Packaging

`Cargo.toml` stays the crates.io-normalised manifest, plus
`unicode-properties` (emoji tables, already in `apps/desktop/Cargo.lock`)
and minus the `[[bench]]` target. Dropped from the published crate:
`benches/`, `Cargo.lock`, `Cargo.toml.orig`. The
unit tests in `src/cosmic_text_system.rs` read zed's own assets and do not
build here; `apps/desktop/crates/eui-gpui/tests/text_clusters.rs` covers
the change against the system's fonts. No other source is modified.
