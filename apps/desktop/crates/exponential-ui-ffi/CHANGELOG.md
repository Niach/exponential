# Changelog: `exponential-ui-ffi` (UniFFI facade, not published)

All notable changes to this package. The SDK packages share one version and
release together from a `ui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## 0.1.0 (unreleased)

First release.

- A coarse `Surface` object: messages go in; frames, visuals, layers, lists and scrolls come out, with batched `Measurer` upcalls.
- Swift and Kotlin bindings, committed and drift-gated.
- `build-ios.sh` builds the xcframework: device, simulator and macOS slices, fat x86_64 + arm64, profile `mobile`, without the `cli` feature.
- `build-android.sh` builds the `.so` per ABI.
- VAPP-106: both scripts build the facade as ONE crate type (`cargo rustc --crate-type staticlib|cdylib`) so the `mobile` profile's LTO applies (cargo skipped it next to the rlib), `build-ios.sh` strips a copy of each archive before `lipo` (a device slice went from 44.5 MB to ~5 MB), and `EXPONENTIAL_UI_FEATURES` selects the core's feature gates. The profile keeps `panic = "unwind"` for the `catch_unwind` boundary.
- VAPP-131 / VAPP-138: `Surface.paint_at(indices)` returns each slot's visual plus, for a leaf, its text style in one call (a step's added and changed slots), instead of `visual(i)` + `text_style(i)` per slot.
- VAPP-134: the bindings skip uniffi's per-function API checksum calls at load (`omit_checksums`, Swift and Kotlin); the contract version check stays.
- VAPP-131: `patch-swift-binding.pl` (run by `generate-bindings.sh` and `build-ios.sh`) makes the generated Swift readers load integers, byte runs and strings in place instead of copying them out of the buffer.
