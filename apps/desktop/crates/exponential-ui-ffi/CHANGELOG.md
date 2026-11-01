# Changelog: `exponential-ui-ffi` (UniFFI facade, not published)

All notable changes to this package. The SDK packages share one version and
release together from a `eui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## Unreleased

## 0.3.0 (2026-10-31)

- VAPP-180 (with the SDK): a package's `exposes` and `upstreams`, validated by the core.

## 0.2.1 (2026-10-30)

- Version-only bump with the SDK (eui-gpui 0.2.1: per-cluster font fallback, YO-30); unchanged from 0.2.0.

## 0.2.0 (2026-10-30)

- VAPP-162 / NEW-77: `voice_wave_json`, `voice_wave_bars_json`, `qr_matrix_json`, `segmented_badge_text`, `Surface::hold` + `is_held` for the Swift and Kotlin painters; bindings regenerated.
- VAPP-122: `source_with_params_json(source, params_json)` and `validate_record_json(schema_json, data_json)` for the Swift and Kotlin hosts; bindings regenerated.

## 0.1.1 (2026-10-23)

- Version-only bump with the SDK (the npm packages moved to the `@exponential_app` scope); unchanged from 0.1.0.

## 0.1.0 (2026-10-23)

First release.

- VAPP-126 (review #1187): `visibility_hidden` is also set for a `display: none` box (the core lays it out at zero size with its subtree): SwiftUI and Compose paint nothing of it, take no taps and keep it out of the a11y tree; a zero frame alone let an icon button's glyph draw past it on SwiftUI.
- A coarse `Surface` object: messages go in; frames, visuals, layers, lists and scrolls come out, with batched `Measurer` upcalls.
- Swift and Kotlin bindings, committed and drift-gated.
- `build-ios.sh` builds the xcframework: device, simulator and macOS slices, fat x86_64 + arm64, profile `mobile`, without the `cli` feature.
- `build-android.sh` builds the `.so` per ABI.
- VAPP-106: both scripts build the facade as ONE crate type (`cargo rustc --crate-type staticlib|cdylib`) so the `mobile` profile's LTO applies (cargo skipped it next to the rlib), `build-ios.sh` strips a copy of each archive before `lipo` (a device slice went from 44.5 MB to ~5 MB), and `EXPONENTIAL_UI_FEATURES` selects the core's feature gates. The profile keeps `panic = "unwind"` for the `catch_unwind` boundary.
- VAPP-131 / VAPP-138: `Surface.paint_at(indices)` returns each slot's visual plus, for a leaf, its text style in one call (a step's added and changed slots), instead of `visual(i)` + `text_style(i)` per slot.
- VAPP-134: the Kotlin binding skips uniffi's per-function API checksum calls at load (`omit_checksums`: ≈280 JNA calls); Swift keeps them (direct calls, microseconds) as the guard against a stale xcframework, and the contract version check stays on both.
- VAPP-131: `patch-swift-binding.pl` (run by `generate-bindings.sh` and `build-ios.sh`) makes the generated Swift readers load integers, byte runs and strings in place instead of copying them out of the buffer.
- VAPP-151: `run-binding-tests.sh kotlin` compiles again and runs in CI (the `compose` job; `swift` runs in the `swift` job): kotlin and JNA are pinned to the Compose package's version catalog and the compiler's classpath to its POM, each jar from the gradle cache at that exact version or fetched once from Maven Central (`sort | tail -1` had picked jna 5.6.0 over 5.17.0); `FacadeTest.kt` escapes `${'$'}schema` in its raw string.
