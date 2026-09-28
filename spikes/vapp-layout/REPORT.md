# VAPP-4 · Spike report: StyleX-subset layout on taffy, painted by SwiftUI + Compose + gpui

Branch `exp/VAPP-4` (no PR, per instruction). Everything below was built and measured on 2026-09-28. Lane details: `FINDINGS-core.md`, `FINDINGS-desktop.md`, `FINDINGS-web.md`, `FINDINGS-ios.md`, `FINDINGS-android.md`; the authoring probes in `spikes/vapp-authoring/`; device scripts `DEVICE-SCRIPT-ios.md`, `DEVICE-SCRIPT-android.md`.

## Verdict: GO, with one design change the contract must adopt

One taffy layout in Rust, painted by four renderers, gives the same kitchen sink on web, desktop, iOS and Android (`shots/vapp-kitchen-sink/`). Geometry parity is exact where it can be measured: the browser's frames match taffy's within 1/64 px on all 48 nodes at 390 and 900 px, LTR and RTL. Accessibility order is correct on every platform once the natives pin it (both SwiftUI and Compose order a custom layout's children geometrically by default). The typing test passed on all four (40 of 40 characters, in order, with the 150 ms echo). RTL works in taffy 0.12 out of the box.

The one thing that does NOT pass as designed is the measurement protocol: "one FFI upcall per leaf measurement" is too slow on Android with uniffi over JNA (about 50 µs per round trip, 130 ms for a 200-node surface on the emulator) and marginal on iOS (about 3.8 µs per call, 12 ms for 200 nodes on the simulator, Debug). taffy itself needs 0.26 ms for that tree. taffy asks every leaf 10 to 15 times per pass (min-content, max-content, several definite widths, then the known size), so 200 nodes means 2000 to 3000 callbacks. VAPP-42 and VAPP-40 must therefore change the measure contract from per-call upcalls to a batched or Rust-side text measurement (options ranked below). With that change the 2 ms budget is reachable: the Rust side of the pass is 0.3 to 0.5 ms and the host text work is about 20 ms on Android only because it is spread across 2000 calls.

## What was built

| piece | where |
|---|---|
| Fixture, one tree for all four | `packages/domain-contract/fixtures/vapp-kitchen-sink.json` (48 nodes: flex rows/columns with wrap + grow/shrink/basis, a grid with `gridTemplateAreas` + `fr` + `minmax`, an absolute badge overlay, 16:9 `aspectRatio` media, min/max + percentages + gap, a 600 px surface breakpoint, `:pressed` buttons, every catalog control once) |
| Core | `apps/desktop/crates/vapp-spike` (taffy `=0.12.2`, style JSON → resolved map → taffy `Style`, host `Measure` trait, pre-order absolute frames, incremental restyle, RTL, bench generator, 11 tests) |
| Mobile facade | `spikes/vapp-ffi` (uniffi 0.32, standalone workspace; `Surface` object, foreign `Measure` trait with node indices, `build-ios.sh` → xcframework, `build-android.sh` → `.so` + Kotlin) |
| Desktop | `crates/ui/src/vapp_spike.rs`, `EXP_DEV_SCREEN=kitchen-sink`: gpui paints the SHARED frames as absolute divs; gpui's own `Styled` cannot express grid areas or mixed tracks |
| iOS | `apps/ios/Exponential/UI/Spike/` + Tuist framework `VappSpikeKit`: a SwiftUI `Layout` placing every node at taffy's frame, measuring via `sizeThatFits`; `-uiTestingScreen kitchen-sink`; `VappSpikeTests` (typing, a11y walk, bench) |
| Android | `apps/android/.../ui/spike/`: a Compose `Layout` measuring leaves via intrinsics only, placing with `place`; `exp.devScreen` intent extra; `VappSpikeTest` (typing, semantics order, bench) |
| Web reference | `/vapp-kitchen-sink`: the same tree as real CSS (container queries for the surface breakpoint, `:active` for `:pressed`), `?geometry=1` frame dump, Playwright geometry diff + typing scripts |
| Authoring | `spikes/vapp-authoring/`: StyleX runtime, `@stylexjs/dev-runtime`, React Strict DOM native probes, and the `vapp-css.ts` prototype |
| Shots | `shots/vapp-kitchen-sink/{web,web-mobile,desktop,ios,android}.webp` via `packages/shots` (view `vapp-kitchen-sink`, group `ide`) |

## Measurements

### Layout time (200-node bench tree, `bench_tree_json(200)`, 203 nodes, 133 leaves)

| platform | measure calls | taffy incl. callbacks | wall (whole host pass) | build | notes |
|---|---|---|---|---|---|
| Rust only, fixed measure (M-series Mac, release) | 2596 to 2992 | 0.44 to 0.47 ms | n/a | 0.23 ms | the engine itself |
| Desktop gpui (M-series Mac, release) | 1942 cold / 1778 first pass per width / 0 steady | 3.0 ms cold / 1.1 ms / 19 µs | same | | gpui's line cache absorbs repeats |
| iOS simulator (iPhone 17 Pro Max, iOS 27, Debug) | 3058 cold / 0 warm | 11.5 ms cold / 7 µs warm | 12.3 ms cold / 0.84 ms warm | 0.15 ms | 3.8 µs per Swift callback; warm wall = marshalling 203 frames |
| Android emulator (API 36 arm64, Debug) | 2064 / 0 cached | 128 ms / 1 µs | 138 ms / 1 to 3 ms | 0.12 to 0.41 ms | about 50 µs per JNA callback round trip; only ~20 ms is Kotlin measuring |
| Android, Rust only (`layout_fixed`, no JNA) | 1734 | 0.26 ms | | | |
| **real iPhone / mid-range Android phone** | **pending** | | | | `DEVICE-SCRIPT-{ios,android}.md`, asked of Danny |

Kitchen sink (48 nodes, 26 leaves): Rust 0.15 ms / 406 calls; desktop 245 µs first pass per width; iOS 2.8 ms cold, 0.27 ms warm; Android 32 ms full pass, 1 to 3 ms cached.

Reading: the engine is never the problem. Every platform pays per callback: gpui ~1.5 µs (in-process, cached), Swift ~3.8 µs (uniffi RustBuffer round trip), Kotlin ~50 µs (uniffi over JNA). All numbers are Debug on the natives and on emulators; the device rows are pending.

### Geometry parity (browser vs taffy, fixed fake measure)

`apps/web/scripts/vapp-spike/geometry-diff.ts`: 48/48 nodes within ±1 px at 900, 390, 900-rtl, 390-rtl; max delta 0.006 px (Blink's 1/64 px `LayoutUnit`). Three rules make the browser agree with taffy: every Box restates taffy's defaults (`box-sizing: border-box; display: flex; flex-direction: row; position: relative`), chrome (card hairlines, field borders) is paint or inside the measured leaf size, and `overflow: hidden` maps to `overflow: clip`. Ablations: browser-default `content-box` → 1/48 match (32 px off); a real 1 px card border → 7 to 9/48; `min-width: 0` (the React Native default) → 43/48 at 390.

### Accessibility reading order

| platform | default order of a custom layout | fix | verified |
|---|---|---|---|
| Web | DOM order = pre-order | none | DOM |
| Desktop gpui | AccessKit tree = element order | `.id()` + `.role()`/`aria_label` per leaf | compiles, no screen-reader run |
| iOS | GEOMETRIC (row by row) | `.accessibilitySortPriority(count - index)` per subview | in-app UIAccessibility walk = exact pre-order (XCUITest's snapshot ignores sort priority, so the test walks the real tree); VoiceOver by ear pending |
| Android | semantics tree = composition order = pre-order; TalkBack sorts geometrically inside a container | `traversalIndex` per node + `isTraversalGroup` on the surface (not yet applied) | tree order verified by test; TalkBack could not be driven from adb; by ear pending |

### Typing (host-owned `echo-field`, 150 ms echo, 40 fast characters)

| platform | injection | result |
|---|---|---|
| Web | Playwright `pressSequentially`, 5 ms delay, 290 to 400 ms total | 9/9 pass |
| Desktop | input handler (headless) + `cliclick` live | pass, 1 echo applied, 39 dropped as stale |
| iOS simulator | XCUITest `typeText`, 0.9 to 1.4 s | 3/3 pass |
| Android emulator | 40× `performTextInput` and `adb shell input text` (0.28 s) | pass, pass |

Rule that made it work everywhere: the client owns the string, every edit carries a revision, an echo applies only if its revision is still the latest. The layout engine never runs during typing (known width, fixed height).

### RTL

taffy 0.12 honours `direction: rtl` in flexbox, grid, block and absolute placement; the core copies the root's direction onto every node (taffy does not inherit it). Physical properties stay physical, exactly like CSS: `right: 8` keeps the badge on the right, `marginLeft: auto` keeps pushing right. Painter facts: SwiftUI mirrors a custom `Layout`'s placements under RTL (so the painter un-flips), Compose does not with `place()` (only `placeRelative` would), gpui does not, the browser mirrors on `direction: rtl` exactly like taffy.

## The v1 style property whitelist (layout + visual, no transitions, no transforms)

Layout: `display` (flex, grid, block, none) · `flexDirection` · `flexWrap` · `justifyContent` · `alignItems` · `alignContent` · `alignSelf` · `justifySelf` · `flexGrow` · `flexShrink` · `flexBasis` · `gap` `rowGap` `columnGap` · `width` `height` `minWidth` `minHeight` `maxWidth` `maxHeight` (px number, `"N%"`, `"auto"`) · `aspectRatio` (number or `"16/9"`) · `position` (relative, absolute) · `top` `right` `bottom` `left` `inset` · `padding` + `paddingTop/Right/Bottom/Left/Horizontal/Vertical` · `margin` + the same sides (`"auto"` allowed) · `gridTemplateColumns` `gridTemplateRows` (`px`, number, `%`, `fr`, `auto`, `min-content`, `max-content`, `minmax()`, `repeat(n|auto-fill|auto-fit, …)`) · `gridTemplateAreas` (string rows, rectangular) · `gridArea` · `gridColumn` `gridRow` (`n`, `a / b`, `span n`, name) · `overflow` (visible, hidden, scroll) · `direction` (root only).

Visual (painted, never laid out, except `borderWidth` which takes layout space): `backgroundColor` · `color` · `borderWidth` · `borderColor` · `borderRadius` · `opacity` · `boxShadow` (one preset token) · `fontSize` · `fontWeight` · `lineHeight` · `textAlign`. Colours: `#rrggbb[aa]` or `$group.token` into the design tokens.

Conditions: `"@media (min-width: Npx)"` against the SURFACE width, `":pressed"` from the client's pressed set. Both nested objects, resolved on the client.

Changes to the plan the spike forces:
1. **Add logical properties**: `marginInlineStart/End`, `paddingInlineStart/End`, `insetInlineStart/End`. taffy 0.12 has only physical rects, so the client resolves them to physical sides from the root direction (a 20-line step in `style.rs`). Without them, a direction-agnostic vApp cannot place a badge at the trailing edge.
2. **Paint order is tree order**: absolute nodes must come last among their siblings (the protocol validates it) instead of adding `z-index`.
3. `borderWidth` is the only visual key with layout effect; the contract states it.
4. Not in v1 and confirmed unnecessary for the kitchen sink: `calc()`, named lines in track lists, `gridAutoFlow`, `justifyItems`, `order`, `zIndex`, multi-value `padding`/`margin`/`inset` shorthands, `:hover` (D7 says client-side; web can add it later).

## Where the platforms diverged

Layout engine vs browser: nothing beyond 1/64 px in geometry mode. Everything else is painter or platform behaviour:

- **Line breaking.** gpui breaks at `/` ("r/selfhosted" wraps mid-word); SwiftUI `Text` at a 0 proposal breaks inside words (min-content is approximated from max-content); CSS and Compose keep the word. Expected per D7 (font-metric line wraps differ).
- **Control chrome sizes.** The contract said 36/28/32/20 (button/pill/listrow/badge); the platforms' own components are: web Badge 16, Pill 24; iOS pill 24 (no 28 rung), toggle 31, progress 4; Android text field 56 (Material `TextField`), pill 32 → forced 28, button paint borrowed from `GlassSubmitButton`. A catalog control's height must come from the CLIENT's component (VAPP-5 should define per-kind tokens, not px).
- **Flat painting.** iOS and Android paint every node as a sibling of one Layout, so a parent's `opacity` does not cascade and `overflow: hidden` is emulated by clipping descendants to the ancestor's rounded rect. Fine for v1; nesting the native views by tree would remove the emulation.
- **Markdown.** Each platform's renderer sizes itself (TipTap adds paragraph margins on web, Android renders at 16 sp), so the block height differs. Expected.
- **Pressed.** Web `:active` (CSS, no relayout); desktop, iOS and Android relayout through `set_pressed` (43 measure calls, 0.3 ms on desktop; an async render on iOS). `:active` differs from a true pressed state on keyboard activation and touch cancel.
- **Colour tokens.** `$semantic.*` has no CSS variable on web today (hex inlined from tokens.json); the blue shade therefore differs slightly from desktop's theme blue.
- **Echo field frame** includes its "host:" caption on the natives (the caption lives inside the leaf).
- **Text alignment inside a wider leaf** is leading-only on iOS unless `textAlign` is set.

## Authoring package: our own `vapp-css`, not StyleX and not React Strict DOM

Measured in `spikes/vapp-authoring/OUTPUT.md`:
- `@stylexjs/stylex` 0.19.1 without the compiler: `create()` and `defineVars()` THROW ("Styles must be compiled by @stylexjs/babel-plugin"). `@stylexjs/dev-runtime` (latest 0.11.1) fails to load against 0.19.1; its inner `create` returns atomic class-name maps plus CSS text, a web artefact natives cannot use.
- `react-strict-dom` 0.0.55 native build, with a hand-made `react-native` shim (bun pulls the 31 MB `react-native` peer; 178 MB, 187 packages): `css.props` does return plain resolved style objects, BUT it drops `gridTemplateAreas`/`gridTemplateColumns` (warning), only understands StyleX's value-level condition form, and resolves `@media` on the HOST against the SCREEN width. D7 wants conditions resolved on the client against the surface.
- `vapp-css.ts` (about 150 lines, zero dependencies): StyleX's `create`/`props` shape with typed keys, identity + whitelist validation, ordered merge that keeps conditions nested for the client. Validated all 28 kitchen-sink styles unchanged; JSON round-trips byte-identical.

Decision: `packages/vapp-sdk` ships `vapp-css` (Apache-2.0, ours); the key list is generated from the same source as `vapp_spike::WHITELIST` (domain-contract, drift-gated). Authors keep the StyleX API they know; we lose `defineVars` and the value-level condition form until someone asks.

## Recommendations for the contract (VAPP-36), the client (VAPP-42) and the mobile core (VAPP-40)

1. **Measure protocol, in this order of preference.** (a) Move text measurement into Rust (parley or cosmic-text with the platform's system fonts, shaping in-process; only controls are measured natively, a handful per surface, batched in one call). This makes the phone number the Rust number. (b) If text stays native: a batched two-phase protocol (one upcall returning every text leaf's min-content and max-content width, taffy pass, one upcall for the heights at the decided widths, final pass) plus a Rust-side memo per (index, known, available) for the life of a content version. (c) On Android additionally replace JNA with hand-written JNI for the one hot callback (uniffi's JNA interface mapping is the 50 µs). Never ship (c) alone.
2. **`Surface` API additions** (already in the facade after the spike): `mark_dirty(index)` and `invalidate_measures()`; a measurer-identity check so a measurer change clears taffy's cache (iOS found stale sizes); an FFI accessor for the resolved text style per node BEFORE the first pass (Compose intrinsics need the font before measuring); a `set_direction`. Send visuals once per node from `nodes()` and only deltas afterwards (the 0.84 ms warm wall on iOS is marshalling 203 `PlacedFrame`s with optional strings).
3. **Accessibility**: the client emits `PlacedNode.order`; painters MUST pin it (`accessibilitySortPriority` on iOS, `traversalIndex` + `isTraversalGroup` on Android). Group cards as containers for screen readers (the flat tree loses grouping).
4. **Whitelist**: adopt the list above plus the logical inline properties; per-kind control heights as tokens, not px; absolute-last-among-siblings validated by the protocol.
5. **Fixtures**: `frames-{390,900}[-rtl].json` are the seed of `vapp-layout.json`; the geometry-diff script is the seed of VAPP-13's browser check.
6. **Desktop**: keep painting shared frames (never map onto gpui's Styled); gpui's line cache already makes it the fastest native.

## Open items handed to Danny (real hardware)

- iPhone: bench captions for the kitchen sink and the 200-node tree (Release), VoiceOver sweep order, hand typing test. `DEVICE-SCRIPT-ios.md`.
- Mid-range Android phone: the same three, plus the TalkBack sweep (adb cannot drive TalkBack gestures). `DEVICE-SCRIPT-android.md`.
The verdict does not hinge on them: the emulator/simulator numbers already show the per-callback cost is the design issue, and the fix is protocol-level.
