# Changelog: `ExponentialUI` (SwiftUI)

All notable changes to this package. The SDK packages share one version and
release together from a `eui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## Unreleased

- **Breaking** (over the core) #EXP-1674: `installPackage` refuses a package without `formatVersion: 4`. `Screens.nav`, `Screens.crossAppTarget`, `UrlPolicy.linkBase`.
## 0.13.0 (2026-11-03)

- Switch gains `size` (`default` | `compact`): the compact switch is a 28×16 track under a 12px thumb, as on the other renderers (#1853).

## 0.12.0 (2026-11-03)

- VAPP-233: `RecordMigrations.migrate(package:version:kind:data:)` / `.refusal(before:after:)`; `RecordMigrationsTests` replays `record-migrations.json`.

## 0.11.0 (2026-11-03)

- VAPP-222: `RecordTransitions.refusal/from/validate` and `RecordsQuery.value(…, transitions:, role:)` + `RecordsQuery.transitions(records:kind:)`; `RecordTransitionsTests` replays `record-transitions.json`.

## 0.10.0 (2026-11-03)

- The committed UniFFI bindings match the core again: 0.9.0's `ExponentialUICore` reported `apiChecksumMismatch` at init (`records_value_json` after Q3). Use 0.10.0, not 0.9.0.

## 0.9.0 (2026-11-03)

- No API of its own; carries the core's computed fields (VAPP-214) and sorted install issues (VAPP-201).

## 0.8.0 (2026-11-03)

- VAPP-200: `SurfaceSettings(readOnly: true)`: every writing control paints disabled (the core's read-only build). Conformance suite `read-only`.

## 0.7.1 (2026-11-03)

- No changes of its own; released with the SDK (#1742's specimen look parity and #1744's neutral example names).

## 0.7.0 (2026-11-03)

- VAPP-160: `VoiceWave` hardening. The rings no longer round-trip JSON through the core every frame: `ExponentialUIPrimitives` gains `VoiceWaveGeometry` (the core's `voiceWave` in pure Swift: style.json's constants, per-edge trig tables cached by `VoiceWaveGeometryCache`, a per-frame `frame(activity:level:bands:elapsed:reducedMotion:)` rounded like JS `Math.round`), gated against style.json and replaying every fixtures/voice-wave.json case; the child box and `variant: bars` stay on the core. The rings and the bars stop animating while off screen or in the background. No visual change.
- **Breaking** (over the core): `installPackage` refuses a package without `formatVersion: 3` and validates each kind's record `policy`.

## 0.6.0 (2026-11-02)

- **Breaking**: eApps keeps no backwards compatibility (over the core): `installPackage` refuses a package without `formatVersion: 2` (version 1 is no longer read); `RecordsQuery.parse(params:kind:)` takes the kind only from `kind` (the source's name, `records:<kind>`): a `kind` param is a field filter, and Exponential's `exp:records?kind=…` is no records source; `authorId` is a reserved record key; `Actions.effect(_:)` no longer knows `harness.records.upsert` / `.delete` (records are written only through `records.upsert` / `.delete` / `.batch`).
- **Breaking** VAPP-191: `Expression` is now `Expressions` (`Expressions.evaluate` / `.check`): Foundation (macOS 15 / iOS 18) ships its own `Expression`, so a bare `Expression` was ambiguous in any file importing both (0.5.0's `ExpressionsTests` among them). Compose keeps `Expression`.
- VAPP-194: typed effects (eApps E1). `Actions.run(_:action:params:data:me:nowMs:zone:maxSteps:)` (the plan `{writes, sets, calls}` or the one error), `Actions.applyBatch(rows:writes:records:me:now:newIds:)` (all or nothing) and `Actions.effect(_:)` over the core. `ExponentialHost.callFunction` runs `actions.<name>` as the surface's package action: every planned host call's gate and consent first, then the writes as ONE `records.batch {writes}`, then the data sets, then the calls; a failure lands nothing (ACTION_FAILED + `HostOptions.onActionFailed`). New `HostOptions.me` / `now` / `zone` / `onActionFailed`; `ActionsTests` replays `fixtures/actions.json`.
- VAPP-196: package tests (eApps T1) over the core: `validatePackageJson` reports a package's malformed or failing `tests` (`/tests/<i>`), `runPackageTestsJson` runs them; `PackageTestsTests` replays `fixtures/package-tests.json`.
- EXP-1589 / EXP-1624: parity with the web renderer. A cartesian Chart maps its nice-tick range; a Button or Toggle dims as one group like CSS; a disabled NumberField or ChipInput dims its whole `field`; the Composer send button draws disabled while the draft is empty; Markdown bullets are web's disc; the CodeBlock copy glyph is 12; a ZoomableImage that has not loaded draws its placeholder (the `image` glyph over the alt) and no pill.

## 0.5.0 (2026-11-02)

- VAPP-191: `Expression.evaluate(_:vars:now:timeZone:maxSteps:)` / `.check(_:types:)` over the core's expression evaluator, `TimeZoneOffset` (a `TimeZone` as `HostZone`); `ExpressionsTests` replays `fixtures/expressions.json`. Bind-time `{call: "mul" …}` / `{call: "expr" …}` resolve through the core.
- VAPP-190: `RecordsQuery.parse(json:kind:)` → `(query, issues)` (the JSON form: group, aggregate, running, before, in / between / or) and `RecordsQuery.dateBucket(_:by:)`; `value` answers `groups?`, `totals?`, `before?`. The tests replay `jsonCases`, `bucketCases` and the JSON `valueCases`.
- VAPP-189: `RecordsQuery` (`parse(params:kind:)`, `parse(source:)`, `value(rows:me:now:names:)`) and `RecordRow`: the core's one records evaluator over the FFI; the tests replay `fixtures/records-query.json`.

## 0.4.0 (2026-11-02)

- EXP-1605: `Motion` interactivity (eui-motion README › Interactivity): a `MotionDriver` runs the core's dotLottie state machine (`stateMachine`, else a `.lottie`'s first via `motionReadDotlottieMachine`); `inputs` (literal or bound) write on change; a press (tap, Enter / Space, VoiceOver) posts `click` then fires `on.press`, the button's pointer down / up post `pointerDown` / `pointerUp`, a machine without `on.press` takes the pointer itself; the pointer (iPad, macOS) posts `pointerEnter` / `pointerExit` + `on.hover`; `on.visible` once; `on.state {state}` per entered state; `complete` posted once per finished play + `on.complete` (also on a final state); every state change restarts the clock. With `on.press` a Motion is a button named by `label`. The tests replay `fixtures/motion-state.json` over the FFI and drive a press through the model.
- EXP-1604: `Motion` paints: a `Canvas` under `TimelineView(.animation)` (≤ 60 fps) draws the core's packed display list (`MotionPlayer` over the FFI, `MotionList` + `MotionPainter`): layers as `drawLayer` groups with alpha, the 16 blends, dst-in/out, xor and luma mattes (luminance → alpha), blur / colour-matrix / shadow filters, dashed strokes, linear, radial and two-point conical gradients (interpolated unpremultiplied like the raster), data-URL images; `layers` compile through the new `motionCompile`, `.lottie` sources through `motionReadDotlottie`; theme `$color.*` slots + `$mode`; the shared clock, `progress` scrubs, `play: false` freezes; the poster under reduced motion, `autoplay: false`, off-screen, in the background and past 8 animating per surface; an img named by `label`, hidden without. The tests replay `fixtures/motion-playback.json` and match the raster's parity PNGs.
- EXP-1586: `ZoomableImage` paints: the host-loaded picture zooms and pans through the core's zoom maths (`zoomJson`, `ZoomMaths.core`): pinch about the fingers with a rubber band past the limits, a drag only along an axis with pan room (at rest a swipe reaches the host's pager), the core's momentum, double tap rest ⇄ max(100%, 2 × rest), ⌘/ctrl-scroll and trackpad pinch about the pointer, plain scroll pans, keys `+ = - 0 1` through the surface focus; THE pill from the recipes (− / percent opening THE Menu of the core's rows / + / divider / fit, selected at rest; `controls` auto / always / none); `zoom {scale, atFit}` + `tap`. `ExponentialUIPrimitives` gains `ZoomableImageView` (any natural-size content, an `Image` / `UIImage` / loader convenience), `ZoomController`, `ZoomPill`, `ZoomMaths` and the zoom value types; the tests replay `fixtures/zoom.json`.
- EXP-1493: the Carousel `counter` pill paints at the track's top-END corner (the core's frame, mirrored under RTL); the current page dot takes the `indicator` recipe's `selected` state (neutral: primary), the others its base.

## 0.3.0 (2026-10-31)

- EXP-1495 (round 6): `Outline` and `Mosaic` paint through the Box / Text / Image paths (no painter of their own); the Carousel `counter` pill counts in tabular figures (`TextStyle.tabular`, measure = paint) and the `thumbnails` strip keeps the current page's thumbnail in view.
- VAPP-180 (with the SDK): a package's `exposes` and `upstreams`, validated by the core.

## 0.2.1 (2026-10-30)

- YO-30: the tests replay `fixtures/text-clusters.json`. No painter change.

## 0.2.0 (2026-10-30)

- VAPP-162 (round 5): `QrCode` paints the core's matrix (`qrMatrixJson`, the `QrMatrix` model; a Canvas with module edges on device pixels, the lighter recipe colour on the plate) and `QrScanner` the viewfinder chrome around the HOST camera (`View.exponentialQrScanner(_:)`, `ExponentialQrScanner`, `QrScannerFeed`; no camera = the placeholder); `VoiceWave` `variant: bars` (`voiceWaveBarsJson`, the `VoiceWaveBarsView` primitive); the vertical Segmented and THE rail with count badges; Button `hold` (gesture, keys, focus loss and VoiceOver through `Surface.hold`); `ui-serif` = the system serif; `QrCodeView`, `QrViewfinderCorners`, `QrScanLine` in `ExponentialUIPrimitives`.
- NEW-77: `VoiceWave` paints (a Canvas of three rings from `voiceWaveJson` on a TimelineView, the recipe colours, root opacity, reduced motion rests) with the child rule; the `VoiceWaveView` primitive and `View.voiceWaveChild(scale:anchor:)` in `ExponentialUIPrimitives`.
- VAPP-122: `bind` is an action the host runs like `openUrl`: `{path, source, params?}` re-follows a path (`BindingSource.sourceWithParams` over the core's `sourceWithParamsJson`), subscriptions are keyed by path so the earlier one at that path is cancelled; the conformance suite replays the new `sourceParams` and `records` cases over `validateRecordJson`.

## 0.1.1 (2026-10-23)

- Version-only release: the SDK's npm packages moved to the `@exponential_app` scope and every package shares one version. `ExponentialUI` is unchanged from 0.1.0.

## 0.1.0 (2026-10-23)

First release.

- VAPP-146: `selectPick` of the option that holds the current value (`""` when unseeded) fires nothing; `fixtures/interactions-round1.json` `select` replays in `Round1ModelTests`.
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
