# `ui-compose`: the Jetpack Compose painter

This is the Android renderer of the Exponential UI SDK (VAPP-89), the Compose twin of the SwiftUI painter
[`packages/exponential-ui-swift`](../exponential-ui-swift).

The Rust core [`exponential-ui`](../../apps/desktop/crates/exponential-ui) does three jobs:
- reduces an A2UI surface,
- resolves the theme,
- lays the tree out with taffy.

This library measures text for the core in batches. It then paints the result in Compose at the frames the core computed.

Android 8+ (minSdk 26). No WebView, no downloaded code, nothing from the Exponential app.

| module | artifact | what | depends on |
|---|---|---|---|
| `:ui-compose` (`ui-compose/`) | `at.exponential:ui-compose` | The painter: `SurfaceModel` + `ExponentialSurface`, the host plugin, extension painters. | The UniFFI facade [`exponential-ui-ffi`](../../apps/desktop/crates/exponential-ui-ffi) (the generated Kotlin binding compiled in; `libexponential_ui_ffi.so` for arm64-v8a, armeabi-v7a and x86_64; JNA 5.17 `@aar`), `:ui-compose-primitives` |
| `:ui-compose-primitives` (`primitives/`) | `at.exponential:ui-compose-primitives` | The generic Compose primitives the catalog natives are painted with, themed by `PrimitiveTokens`: pill, segmented control, field chrome, drawn switch, avatar, meter track, ring, the markdown model + `MarkdownView`, empty state, disclosure header. | nothing (pure Compose) |
| `:example` (`example/`) | (app `at.exponential.ui.kitchensink`) | The kitchen-sink app. | `:ui-compose` only |

The Exponential Android app includes `:ui-compose-primitives` by path (the SLOP-18 convergence) without linking the core. That is why the primitives use literal dependency coordinates.

## Build

```bash
bash apps/desktop/crates/exponential-ui-ffi/build-android.sh   # → out/jniLibs/<abi>/libexponential_ui_ffi.so (gitignored)
cd packages/exponential-ui-compose
./gradlew :ui-compose:assembleRelease                         # the AAR
./gradlew :example:installDebug                               # the kitchen sink on a device / emulator
```

- **The binding.** `ui-compose` compiles `exponential-ui-ffi/bindings/kotlin` (package `at.exponential.ui.ffi`) as a second source dir and takes the `.so` files from `out/jniLibs`.
- **R8.** The consumer rules keep JNA and the binding, so an app with R8 on needs nothing extra.
- **Publishing.** `maven-publish` is wired with group `at.exponential`, version `0.1.0`; uploading is VAPP-91.

### In a blank project

`./gradlew :ui-compose:publishReleasePublicationToMavenLocal :ui-compose-primitives:publishReleasePublicationToMavenLocal`
puts both AARs in `~/.m2`. A blank Android Studio project (Compose template, AGP 8.10, Kotlin 2.4,
minSdk 26) then needs only `mavenLocal()` in its repositories and
`implementation("at.exponential:ui-compose:0.1.0")`; the POM brings the primitives, Compose,
coroutines and JNA (`@aar`). Ten lines of `MainActivity` (a `SurfaceModel`, `setNested` with the
kitchen sink from `assets/`, `ExponentialSurface` in a `verticalScroll`) render the kitchen sink
with R8 on and no extra rules. Verified on the emulator on 2026-10-08.

## Embedding

```kotlin
val model = remember {
    SurfaceModel("s1", SurfaceOptions(theme = ThemeHandle.builtin("exponential"), mode = Mode.Dark), host = MyHost()) { error("set by ExponentialSurface") }
}
LaunchedEffect(Unit) { model.apply(a2uiMessage) }   // or setNested / setComponents / setData(path, value)
Column(Modifier.verticalScroll(rememberScrollState())) {
    ExponentialSurface(model)                       // as wide as its container, as tall as the content
}
```

- **Host.** `HostPlugin` gives every member a default. `ClosureHost(icons, actions, inputs, urls, unknowns)` builds one from lambdas.
  - `icon(name, size)` maps the catalog's registry concepts to your composables, painted under `LocalContentColor`. `null` paints the placeholder circle.
  - `onAction` receives every A2UI action.
  - `onInput` receives host-owned text edits: `Change` is debounced 150 ms and carries a revision; `Commit` fires on blur or IME Done.
  - The rest: `openUrl` (default: `ACTION_VIEW` through `ExponentialUi.appContext`), `resolveUrl`, `onUnknown`, `fontFamily`, `markdown`.
- **Fonts.** `ExponentialUi.registerFont(family, FontFamily)` registers a family a theme names. A missing family falls back to the platform default.
- **Themes.** Load one with `ThemeHandle.builtin(id)` or `ThemeHandle.load(json, parents)`; switch with `model.setTheme` / `setMode`.
  - Painters never read recipes: the core hands them resolved visuals.
  - Sub-parts the core does not synthesize (a Checkbox `check`, a Switch `thumb`, a Select `trigger`…) resolve through the facade's `Theme` object (`model.part(component, part, props)`), cached per query.
- **Extensions.** Register with `ExponentialUi.register(extensionJson, mapOf(kind to painter))`.
  - An `ExtensionPainter` answers `measure(leaf, wrap)` (the border box) and `@Composable Paint(context)` (the content inside the frame).
  - `context.emit` fires the node's `on` handlers.
- **Viewport.** The width comes from the surface's own layout (`onSizeChanged`, reported after layout, never during composition). `model.setViewport(width, height, maxHeight)` sets the visible height (dialog centring, windowed lists) and a card bound.

## The painting model

- **Frames and order.** The core returns absolute frames (dp) in surface coordinates. Pre-order is both paint order and accessibility order.
- **`FrameLayout`.** A custom `Layout` measures each child with `Constraints.fixed` at its frame and `place`s it, never `placeRelative`, relative to its container. It never asks for intrinsics.
- **Nesting.** Containers nest like the tree, so `overflow: hidden`, radius clips and opacity inherit without emulation.
- **Direction.** The surface pins `LocalLayoutDirection` to Ltr, because an RTL surface has already been mirrored by the core.
- **Font scale.** The surface pins `fontScale = 1`: sizes are the theme's.
- **Boxes.** A container paints its box: background, radius, shadows (CSS blur → a `BlurMaskFilter`), border and opacity (`ModulateAlpha`, so shadows survive). A measured leaf paints inside the recipe padding the measurer counted.
- **Text.** ONE `TextMeasurer`, created by `ExponentialSurface` on that density, both measures and paints. The style is the same in both places (`lineHeight` with `LineHeightStyle(Center, Trim.None)`, `includeFontPadding = false`), so the measured height is the painted height and `n` lines = `n × lineHeight`.
- **Text colour.** The node's own, else the nearest ancestor's, else the theme's `foreground`.

## Measurement (the VAPP-4 verdict)

`SurfaceMeasurer` implements the facade's batched `Measurer` and crosses at most three times per pass:
- `measureIntrinsics`: min-content, max-content and the height at max-content for every leaf, in ONE call.
- `measureHeights`: the leaves whose width came out narrower.

The core memoizes the answers per measure identity, so a resize to a width it has seen makes no upcall.

Every answer is the BORDER box: the recipe's padding and border around the content, with fixed and minimum sizes winning. The rules are a verbatim port of the Swift measurer (the gpui rules):
- a `lines: 1` text shrinks to 0 at min-content;
- Select and DatePicker fields size from the `trigger` recipe;
- Markdown measures with the same block layout it paints;
- platform controls have recipe-fixed boxes.

Nothing is measured through Compose intrinsics, so a pass runs headless (JVM tests).

## Controls

- **Native or drawn.** Controls are platform-native where users expect it, and drawn when the theme says so (`native: false` on the part's recipe; the built-in themes set it on the Switch track).
- **Switch.** An M3 `Switch` scaled into the track frame, or the primitives' `DrawnSwitch`.
- **Slider.** An M3 `Slider`, or the drawn range + thumb with a drag that snaps to `step`.
- **DatePicker.** An M3 `DatePickerDialog`, date only, with `min` / `max` as `SelectableDates`.
- **Select.** An M3 `DropdownMenu` on the trigger, with a filter field when `searchable`; it stays open per pick when `multiple`.
- **DropdownMenu.** An M3 `DropdownMenu`, its open state driven by the core's layer.
- **Text fields.** Input, Textarea and Composer are host-owned `BasicTextField`s (`OwnedTextField`):
  - the view owns its `TextFieldValue`;
  - every edit carries a revision;
  - a model write (an echo, the composer clearing) reloads the view only when `FieldState.writeGeneration` moves.
- **Unbound values.** Unbound Checkbox, Switch, Radio, Toggle, ToggleGroup, Select, DatePicker and Slider values live in a local mirror. The painter re-resolves part visuals from the mirror until the prop changes.

## Overlays

`SurfaceOptions.overlays` chooses how layers present.

**`Native` (the default):**
- **Dialog:** a Compose `Dialog`, back and outside-tap only when `dismissible`.
- **Drawer:** an M3 `ModalBottomSheet`, which refuses Hidden when not dismissible. A second modal stacks.
- **Popover:** a focusable `Popup` on the side the core placed it. It flips and clamps into the window.
- **DropdownMenu:** an M3 menu.
- **Tooltip:** painted in the surface at the core's frame after a long press.

**`Painted`:** draws every layer inside the surface at the core's frames, with a scrim. Use it for snapshots and for hosts that own their windows.

## Windowed lists

The core windows a `List` past 24 rows.
- The rows sit at content offsets inside a `verticalScroll` whose content is as tall as the core says.
- The scroll offset goes back through `scroll(list, offset)`, which moves the window in constant work.
- No `LazyColumn`, no guessed row heights.

## Accessibility

TalkBack reads pre-order. This is the VAPP-4 fix, which needs both halves:
- **Every node** carries `semantics { traversalIndex = index; isTraversalGroup = true }`.
- **The surface** is a traversal group.

Node kinds:
- **Pressable containers** become ONE button labelled from their leaves (`combinedLabel`, the children cleared).
- **Card / Group / Alert** containers are labelled groups.
- **Leaves** carry their role, label and state: Tab `selected`, toggleable state, slider range + `setProgress`, image, button.
- **Modal layers** block the rest.

The example's `a11yDump` writes the walk Compose's accessibility delegate hands TalkBack. The real TalkBack walk on the emulator matches it item for item (see Numbers).

## Example app

`example/` is a blank Android app that links `:ui-compose` and nothing else (plus `material-icons-core` for its icon map). It renders `packages/exponential-ui/fixtures/kitchen-sink.json`, which the build copies into the assets together with `theme-extends.json`.

- **Chrome.** A theme dropdown (the built-ins + `brand`, the third-party acceptance theme of `theme-extends.json`: `extends: neutral`, a new primary, the button radius) and a Dark / Light segmented toggle.
- **Host.** It echoes every input after 150 ms (`setData` on the bound path + the monospace `host:` line, `testTag("host-echo")`) and logs actions to the `ExponentialUI` logcat tag.
- **Icons.** The kitchen sink's concepts are mapped to Material outlined icons. A real host hands the renderer its own registry.

Launch extras (`adb shell am start -n at.exponential.ui.kitchensink/.MainActivity …`):

| extra | effect |
|---|---|
| `--es shot <view>` | hides the chrome. What `bun run shots --platform android --views exponential-ui-kitchen-sink` captures (release build). |
| `--es theme exponential\|neutral\|playful\|brand`, `--es mode light\|dark` | the look |
| `--ez rtl true` | the root `direction` becomes `rtl` |
| `--ei width <dp>` | pins the surface width |
| `--es overlays painted` | painted layers |
| `--es dump <path>` | 2 s after the first pass, the WHOLE surface (every pixel, not the viewport) as a PNG. A path the app may not write lands in `/sdcard/Android/data/at.exponential.ui.kitchensink/files/`. |
| `--es a11yDump <path>` | 2 s after the first pass, the TalkBack walk, one label per line, then a semantics trace after `---` |
| `--ei bench <n>` (or `--es shot bench`) | `benchTreeJson(n)` instead of the kitchen sink |
| `--ez benchLoop true` (or a tap on the `host:` line) | the timed pass series: one `exponential-ui: surface=… phase=… layout_ns=… wall_ns=… upcalls=… measure_calls=…` line per pass + `summary` lines |

## Tests

- **`./gradlew :ui-compose:testDebugUnitTest`** runs on the JVM (Robolectric) against the host build of the facade. Export `CARGO_TARGET_DIR` (the tests load `$CARGO_TARGET_DIR/release/libexponential_ui_ffi.dylib` through `jna.library.path`). Each test class runs in its own JVM. The suite covers:
  - the fixture replays (the kitchen sink in every theme and mode, the catalog component cases, macros, the extension fixture);
  - the geometry of `layout-geometry.json`;
  - the measurer rules;
  - interaction (actions, mirrors, the field debounce + revisions + echo rule, a 40-key burst, layers, windowed lists);
  - `src/test/snapshots/components.json`, the painted tree of every component case under the fixed measure;
  - Roborazzi image snapshots (`src/test/snapshots/images/`).

  `EXPONENTIAL_UI_RECORD=1` rewrites the snapshots.
- **`./gradlew :example:connectedDebugAndroidTest`** runs on a device:
  - `AccessibilityOrderTest`: the TalkBack walk starts `Alex Chen, Reddit radar, Kitchen sink · one catalog on every client, 3, Scan now, Sources`, keeps the iOS orderings, and has more than 60 labels.
  - `TypingTest`: 40 characters in one `performTextInput` burst land in order in the Title field and on the `host:` line.

## Numbers

From the release build of the example (R8, non-debuggable) on the `Medium_Phone_API_36.0` emulator (arm64, API 36). They are indicative only, since this is an emulator. `layout` = the core's pass including the JNA upcalls and the Kotlin measurer.

| pass | bench 200 (205 nodes) | kitchen sink (287 nodes) |
|---|---|---|
| cold, the first pass of a process (5 launches) | 15.0 to 18.5 ms layout, 25 to 27 ms wall (41 ms on the first launch after install); 2 upcalls, 379 answers | 29.7 to 37.2 ms layout, 42 to 48 ms wall; 2 upcalls, 371 answers |
| warm full pass, width flipping 390 / 411 dp | **0.78 to 0.79 ms** best, 0.87 to 0.89 median (wall 1.09 / 1.20); 0 upcalls, memo answers | 1.94 to 2.1 ms best, 2.19 to 2.28 median; 0 upcalls |
| the same under the core's fixed measure (no JNA, no Kotlin) | 0.76 to 0.78 ms best | 1.93 to 2.18 ms best |
| full re-measure (a new measure identity: every leaf crosses) | 2.8 to 3.4 ms best, ~3.6 median | 5.7 to 6.2 ms best, ~6.9 median |
| of which Kotlin (the measurer) / the two JNA crossings | ~1.7 ms / ~0.9 ms | ~3.4 ms / ~1.6 ms |
| cached pass (nothing dirty) | 0.02 ms layout, 0.23 to 0.42 ms wall | 0.02 ms layout, 0.25 ms best wall |

**Reading:**
- The target, under 2 ms warm for 200 nodes including JNA, is met: 0.8 ms.
- The kitchen sink's warm pass is the engine itself, with a fixed measure just as fast.
- A full re-measure costs about 2 ms of Kotlin text measuring plus about 0.5 ms per JNA crossing. That crossing cost is the size of the batch payload, not a per-leaf callback (the VAPP-4 spike paid 15 to 80 µs per leaf).

The TalkBack walk (real TalkBack, swipes on the emulator's virtual touchscreen) reads the kitchen sink in pre-order from the avatar to the `host:` line. Inside the open Dialog it stays in the dialog: title, description, body, `Delete for good`, Close, then it wraps.

## Known divergences from the React renderer

- Line breaks follow Android's text layout; the `lines` clamp ellipsizes.
- Skeleton is static. Images load through a small `BitmapFactory` loader: http(s), file, content and android.resource URIs, downsampled to 2048 px. `data:` URIs and SVG are not supported, and the host app needs the INTERNET permission. Video and AudioPlayer are static placeholders.
- Native M3 controls (Switch, Slider, menus, the DatePickerDialog) take M3 defaults apart from the primary tint; the DatePickerDialog takes the host's `MaterialTheme`. The M3 Slider keeps a 16 dp thumb (not M3's 44 dp bar) and a 48 dp touch height, both overflowing the 6 dp track frame vertically.
- A pressable container clears its children's semantics (`clearAndSetSemantics`, the SwiftUI `.ignore`), so a nested pressable inside a pressable row is not separately reachable.
- Markdown paints with the primitives' `MarkdownView`. It rounds the wrap width while the measurer takes the ceiling, so a paragraph exactly on a wrap boundary can paint one line off (the leaf clips to its frame).
- The Ring's value label is wider than the 32 dp ring and clips at a narrow card edge (the shared measurer rule; iOS does the same).
- TalkBack reads a labelled field's label twice (the `Label` node, then the field's own description), as it reads the slider's value twice.
- Painted mode has no back handling (no `activity-compose` dependency). Native mode gets it from the Dialog and the sheet.
