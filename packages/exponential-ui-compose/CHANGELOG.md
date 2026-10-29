# Changelog: `at.exponential:ui-compose` + `ui-compose-primitives`

All notable changes to this package. The SDK packages share one version and
release together from a `ui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## Unreleased

- VAPP-162 (round-5 contract): `QrCode` paints the core's matrix (`qrMatrixJson`, `qr-code.json` bit for bit) on its plate, module edges snapped to device pixels, dark on light in every mode; `QrScanner` paints the viewfinder chrome (corners, the 2 s scan line, the `Camera unavailable` placeholder) around the host's camera through the new `LocalQrScanner` (`QrScannerHost.Feed(formats, report, modifier)`; `scanned {value}` once per distinct value); `VoiceWave variant: bars` from `voiceWaveBarsJson`; the vertical Segmented and THE rail (72 × items × 56) with `option.count` badges (`99+`, read after the label); Button `hold` through `Surface.hold` (pointer claimed on down, the core's 24 px slop, Space/Enter/Escape, focus loss cancels; TalkBack's click = one whole hold); `ui-serif` = `FontFamily.Serif`. `Round5Test` replays `qr-code.json`, `voice-wave.json bars` and `interactions-round1.json buttonHold`.
- NEW-77: the `VoiceWave` native paints: three closed waves from the core's `voiceWaveJson` (filled / stroked in the `VoiceWave/outer|mid|inner` recipe colours, the root recipe's box and opacity), animated per display frame while listening, thinking or speaking (still under reduced motion), its one child (THE CHILD RULE) at the core's centred frame at its own size, drawn scaled about the box centre by the core's `child.scale` (min(1, childSize / its longer edge), never up) and kept but unpainted at `sm` (no input, no semantics), in the `VoiceWave/child` colour; TalkBack reads one image `<label>, <activity>` (hidden without a label). `ui-compose-primitives` gains `VoiceWaveView` (with a child overload) / `DrawScope.drawVoiceWave` / `Modifier.voiceWaveChild`; `fixtures/voice-wave.json` replays in `Round2InteractionTest` within 0.5 px (the child box too) and `ControlPaintTest` measures the drawn child against its `child` cases within 0.5 px.
- VAPP-122: `bind` is an action the host runs like `openUrl`: `{path, source, params?}` re-follows a path (`sourceWithParams` over the core's `sourceWithParamsJson`), subscriptions are keyed by path so the earlier one at that path is cancelled; the conformance suite replays the new `sourceParams` and `records` cases over `validateRecordJson`.

## 0.1.1 (2026-10-23)

- Version-only release: the SDK's npm packages moved to the `@exponential_app` scope and every package shares one version. Coordinates unchanged (`at.exponential:ui-compose`, `at.exponential:ui-compose-primitives`); same contents as 0.1.0.

## 0.1.0 (2026-10-23)

First release.

- VAPP-146: `selectPick` of the option that holds the current value (`""` when unseeded) fires nothing; `fixtures/interactions-round1.json` `select` replays in `InteractionTest`.
- VAPP-126 (review #1187): a `display: none` box arrives as `visibilityHidden` from the facade: its zero-framed subtree is inert and hidden from accessibility, as on the web.
- `:ui-compose`: `SurfaceModel` + `ExponentialSurface` paint an A2UI surface at the frames the Rust core computes on Android 8+. The AAR bundles the UniFFI facade (arm64-v8a, armeabi-v7a, x86_64) and its Kotlin binding.
- `:ui-compose-primitives`: generic Compose primitives themed by `PrimitiveTokens`.
- Host-owned text fields set the Input `type`'s keyboard, autocorrection and autofill hints (email, password, phone).
- Material sliders keep the author's range. `step` 0 is continuous; a step that divides the range is passed as M3 `steps`.
- An extension the core refuses fails `SurfaceModel` creation; it is no longer skipped silently.
- Video and AudioPlayer play their `src` through the media policy with Media3 ExoPlayer (`androidx.media3:media3-exoplayer` + `media3-ui` 1.11.1): poster and duration until the press, `autoplay` muted, a header-carrying or `data:` request fetched within `media.limits` into a cache file; a denied src loads nothing.
- Media redirects: each hop is re-policed (a hop off a rule's prefix loses its headers) and an https → http hop is refused.
- Video / AudioPlayer: http(s) streams with its headers after a policed redirect probe, pinned to the resolved origin (no byte cap, so authed media over 20 MB plays); only `data:` goes to a file, uncapped.
- Video / AudioPlayer: one open per player; a second press waits (spinner, control disabled), another src cancels it, a replaced player is released.
- `data:` media files are refcounted and deleted by the last player, at most 8 kept, the cache directory emptied on the first use per process.
- An `Image` without `alt` paints only the glyph (no English "image" label).
- `ChoiceTiles` (EXP-1280): the core expands it into tiles; a tile reads as a radio (single) or a checkbox (multiple) with its picked state, arrows rove the tiles (a single choice follows focus), and an unexpanded one measures as its grid. `:ui-compose-primitives` adds the standalone `ChoiceTiles` primitive (picture over label + muted description, 2 dp accent ring on a pick, radius 10, gap 8, picture 1.6 : 1).
- Markdown inline parsing is linear on hostile input (unclosed destinations, bracket and emphasis floods); link labels nest at most 32 deep.
- Paint failures are keyed by component: a failed node paints empty, unreported, until ITS props change; it drops when it leaves the surface. The host dedupes per surface + component and forgets one when `updateComponents` names it.
- Markdown block images align to the top of their box.
- `SurfaceModel.snap` reaches `max` when the step does not divide the range.
- The example's `brand` theme loads with `ThemeHandle.loadOrDefault`; a unit test loads every shipped sample theme strictly.
- Text `shimmer` paints a moving gradient (`textShimmerJson` colours, plain under reduced motion); `Trace` / `TraceItem` and CodeBlock `variant` / `size` / ANSI tokens paint from the core; CodeBlock copy copies the `copy` part's `text` (ANSI escapes stripped).
- Faster passes (VAPP-134 to 138): nodes are patched from the layout's delta (`nodesAt`), visuals and text styles are re-read only for the changed nodes and the leaves under them, theme scalars and wrapped line counts are memoised, and ICU formatters load on first use. A 10,000-item template paints in half the time; a Table scroll step costs 0.5×; the facade moves 57 % less data over a stream.
- `SurfaceModel` reads each step's paint with ONE batched `Surface.paintAt` (the touched slots' visuals and the text styles of the live leaves under them) instead of a `visual(i)` / `textStyle(i)` call per slot. Facade calls per list scroll step 143 → 82; on the emulator a streamed message applies 2.1× faster (p95 29 → 9.4 ms), a mode switch 3.9×, a 10,000-item rebind 1.3×. Budget exception: the streaming scenario's `ffiBytesTotal` rises 2.4 % (5,212,191 → 5,336,991 B, each re-read leaf carries its visual in the batched read) for 43 % fewer facade calls per step; no timing budget moved.
- Robolectric unit, conformance and snapshot suites, plus instrumented tests (`src/androidTest`) that run on an emulator in CI.
