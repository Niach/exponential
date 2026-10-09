# Changelog: `exponential-ui-ffi` (UniFFI facade, not published)

All notable changes to this package. The SDK packages share one version and
release together from a `ui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## 0.1.0 (unreleased)

First release.

- A coarse `Surface` object: messages go in; frames, visuals, layers, lists and scrolls come out, with batched `Measurer` upcalls.
- Swift and Kotlin bindings, committed and drift-gated.
- `build-ios.sh` builds the xcframework: device, simulator and macOS slices, fat x86_64 + arm64, profile `mobile`, without the `cli` feature.
- `build-android.sh` builds the `.so` per ABI.
