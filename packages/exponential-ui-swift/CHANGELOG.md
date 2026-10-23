# Changelog: `ExponentialUI` (SwiftUI)

All notable changes to this package. The SDK packages share one version and
release together from a `ui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## 0.1.0 (unreleased)

First release.

- VAPP-126 (review #1187): a `display: none` box arrives as `visibilityHidden` from the facade: its zero-framed subtree paints nothing (a SplitView's phone back button drew its glyph at the top-left of a phone surface), takes no taps and leaves the accessibility tree.
- `ExponentialUI`: `SurfaceModel` + `ExponentialSurface` paint an A2UI surface at the frames the Rust core computes, on iOS 17+ and macOS 14+. It measures text in batches with TextKit/CoreText.
- `ExponentialUIPrimitives`: generic SwiftUI primitives themed by `PrimitiveTokens`. Pure SwiftUI, no binary.
- `ExponentialUIFFI.xcframework`: device arm64, simulator arm64 + x86_64, macOS arm64 + x86_64, profile `mobile`.
- Host-owned text fields take the Input `type`'s keyboard, autofill content type and autocorrection. Platform sliders keep the author's range; they take the step only when it divides the range (otherwise the value snaps and still reaches `max`), and are continuous when `step` is 0.
- An extension the core refuses throws: at `ExponentialUI.register`, and when a `SurfaceModel` is created.
- Video and AudioPlayer play their `src` through the media request (AVKit's player for Video; play / pause, a seekable track and the elapsed time for AudioPlayer). An http(s) `src` streams with the request's headers, any size; a policed probe resolves its redirects first. A `data:` `src` plays from a temporary file: at most 8 are kept, each is deleted when no player holds it, and the directory is emptied once per process. A second press while a source opens waits for it. A denied `src` loads nothing, and the controls stay inert.
- Every redirect hop of a media load passes the media policy again: schemes, hosts, no https→http downgrade, and only the headers of the rules the new url matches.
- A failed painter is reported once per component and is not painted again until its props change.
- Markdown link parsing is linear on hostile input, and links nest at most 32 deep.
- Unnamed media fall back to the platform's localized image trait, not to English text.
- Faster passes, the same frames (VAPP-131/133/142): props and leaf records are parsed straight from their bytes, a leaf's measurement is kept between passes (cleared on a theme, mode, font or settings change; Extensions and Markdown are always measured), a freed slot no longer costs a facade read, a large delta reads the visuals in one call, a Table's number cells reuse their formatter. On macOS, against the same bench harness: 10,000 template items paint in 404 ms (was 880) and rebind in 565 ms (was 1,139); the 100,000-row list's first paint is 93 ms (was 161), a scroll step 4.8 ms (was 7.3) with 40 measures (was 78) and 237 facade calls (was 275); a Table step is 5.5 ms and a sort 10.4 ms (was 10.2 and 21.5). The bench itself was corrected in the same change and those parts are not renderer gains: its data generator cost about 190 ms of the list's old first paint, and its counter charged 58 text-style reads per step that were never made (the corrected count is 179).
- `SurfaceModel` reads each step's paint with ONE batched `Surface.paintAt` (the changed slots' visuals and the text styles of their live leaves) instead of a `visual(i)` and a `textStyle(i)` call per slot (VAPP-151). Facade calls per list scroll step 179 → 43 (−76 %); timings unchanged within noise against the same commit without it (macOS, best of 3 × 2 rounds: a 10,000-item rebind 157.0 → 157.7 ms). The trade: each `FfiPaint` adds a 4-byte index, so a stream moves 9,190,883 bytes (was 8,942,883, +2.8 %) and a list step 24,816 (was 24,411, +1.7 %); both budgets are named exceptions.
- Conformance, real-font geometry and fixture replay suites run on macOS (`swift test`) and on the iOS Simulator (`xcodebuild test`).
