# Vendored gpui-pre-apple (Exponential patch, #VAPP-166)

Source: the `gpui-pre-apple` crate 0.3.11 as published on crates.io, a
snapshot of the `gpui_apple` crate (`crates/gpui_apple`) of
https://github.com/zed-industries/zed at rev
`f816e1f9263e3a8ef63c4baaa02e1441ce1c6038`, by Zed Industries and the Zed
contributors, licensed Apache-2.0 (`LICENSE-APACHE`).

Consumed through `[patch.crates-io]` in `apps/desktop/Cargo.toml`; every
other `gpui-pre-*` crate stays a crates.io dependency at the same version.
On a gpui-pre bump, re-vendor the published crate at the new version and
re-apply the change below, or drop the patch once upstream carries it.

## Changes (Apache-2.0 section 4(b))

All in `src/text_system.rs` (the CoreText text system of macOS and iOS),
each marked `Exponential patch (#VAPP-166)`:

- A variable font paints at the requested weight. Upstream matched the
  family's faces with font-kit and used the match as is, so a variable file
  (one face, loaded at its `wght` axis default) painted every weight at the
  default: Nunito[wght] at 200, 400, 600 and 800 shaped one width.
  `font_id` now passes the match through `variable_instance`: a face loaded
  at its axis default whose own weight is not the requested one gets the
  instance at the CSS weight clamped to the axis, with its own `FontId`
  (cached per base face and weight) holding a CTFont copy that carries
  `kCTFontVariationAttribute`, so shaping, metrics, raster bounds and
  rasterising all use the instance. `id_for_native_font` maps a shaped
  run's CTFont back to its instance by PostScript name and `wght`
  (`font_ids_by_variation`). Static faces, named instances (the system
  font's faces) and the axis default stay the matched face. The `variable`
  module reads the axis and the variation through CoreText directly.

The Linux twin of this change is in `vendor/gpui-pre-wgpu`
(`variable_instance` in `src/cosmic_text_system.rs`).

## Packaging

`Cargo.toml` stays the crates.io-normalised manifest. Dropped from the
published crate: `Cargo.lock`, `Cargo.toml.orig`, `.cargo-ok`. The crate's
own `vendor/gpui` sources (part of the published package) are kept. No other
source is modified. `apps/desktop/crates/eui-gpui/tests/variable_font_macos.rs`
covers the change: Nunito[wght] at 400/600/800 matches the static Nunito
Regular/SemiBold/ExtraBold widths within 1 %.
