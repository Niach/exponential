# VAPP-4 · iOS findings (SwiftUI)

Simulator: iPhone 17 Pro Max, iOS 27.0 (Xcode 27.0 beta), Debug build, on an Apple silicon Mac. Surface width 440 pt.
All numbers come from `ExponentialUITests/VappSpikeTests.swift` (`testBenchTiming`), with the app's own clock around `layout()` (wall) and `result.layoutNs` (taffy, callbacks included). Device numbers: see `DEVICE-SCRIPT-ios.md`.

## Timing

| tree | pass | nodes | measure calls | taffy µs (best of 5) | wall µs (best of 5) | build µs |
|---|---|---|---|---|---|---|
| kitchen sink | first (screen open) | 48 | 408 | 3284 | 3593 | about 150 |
| kitchen sink | cold (fresh Surface) | 48 | 408 | 2753 | 2992 | about 150 |
| kitchen sink | warm (same Surface, forced) | 48 | 0 | 2 | 268 | |
| bench 200 | first | 203 | 3058 | 12836 | 13803 | |
| bench 200 | cold | 203 | 3058 | 11491 | 12340 | |
| bench 200 | warm | 203 | 0 | 7 | 837 | |

Readings (µs, taffy / wall). Kitchen sink cold: 2844/3078, 3256/3500, 2753/2992, 2952/3193, 2930/3157. Warm: 3/309, 3/313, 3/268, 3/310, 2/319. Bench cold: 11641/12528, 11616/12458, 11491/12340, 11537/12383, 11724/12608. Warm: 8/881, 9/868, 9/882, 8/873, 7/837.

- A cold pass is almost entirely host measurement: about 3.8 µs per SwiftUI `sizeThatFits` callback, at about 15 measure calls per leaf (408 calls for 26 leaves, 3058 for 203 nodes). taffy asks each leaf several times: max-content, then definite widths, then known width with unknown height, then the reverse. The biggest lever is a per-pass memo in the core, keyed by `(index, known, available)`.
- A warm pass costs nothing in taffy (about 8 µs), but wall time is still 0.84 ms for 203 frames. That is uniffi marshalling of `LayoutResult`: RustBuffer serialisation of 203 `PlacedFrame`s with optional strings in `FfiVisual`, about 4 µs per frame. Sending visuals only when they change (or once per node through `nodes()`) would cut it.
- `buildNs` for the kitchen sink is 130 to 190 µs. The first `Surface` in a process took 2.5 ms (cold dylib/allocator).

## Cache behaviour (what `sizeThatFits` received)

The proposals were, in order, `w=0 h=nil`, then `w=44 h=nil` (60 when pushed on the signed-in navigator), then `w=440 h=nil`. `.infinity` and `.unspecified` never arrived in this host (ScrollView > NavigationStack).
- `w=0` is answered from the cache, without a pass.
- `w=44` is a transient NavigationStack probe. The first build ran a full pass for it (415 calls, 2751 pt tall), so I added a floor: widths under 64 pt are answered from the cache (`TaffyLayout.probeWidth`).
- `w=440` runs ONE FFI pass, cached by `(width, pressed ids, nonce)`. `placeSubviews` reuses it (logs show exactly one pass per width), and a zero-width placement is skipped.
- Pressed: a `ButtonStyle` reports `isPressed` to the model, which calls `set_pressed([id])`. The Layout's `pressed` input changes, so SwiftUI runs a new pass. The core returns opacity 0.6 in `visual`, and the painter publishes it after the pass (one async render). I did it through the relayout, not locally.

## Stale cache bug (core; worth fixing)

`layout_fixed()` followed by `layout(measure)` on the SAME Surface reuses the FixedMeasure sizes for subtrees whose available space did not change. In the flex demo, "25%" was laid out 24 wide (8 px × 3 chars) and wrapped. I now seed the paint data from a throwaway Surface. The general issue: taffy's per-node cache is never cleared when the measure function changes, or when host content changes (text edits, Dynamic Type, font load). The FFI needs a `mark_dirty(index)` / `invalidate_measures()`, and `layout(measure)` should clear the cache when the previous pass used a different measurer.

## RTL

SwiftUI DOES mirror a custom `Layout`'s placements under `.environment(\.layoutDirection, .rightToLeft)`. taffy also mirrors (root `direction: rtl`), so the two flips cancel. With `-vappNoUnflip` the header renders avatar-left, and only leaf internals are RTL (`/tmp/vapp4-ios-kitchen-sink-rtl-noflip.png`). The Layout un-flips (`x' = bounds.width - x - w`) whenever the surface is RTL. After that, the header, pills, form row and flex demo match a mirrored web layout (`/tmp/vapp4-ios-kitchen-sink-rtl*.png`). For the RTL run, the host rewrites the fixture's root `"direction": "ltr"` to `rtl` and sets the SwiftUI environment. The FFI has no direction setter.

## Typing (`echo-field`)

The field is `GlassTextField`. It owns the text through `@State`. Every edit bumps a revision and schedules `Task.sleep(150 ms)`, and the echo applies only when its revision is still the latest. Characters were injected with XCUITest `typeText` (40 chars in 0.94 to 1.36 s, simulator software keyboard path).

| run | field value | host echo | result |
|---|---|---|---|
| 1 | all 40 chars | `host: ` + all 40 | pass |
| 2 | all 40 chars | `host: ` + all 40 | pass |
| 3 | all 40 chars | `host: ` + all 40 | pass |

## Accessibility order

- SwiftUI orders a custom Layout's accessibility elements GEOMETRICALLY (row by row, leading to trailing), not by subview order. Without help, "Scan now" came before the header subtitle, and "grow 2 · max 50%" came before "basis 30%".
- The fix is `.accessibilitySortPriority(count - index)` on each subview, so pre-order wins.
- XCUITest's element snapshot (`descendants(matching: .any)`) IGNORES both `accessibilitySortPriority` and `accessibilityHidden`, so it cannot act as a VoiceOver proxy. The test therefore asserts on an in-app walk of the UIAccessibility tree (`VappA11yDump`, the "A11y" button). That walk passes.
- The walk: `Reddit radar, Kitchen sink · one taffy layout on every client, 3, Scan now, Sources, r/selfhosted, r/opensource, r/webdev, Auto-scan, Drafts, Cover, LIVE, <markdown>, Progress, All, Drafts, Sent, Archived, More, Draft reply, Title (echoes after 150 ms), host: , Body, r/selfhosted, Cancel, Send, basis 30% · grow 1, 160 · shrink 0, grow 2 · max 50%, 25%`. That is exactly the fixture pre-order.
- The avatar and divider are hidden. Containers are hidden too: the tree is flat, so there is no per-card grouping (a divergence from the web's DOM).
- Real VoiceOver still needs the device run.

## What painted what (ExpUI)

- `card`: `Color.clear.glassCard()` (fillCard, strokeCard hairline, radius 16).
- `box`: `RoundedRectangle` with the resolved `backgroundColor`/border.
- Tokens: `$palette.*` maps to `DesignTokens.Palette`, `$semantic.*` to `DesignTokens.Semantic`, and `#hex` goes through `Color(hex:)`.
- `text`: `Text` at the frame's size/weight. `lineHeight` = `lineSpacing` plus half-leading padding, so n lines = n × lineHeight. `muted` = `mutedForeground`, `label` = white at 0.7.
- `button`: a hand-built capsule on ExpUI tokens at `controlLg` 36. Primary = `primary` fill; outline = fillCard + strokeCard; ghost = bare.
- `textfield`/`textarea`: `GlassTextField` (verticalPadding 8; textarea `lines: 3...6`).
- `toggle`: `Toggle` + `GlassToggleStyle`.
- `select`: `Menu` + `Picker`, with the trigger styled like a glass field (36 tall).
- `listrow`: HStack + `.flatRow()`.
- `badge`: primary capsule, min 20×20.
- `pill`: `GlassPill` `.sm`; `tone: live` = readonly with a green `dot`.
- `avatar`: `UserAvatar(initials:)`.
- `image`: tinted fill + `AppIcon("image")`.
- `divider`: `GlassDivider`.
- `progress`: linear `ProgressView`.
- `markdown`: `AgentMarkdownText`. It sized correctly inside the Layout, so no fallback was needed.

## Divergences from the web reference and the contract

- Pills are 24 tall (`GlassPill` `.sm`, `controlSm`), not 28. ExpUI has no 28 rung.
- `echo-field` is 57 tall: the `host:` line lives INSIDE the leaf, under the 37 pt field.
- The toggle is 31 tall (UISwitch geometry), and the progress bar is 4 tall, not 8.
- Flat painting cannot clip descendants by nesting. `overflow: hidden` + radius on an ancestor (`media`) is emulated: each descendant masks itself with the ancestor's rounded rect, read in the surface coordinate space.
- Text min-content is approximated. The longest word is sized proportionally from the max-content width, because SwiftUI `Text` at a 0 proposal breaks inside words.
- Rounding: taffy's pixel rounding handed a 33.67 pt two-line text a 33 pt frame, and SwiftUI truncated it to one line. Measured sizes are now ceiled.
- `textAlign` is honoured only via `multilineTextAlignment`. A leaf wider than its text stays leading-aligned. The fixture has no `textAlign`.
- Fonts are fixed pt from the core, so Dynamic Type is not applied (the core owns font size).
- A `@State` initial value is evaluated on every view init, so the model (and a Surface) is built twice per screen open. That costs about 150 µs; a lazy holder would avoid it.

## Swift 6 and the generated bindings

- The kit target compiles in Swift 5 / minimal as specified, with zero warnings.
- The generated `VappSpikeFFI.swift` also typechecks clean under `-swift-version 6 -strict-concurrency=complete` (uniffi emits `Sendable` conformances), so the Swift 5 carve-out is optional.
- App side: `Measure` is `AnyObject, Sendable`, so the host class is `final class …: Measure, @unchecked Sendable`, holding `LayoutSubviews`. Callbacks arrive synchronously on the main thread inside `layout()`, and the bodies run in `MainActor.assumeIsolated`.
- `Surface` is `@unchecked Sendable` (a Mutex on the Rust side), with no friction.

## Size

- `VappSpikeFFI.xcframework`: 41 MB on disk (two 21 MB static `.a` slices, unstripped, with debug info).
- The linked `VappSpikeKit.framework` binary is 2.5 MB (Debug, simulator arm64), taffy + serde_json included.

## Direct open and styleguide

- `-uiTesting -uiTestingScreen kitchen-sink[-bench]` pushes onto the signed-in navigator. With nobody signed in, it shows the screen at the root, so the spike tests need no backend.
- Styleguide: `snapshot("sg_vapp-kitchen-sink")` is taken after `sg_settings-account`, the last signed-in screen. It is reached through an invisible `-uiTesting`-only hook, `open-vapp-kitchen-sink`: 20 pt, trailing edge, mid-height. A top-leading placement did not receive XCUITest taps (status bar or back-swipe edge).
- `VappSpikeTests.testStyleguideHookOpensKitchenSink` signs in against the local seeded backend and proves the hook opens the screen and Back returns. It passes.
- The full styleguide walk (`EXP_SHOTS=sg_vapp-kitchen-sink`) stopped EARLIER, at `StyleguideScreenshots.swift:370` ("No demo@exponential.at login row, is the stub device reporting agent accounts?"). That step needs the shots stub device heartbeat, which was not running, so the captured shot itself is still pending a full shots run.

## Screenshots

`/tmp/vapp4-ios-kitchen-sink.png`, `/tmp/vapp4-ios-kitchen-sink-bottom.png`, `/tmp/vapp4-ios-kitchen-sink-rtl.png`, `/tmp/vapp4-ios-kitchen-sink-rtl-bottom.png`, `/tmp/vapp4-ios-kitchen-sink-rtl-noflip.png` (SwiftUI's double flip), `/tmp/vapp4-ios-bench.png`.
