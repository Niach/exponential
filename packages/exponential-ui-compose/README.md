# `ui-compose`: the Jetpack Compose painter

This is the Android renderer of the Exponential UI SDK (VAPP-89), the Compose twin of the SwiftUI painter
[`packages/exponential-ui-swift`](../exponential-ui-swift).

The Rust core [`exponential-ui`](../../apps/desktop/crates/exponential-ui) does three jobs:
- reduces an A2UI surface,
- resolves the theme,
- lays the tree out with taffy.

This library measures text for the core in batches. It then paints the result in Compose at the frames the core computed. It implements the round 1 and round 2 contracts ([`docs/round-1-contract.md`](../exponential-ui/docs/round-1-contract.md), [`docs/round-2-contract.md`](../exponential-ui/docs/round-2-contract.md)).

Android 8+ (minSdk 26). No WebView, no downloaded code, nothing from the Exponential app.

| module | artifact | what | depends on |
|---|---|---|---|
| `:ui-compose` (`ui-compose/`) | `at.exponential:ui-compose` | The painter: `SurfaceModel` + `ExponentialSurface`, the host plugin, extension painters. | The UniFFI facade [`exponential-ui-ffi`](../../apps/desktop/crates/exponential-ui-ffi) (the generated Kotlin binding compiled in; the generated catalog object `at.exponential.ui.ExponentialUICatalog` from `packages/exponential-ui/generated`; `libexponential_ui_ffi.so` for arm64-v8a, armeabi-v7a and x86_64; JNA 5.17 `@aar`), `:ui-compose-primitives` |
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
  - The rest: `urlPolicy` + `mediaOptions` (null = the contract defaults), `openUrl` (gets only hrefs the URL policy allowed; default: `ACTION_VIEW` through `ExponentialUi.appContext`), `resolveUrl` (a src rewrite BEFORE the media policy), `onPaintError` (a painter failed; once per component + message), `onUnknown`, `fontFamily`, `markdown`, `copy` (default: the clipboard), `pickFiles` (a FileUpload asks; answer `request.done(files)`).
- **Fonts.** `ExponentialUi.registerFont(family, FontFamily)` registers a family a theme names. Text without a family uses the theme's `sans`; a family that is not registered falls back to the platform default.
- **Themes.** Load one with `ThemeHandle.builtin(id)` or `ThemeHandle.load(json, parents)`; switch with `model.setTheme` / `setMode`.
  - Painters never read recipes: the core hands them resolved visuals.
  - Sub-parts the core does not synthesize (a Checkbox `check`, a Switch `thumb`, a Select `trigger`…) resolve through the facade's `Theme` object (`model.part(component, part, props)`), cached per query.
  - Tokens and sub-parts come from `model.effectiveTheme` (`Surface.effectiveTheme()`: `extends`, density and contrast applied), so they match the core's layout under `compact` or high contrast. `model.theme` stays the theme you set.
- **Extensions.** Register with `ExponentialUi.register(extensionJson, mapOf(kind to painter))`.
  - An `ExtensionPainter` answers `measure(leaf, wrap)` (the border box) and `@Composable Paint(context)` (the content inside the frame).
  - `context.emit` fires the node's `on` handlers.
- **Viewport.** The width comes from the surface's own layout (`onSizeChanged`, reported after layout, never during composition). The visible part comes from the window (`onGloballyPositioned`): unbounded lists window against it, sticky pins and viewport layers follow it. `model.setViewport(width, height, maxHeight)` pins the visible height yourself.
- **Settings** (`SurfaceOptions.settings`, `model.setSettings`). `null` members follow the device, which `ExponentialSurface` reads:

  | setting | default |
  |---|---|
  | `locale`, `timeZone` | the device's (`en-US` / `UTC` headless) |
  | `strings` | `{}` (overrides of `catalog/strings.json`) |
  | `followSystemMode` | `false` (`true` = `system` mode) |
  | `density` | `default` (`compact`, `comfortable`) |
  | `contrast` | `normal` (`high`, `system`) |
  | `fontScale` | the device's font scale (the core scales the type; painting stays at 1) |
  | `reducedMotion` | animator duration scale 0 |
  | `hover` | a mouse is connected |
  | `insets` | the window's safe-drawing insets |
  | `formatter` | `IcuFormatter(locale, timeZone)` (`android.icu`) |

- **Commands.** `model.command(json)` takes `{focus: {id}}`, `{announce: {text, live}}`, `{scrollIntoView: {id}}`, `{scrollToIndex: {id, index, align}}`; `model.scrollToIndex(id, index, align)` is the shorthand.

## The painting model

- **Frames and order.** The core returns absolute frames (dp) in surface coordinates. Pre-order is both paint order and accessibility order.
- **`FrameLayout`.** A custom `Layout` measures each child with `Constraints.fixed` at its frame and `place`s it, never `placeRelative`, relative to its container. It never asks for intrinsics.
- **Nesting.** Containers nest like the tree, so `overflow: hidden`, radius clips and opacity inherit without emulation.
- **Direction.** The surface pins `LocalLayoutDirection` to Ltr, because the core has already mirrored an RTL node. A leaf shapes its text with ITS direction as the bidi paragraph direction and the core's physical `textAlign`. Only `rtlMirroredIcons` flip (`scaleX = -1`, outermost).
- **Font scale.** The surface pins `fontScale = 1`; the device's scale reaches the core, which scales the theme's type.
- **Boxes.** A container paints its box:
  - background + gradient (CSS angle) on per-corner radii (CSS clamping);
  - shadows (CSS blur → a `BlurMaskFilter`);
  - the border inside the box: per-side widths in one colour, `dashed` / `dotted`;
  - opacity (`ModulateAlpha`, so shadows survive), the paint-only `transform` about the centre, `visibility: hidden` (inherited: out of the tree for TalkBack, no taps, no focus);
  - `backdropBlur` paints the translucent background alone (see the divergences).

  A measured leaf paints inside the recipe padding the measurer counted (per side).
- **Motion.** `transition` animates background, border colour and opacity over its ms with its cubic bezier. A style `animation` paints the core's frame (`animationFrameJson`) every display frame from the node's entry; infinite ones run on `withInfiniteAnimationFrameNanos`. Reduced motion paints the rest frame.
- **Scrolling.** The core's `scrolls` (any `overflow: scroll | auto`, windowed lists, a Dialog body) become Compose scrollers on one or both axes. Frames stay unscrolled; every offset goes back through `scrollTo(id, x, y, fromView = true)`.

  | the scrolled container | per scroll step |
  |---|---|
  | plain (no windowed list, no sticky node inside) | nothing: Compose already moved the paint |
  | a windowed list or a sticky node inside | one layout pass (the window or the pins move) |

  An offset the core sets (`scrollIntoView`, `scrollToIndex`, a clamp, a key) scrolls the view (`scrollEpoch`). The visible height the surface reports to the core applies at once when it grows by 48 dp or more, otherwise after 150 ms at rest.
- **Sticky.** `position: sticky` nodes and pinned List headers paint at their frame plus the core's `(dx, dy)`, above their siblings.
- **Text.** ONE `TextMeasurer`, created by `ExponentialSurface` on that density, both measures and paints. The style is the same in both places (`lineHeight` with `LineHeightStyle(Center, Trim.None)`, `includeFontPadding = false`, `letterSpacing`, `TextMotion.Animated` = unhinted fractional advances, as Chromium and gpui), so the measured height is the painted height and `n` lines = `n × lineHeight`. Widths are the FRACTIONAL shaped extent, as on the web and gpui. Frames round to whole px and Compose ceils a text's intrinsic width, so a text up to `TEXT_SLACK_PX` (2 px) wider than its box still paints on one line.
- **Text colour.** The node's own, else the nearest ancestor's, else the theme's `foreground`.

## Measurement (the VAPP-4 verdict)

`SurfaceMeasurer` implements the facade's batched `Measurer` and crosses at most three times per pass:
- `measureIntrinsics`: min-content, max-content and the height at max-content for every leaf, in ONE call.
- `measureHeights`: the leaves whose width came out narrower.

The core memoizes the answers per measure identity, so a resize to a width it has seen makes no upcall.

Every answer is the BORDER box: the recipe's padding (per side) and border around the content, with fixed and minimum sizes winning, plus the first baseline. The rules follow the gpui measurer and round 2 §7:
- an empty text is 0 lines; a `lines: 1` text is `nowrap`: min-content = the whole line (gpui's rule; the Swift measurer still shrinks it to 0, a drift for the iOS lane);
- a text part carries its owner's chrome: a tab's icon and count, an accordion's count and chevron, a Select option's check, a menu item's icon, a Table header's sort arrow;
- typed Table cells (`boolean` a tick, `badge` a pill) and CodeBlock lines (one line, never wrapped);
- picker triggers (Select, DatePicker, TimePicker, DateRangePicker) size their text plus the glyph, no 160 floor;
- text fields are 160 wide at max-content (`catalog/layout.json`); inline fields (NumberField / ChipInput `input`, Select `search`) and autosize Textareas size their LIVE text;
- Image / Video take `aspectRatio` (16:9), 320 at max-content, 0 at min-content; a Chart's `height` is the whole box; an AudioPlayer is the title line + `$control.row`;
- the leaf's `FfiTextStyle`: size, weight, line height, family, `letterSpacing`, `textTransform` and italics, inherited like CSS (the leaves paint with the same style);
- a number or boolean in a text prop shows as its display string (`412`);
- Markdown measures with the same block layout it paints; platform controls have recipe-fixed boxes.
- Markdown link and image destinations follow CommonMark (balanced parentheses, `\` escapes, no whitespace). A list item whose marker sits ≥ 2 columns right of the previous level's nests one `listIndent` deeper. A paragraph that is one `![alt](src)` is a block image `imageHeight` tall (loading or loaded, so measure = paint; the alt text in the box when it fails); a src the media policy DENIES is a paragraph of its alt text (none without one), in the measurer and the painter alike; an image inside running text stays its alt text.

Nothing is measured through Compose intrinsics, so a pass runs headless (JVM tests).

## Controls

- **Native or drawn.** Controls are platform-native where users expect it, and drawn when the theme says so (`native: false` on the part's recipe; the built-in themes set it on the Switch track).
- **Switch.** An M3 `Switch` scaled into the track frame, or the primitives' `DrawnSwitch`.
- **Slider.** An M3 `Slider`, or the drawn range + thumb with a drag that snaps to `step`.
- **DatePicker.** An M3 `DatePickerDialog`, date only, with `min` / `max` as `SelectableDates` (native overlays).
- **Select.** An M3 `DropdownMenu` on the trigger, with a filter field when `searchable`; it stays open per pick when `multiple` (native overlays, unless the recipe paints the popup).
- **TimePicker, DateRangePicker** (and the two above under painted overlays): the core's popup layer.
- **Resizable.** A handle drags from the sizes at the drag START (`drag {phase, delta}`, the core's `resizePanels`), on Compose's minimum touch target around the hairline; arrows, Home, End and Enter go to the core's `keyboardResize`; TalkBack adjusts it.
- **Chart.** Bar, stackedBar, line, area, pie, donut and sparkline from the core's numbers: the nice ticks and their formatted labels, the palette, the donut hole, the legend. Value labels, the tooltip and the summary format through the surface formatter. A tap or the pointer shows the tooltip; focused, ArrowLeft / ArrowRight move it, Home / End jump, Escape hides it. In RTL the cartesian kinds mirror (pie and donut do not). A missing point is skipped and reads `—` in the tooltip.
- **CodeBlock.** The core tokenizer's tokens coloured by `CodeBlock/token {kind}`; copy goes through `HostPlugin.copy` and the core announces `copied`.
- **Segmented.** One leaf painted from the `Segmented/item` recipe (`segmented`, `toggles`, `outline`); a tap selects through the core (single, or the toggled set when `multiple`). `bar` (the old TabBar) fills the row, each item a column of the `icon` part over a caption `label`, every item a tab with the current page selected.
- **Menu.** The core's layer, as gpui and SwiftUI paint it: action rows, checkbox rows with the `Menu.check` glyph (a checkbox to TalkBack), separators, group labels, and a submenu as its own layer beside its row (one level; an outside tap on it closes only the submenu). `openOn: press` opens from its ONE child (or the default outline Button); `contextmenu` from a long press on the child.
- **Tree guides.** A `Row` with `depth` reserves `depth × 14` dp and paints the guides the core computed: the 1 dp line of column i at x = i·14 + 7, a 3 dp rounded elbow into a stub to the column's edge, verticals overshooting the row's top by 1 dp so they cross a Section divider; mirrored in RTL.
- **Text fields.** Input, Textarea, Composer and the inline fields (NumberField / ChipInput `input`, Select `search`) are host-owned `BasicTextField`s (`OwnedTextField`):
  - the view owns its `TextFieldValue`;
  - every edit carries a revision;
  - a model write (an echo, the composer clearing) reloads the view only when `FieldState.writeGeneration` moves.
- **Interaction states.** Press, focus and hover reach the core as the node's states (`setStates`), so recipes' `hover` / `pressed` and `:hover` styles restyle it. A mouse hovers pressables, the nodes the core marks `hoverStyled` (a `:hover` style block or a hover recipe rule), triggers, text fields and nodes with a transition (gpui's set).
- **Unbound values.** The core keeps every unbound control's value (round 1) and restyles its parts; the painter's mirror only bridges the press until the next pass.

## Overlays

`SurfaceOptions.overlays` chooses how layers present.

**`Native` (the default):**
- **Dialog:** a Compose `Dialog`, back and outside-tap only when `dismissible`.
- **Drawer:** an M3 `ModalBottomSheet`, which refuses Hidden when not dismissible. A second modal stacks.
- **Popover**, **Menu** (and an open submenu) and the core's picker popups: a focusable `Popup` with the core's layer on the side the core placed it. It flips and clamps into the window.
- **Tooltip:** painted in the surface at the core's frame after a long press.
- **Toast:** painted in the surface at the core's frame. The duration runs in the model; a press or the pointer pauses it, and on release the REST of it runs. Escape never closes one.
- **Menu `openOn: contextmenu`:** a long press on its target (Shift+F10 from the keyboard).

Viewport-placed painted layers (a centred dialog, an edge sheet, the toasts) follow the part of the surface the host shows.

**`Painted`:** draws every layer inside the surface at the core's frames, with a scrim. Use it for snapshots and for hosts that own their windows.

## Windowed lists

The core windows a `List` or `Table` past 50 rows (`windowThreshold`), on one axis (horizontal lists too).
- The rows sit at their content offsets inside a scroller whose content is as large as the core says. Item keys are the core's (`#<index>` rules, accumulated suffixes).
- The offset goes back through `scrollTo(list, x, y)`, which moves the window in constant work.
- A list without a bounded size windows against the host's scroller (the surface reports what is visible).
- Sections (`sectionBy`) and `stickyHeaders` come from the core; the pinned header paints with the sticky offsets.
- `scrollToIndex` renders the item first, then scrolls (a page-scrolled list asks the host scroller through `bringIntoView`).
- No `LazyColumn`, no guessed row heights.

## Keyboard

Hardware keys follow `catalog/a11y.json`:

| keys | on |
|---|---|
| Tab / Shift+Tab | every control, in pre-order (handler-less buttons too); focus from the keyboard is `:focus-visible` |
| Enter, Space | press the focused control |
| arrows, Home, End | rove Tabs (activating), Radio, Segmented, Accordion headers, menu items and options (with type-ahead); step a Slider, a Carousel, a Resizable handle; ±1 day / week in a calendar (PageUp / PageDown = month, Shift = year) |
| ArrowDown | open a picker or menu trigger |
| ArrowUp / ArrowDown in a NumberField | step it |
| Backspace in an empty ChipInput | remove the last chip |
| Shift+F10 | open the context Menu around the focused node |
| Escape | close the top layer (an AlertDialog presses its cancel), else a tooltip |

A focused scroll container scrolls with the arrows, Page keys, Home and End. Tab leaves a multi-line field.

## Accessibility

TalkBack reads pre-order. This is the VAPP-4 fix, which needs both halves:
- **Every node** carries `semantics { traversalIndex = index; isTraversalGroup = true }`.
- **The surface** is a traversal group.

Node kinds:
- **Pressable containers** become ONE button with a click action, named by `accessibility.label` else their leaves (`combinedLabel`, the children cleared).
- **The node's `accessibility`** (macro `$a11y`, the author's, the core's defaults) adds: heading, button / checkbox / switch / radio / image roles, pressed / checked / selected / current, expand / collapse actions, progress values (progressbar, meter, slider, separator), pane titles of dialogs, `polite` / `assertive` live regions (Text `live`, Toast, `status` / `alert`), set positions of windowed items. `hidden` (and `visibility: hidden`) leaves the tree.
- **Leaves** carry their role, label and state: Tab `selected`, toggleable state, slider range + `setProgress`, a Resizable handle (named `$string.resize`, adjustable), image (a Chart named by `chartSummary` / `sparklineSummary`), button; a field carries its failing checks as its error, one per line.
- **Announcements** (`announce`, Form `invalidFields`, CodeBlock `copied`) go through the view's `announceForAccessibility`; `assertive` first interrupts what TalkBack is saying; `focus` requests reach the node's `FocusRequester`.
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

## The host API

`at.exponential.ui.host` is the Kotlin twin of the TS reference runtime (`packages/exponential-ui/src/host/runtime.ts`, contract `catalog/host.json`). The rules (router, decoders, policy, sources, packages) are the Rust core's, reached through the facade; nothing is re-implemented here.

```kotlin
val host = remember {
    ExponentialHost(HostOptions(
        transport = JsonlStreamTransport("https://example.com/a2ui.jsonl", postUrl = "https://example.com/action"),
        extensions = listOf(HostExtension(extensionJson, mapOf("TrendLine" to TrendLinePainter))),
        functions = mapOf("app.toast" to { args, call -> toast(args["text"]?.string) }),
        sources = mapOf("exp" to SourceResolver { source, emit -> subscribe(source, emit) /* returns the cancel */ }),
        policy = HostPolicy(functions = FunctionPolicy(ask = listOf("app.*")), onFunctionCall = { consent(it) },
            urls = UrlPolicy(hosts = listOf("*.example.com")), media = MediaOptions(baseUrl, listOf(MediaRule(prefix, authHeaders)))),
        theme = ThemeHandle.load(themeJson), mode = Mode.Light, plugin = myIcons,
    ))
}
DisposableEffect(host) { host.connect(); onDispose { host.dispose() } }
HostSurface(host, "greenhouse", fallback = { Text("Waiting…") })
```

- **`ExponentialHost`** owns the transport, the core's `HostRouter`, one `SurfaceModel` per surface, the source subscriptions, the function registry and the policy. Its `surfaces`, `status` (`connecting|open|closed|error`, `hasTransport`) and `unsupportedCatalog` are Compose snapshot state.
  - Transport and source callbacks hop onto its `scope` (default `Main.immediate`).
  - `receive(json)` feeds a message directly; `installPackage(json)` installs a declarative package for `applyTemplate`.
  - `supportedCatalogIds` and `clientCapabilities()` cover catalog negotiation.
- **Ops.**
  - `create` makes a new model with the host's theme, mode and extensions.
  - `components` merges by id.
  - `data` sets data; no `value` removes the path.
  - `bind` runs the scheme's resolver; with no resolver it sends `VALIDATION_FAILED`.
  - `delete` drops the model and cancels its subscriptions.
  - `send` goes out on the transport; an `UNSUPPORTED_CATALOG` also sets `unsupportedCatalog`.
- **Painter events.**
  - An `action` becomes the A2UI client message on the transport (the plugin's `onAction` still fires).
  - A `functionCall` goes through `callFunction`: `not_found` → `FUNCTION_NOT_FOUND`, `ask` → the `onFunctionCall` consent hook, `deny` → `FUNCTION_DENIED`, `allow` → the handler. A package surface's `functions` list narrows the decision.
  - EVERY href (`openUrl`, `Link`, markdown links) goes through the URL policy (relative urls resolve against the media `baseUrl`), then `policy.openUrl` (default: the system opener); a denied href paints as plain text.
  - EVERY src (images, avatars, posters, markdown images) goes through `host.mediaRequest`: `resolveUrl`, then the media policy (`schemes`, `hosts`; the defaults without `media`), so the media rules' headers reach the fetch. A plain `HostPlugin` gets the same policy from its `mediaOptions`.
  - `paintError` forwards a failed painter as an A2UI `RENDER_FAILED` error at `/components/<id>`, once per surface + component + message (cleared when the surface's components change).
- **Transports.** All of them parse through the core's decoders.
  - `MemoryTransport` (`feed`, `feedJsonl`, `sent`).
  - `JsonlStreamTransport` and `SseTransport`: a streamed `HttpURLConnection` GET on `Dispatchers.IO`, client messages POSTed to `postUrl`, reconnecting after `reconnectMs` (0 = never).
  - `McpTransport`: JSON-RPC `initialize`, then `tools/call`; client messages go out as `a2ui_event` calls.
  - `WebSocketTransport` runs over a socket the APP supplies. The AAR has no HTTP or WebSocket dependency: implement `fun interface WebSocketConnector { fun connect(url, listener): WebSocketConnection }`. The KDoc has the OkHttp version; it is ten lines.

## Publishing

Both modules publish as `at.exponential:ui-compose` and `at.exponential:ui-compose-primitives`.
- Each artifact has the AAR, a full POM (Apache-2.0; developer `Exponential` <hello@exponential.at>; scm `github.com/Niach/exponential`), and sources and javadoc jars.
- The version is the `uiVersion` Gradle property (`-PuiVersion=…`; release-ui.yml passes the tag's version, the `0.1.0` default lives in each module's `build.gradle.kts`).
- Signing is applied ONLY when `ORG_GRADLE_PROJECT_signingInMemoryKey` (+ `…Password`) is set. The root `build.gradle.kts` holds the shared POM, signing and repository setup.

```bash
bash apps/desktop/crates/exponential-ui-ffi/build-android.sh       # the .so files the AAR bundles
bash packages/exponential-ui-compose/release/central-bundle.sh -PuiVersion=0.1.0
```

`central-bundle.sh` publishes into the local `centralBundle` repository (`build/central-bundle`). It drops `maven-metadata.xml`, adds any missing `.md5`/`.sha1`, and zips the Maven layout into `build/central-bundle.zip`, the Central Portal upload format. The upload is a CI `curl`; an unsigned bundle builds but the Portal rejects it.

## Tests

- **`./gradlew :ui-compose:testDebugUnitTest`** runs on the JVM (Robolectric) against the host build of the facade. Export `CARGO_TARGET_DIR` (the tests load `$CARGO_TARGET_DIR/release/libexponential_ui_ffi.dylib` through `jna.library.path`). Each test class runs in its own JVM. The suite covers:
  - the fixture replays (the kitchen sink in every theme and mode, the catalog component cases, macros, the extension fixture);
  - the geometry of `layout-geometry.json`;
  - the measurer rules;
  - interaction (actions, mirrors, the field debounce + revisions + echo rule, a 40-key burst, layers, windowed lists);
  - `src/test/snapshots/components.json`, the painted tree of every component case under the fixed measure;
  - Roborazzi image snapshots (`src/test/snapshots/images/`);
  - `HostTest`: the host runtime (router ops, sources + cancel, the function gate + consent + package narrowing, the URL policy, media requests, the in-memory transport, actions → client messages, catalog negotiation);
  - `ConformanceTest`: every suite of `packages/exponential-ui/conformance/manifest.json` (version 2, 24 suites).
    - The painted suites run THROUGH this painter: catalog and replay composed by `ExponentialSurface`, control geometry from `SurfaceMeasurer`.
    - `format` runs through the painter's `IcuFormatter` (en-US, UTC); the animation timings and the leaves' direction come from a live surface's visuals.
    - The pure suites run through the bindings.
    - The report goes to `$EXPONENTIAL_UI_CONFORMANCE_REPORT`, default `<repo>/.conformance/exponential-ui-compose.json`. Check it with `bun run --filter @exponential-at/ui conformance:check "$PWD/.conformance/exponential-ui-compose.json"`.

  - `Round2InteractionTest`: settings + the ICU formatter, strings, copy / focus / announce / file picks, toast timers, Resizable drags and keys, scroll containers + sticky pins, `scrollToIndex`, keys, animations, direction.
  - `RealFontConformanceTest`: the real-font harness (`packages/exponential-ui/conformance`, round 2 §8). Every case of `fixtures/conformance-cases.json` laid out with ONLY the conformance fonts (Robolectric's native text, density 3) is dumped to `build/conformance/compose.json` (the `xui-frame-dump/1` format) and compared with the web baseline by the rules of `compare.ts`. The gate is a ratchet over `src/test/conformance-known.json`: a worse case fails, and so does a better one until its budget is lowered. `onlyRef` (nodes the web places that Compose does not emit) is budgeted per case; the run prints the total. `build/conformance/report.txt` lists every origin. Rewrite the budget after a fix or a baseline rewrite: `EXP_UI_WRITE_FIXTURES=1 ./gradlew :ui-compose:testDebugUnitTest --tests '*RealFontConformanceTest'`.
  - `ListBenchTest`: the shared 100,000-row bench (`fixtures/bench-list.json`).

  `EXPONENTIAL_UI_RECORD=1` rewrites the snapshots. The SwiftUI comparison checks every case both snapshots have. Cases waiting for a Swift re-record are listed in `src/test/snapshots/swift-parity-pending.json`: new drift fails, and so does a listed case that matches again.

  What the conformance suites exercise:

  | suites | through |
  |---|---|
  | catalog, replay, geometry, controls, format, animation timings, direction of the leaves | this painter (`ExponentialSurface`, `SurfaceMeasurer`, `IcuFormatter`, live visuals) |
  | bind, style-conditions, code-tokens, template-items, resizable, virtual-list, keyframes CSS | the bindings or a test-side port of the TS reference (the Rust core's own tests cover the same files) |

  Painter coverage for windowing, sticky pins, Resizable drags and toasts is `Round2InteractionTest`, not the suite count.
- **`./gradlew :example:connectedDebugAndroidTest`** runs on a device:
  - `AccessibilityOrderTest`: the TalkBack walk starts `Back, Inbox, Search, Boards, Sprint 12, EXP-42, Alex Chen, Reddit radar, …, Scan now, Sources`, keeps the iOS orderings, and has more than 60 labels.
  - `KeyboardTest`: Tab walks the controls in pre-order with `:focus-visible` and reaches the tabs, ArrowRight activates the next tab, the Escape KEY closes the top layer.
  - `TypingTest`: 40 characters in one `performTextInput` burst land in order in the Title field and on the `host:` line.

## Numbers

From the release build of the example (R8, non-debuggable) on a `Medium_Phone_API_34` emulator (x86_64, API 34, KVM on a 32-core Linux host, 2026-10-08). Indicative only, since this is an emulator; the earlier Mac numbers (arm64 emulator) were 4 to 8 times lower for the Kotlin measurer. `layout` = the core's pass including the JNA upcalls and the Kotlin measurer.

| pass | bench 200 (205 nodes) | kitchen sink (500 nodes) |
|---|---|---|
| cold, the first pass of a process (6 launches) | 52 to 78 ms layout, 70 to 99 ms wall; 2 upcalls, 379 answers | 105 to 181 ms layout, 144 to 215 ms wall; 2 upcalls, 628 answers |
| warm full pass, width flipping 390 / 411 dp | **0.17 ms** best, 0.18 median; 0 upcalls, memo answers | 1.96 ms best, 2.12 median |
| the same under the core's fixed measure | 0.2 ms best | 0.09 to 2.5 ms |
| full re-measure (every leaf crosses) | 16.8 ms best, 18.3 median | 38 ms best, 47 median |
| of which Kotlin (the measurer) / the two JNA crossings | ~14.7 ms / ~1.8 ms | ~29 ms / ~4.4 ms |
| cached pass (nothing dirty) | 0.02 ms layout, 0.54 ms best wall | 0.06 ms layout, 0.72 ms best wall |

**Reading:**
- The warm target, under 2 ms for 200 nodes including JNA, is met: 0.17 ms.
- A full re-measure is the Kotlin text measuring (a Compose `TextMeasurer` call per question); the JNA crossings stay a batch each.

**The list bench** (`fixtures/bench-list.json`, `ListBenchTest`: the model with Robolectric's native text at density 2.75 on the JVM, the same Linux host): `firstPaintMs` 139, `scrollStepMs` 14.1 (the core's own step is 3.2 ms; the rest is the newly windowed rows' measuring and the node / visual read-back), `scrollToIndexMs` 17.0, `renderedItems` 30.

The TalkBack walk (real TalkBack, swipes on the emulator's virtual touchscreen) reads the kitchen sink in pre-order from the avatar to the `host:` line. Inside the open Dialog it stays in the dialog: title, description, body, `Delete for good`, Close, then it wraps.

## Known divergences from the React renderer

- Line breaks follow Android's text layout; the `lines` clamp ellipsizes.
- Images load through a small `BitmapFactory` loader, only for a POLICED request: http(s) (redirects re-policed per hop) and `data:` URIs, never a local file or content URI, within `media.limits` (bytes up front and streamed, the request timeout, width × height from the header before decoding), downsampled to 2048 px. SVG is not supported, and the host app needs the INTERNET permission. Video and AudioPlayer are static placeholders (their poster loads through the media policy).
- Compose has no error boundary: a throw inside a composable cannot be caught. An extension painter's `measure` throw is caught; its `Paint` reports props it cannot paint through `context.fail(message)`; built-in painters keep failure-prone preparation in `rememberPainted`. Each failure paints an empty box and calls `onPaintError`.
- Native M3 controls (Switch, Slider, the Select menu, the DatePickerDialog) take M3 defaults apart from the primary tint; the DatePickerDialog takes the host's `MaterialTheme`. The M3 Slider keeps a 16 dp thumb (not M3's 44 dp bar) and a 48 dp touch height, both overflowing the 6 dp track frame vertically.
- A pressable container clears its children's semantics (`clearAndSetSemantics`, the SwiftUI `.ignore`), so a nested pressable inside a pressable row is not separately reachable.
- Markdown paints with the primitives' `MarkdownView`. It rounds the wrap width while the text measurer ceils it, so a paragraph exactly on a wrap boundary can paint one line off (the leaf clips to its frame).
- The Ring's value label is wider than the 32 dp ring and clips at a narrow card edge (the shared measurer rule; iOS does the same).
- TalkBack reads a labelled field's label twice (the `Label` node, then the field's own description), as it reads the slider's value twice.
- Painted mode has no back handling (no `activity-compose` dependency). Native mode gets it from the Dialog and the sheet.
- `backdropBlur` paints the translucent background alone: a Compose `RenderEffect` blurs a node's OWN content, and sampling what is behind it needs a copy of the surface drawn under every blurred node. The authoring rule (pair it with a translucent `backgroundColor`) keeps the result legible.
- TalkBack has no heading levels and no role descriptions in Compose: a heading is a heading, a Carousel page a group.
- A FileUpload takes files only through `HostPlugin.pickFiles` (no drag and drop).
- The real-font conformance dump runs on Robolectric's text stack, not a device's (`src/test/conformance-known.json` counts every divergence; most origins are shared with gpui and decided in `fixtures/conformance-known.json`).
