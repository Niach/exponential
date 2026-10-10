# Changelog: `ExponentialUI` (SwiftUI)

All notable changes to this package. The SDK packages share one version and
release together from a `ui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## 0.1.0 (unreleased)

First release.

- `ExponentialUI`: `SurfaceModel` + `ExponentialSurface` paint an A2UI surface at the frames the Rust core computes, on iOS 17+ and macOS 14+. It measures text in batches with TextKit/CoreText.
- `ExponentialUIPrimitives`: generic SwiftUI primitives themed by `PrimitiveTokens`. Pure SwiftUI, no binary.
- `ExponentialUIFFI.xcframework`: device arm64, simulator arm64 + x86_64, macOS arm64 + x86_64, profile `mobile`.
- Host-owned text fields take the Input `type`'s keyboard, autofill content type and autocorrection. Platform sliders keep the author's range and step (continuous when `step` is 0).
- An extension the core refuses throws: at `ExponentialUI.register`, and when a `SurfaceModel` is created.
- Video and AudioPlayer play their `src` through the media request (AVKit's player for Video; play / pause, a seekable track and the elapsed time for AudioPlayer). A request with headers is fetched under `media.limits` first. A denied `src` loads nothing, and the controls stay inert.
- Unnamed media fall back to the platform's localized image trait, not to English text.
- Conformance, real-font geometry and fixture replay suites run on macOS (`swift test`) and on the iOS Simulator (`xcodebuild test`).
