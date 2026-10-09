# `ExponentialUI`: the SwiftUI painter

The iOS + macOS renderer of the Exponential UI SDK (VAPP-88). The Rust core
[`exponential-ui`](../../apps/desktop/crates/exponential-ui) reduces an
A2UI surface, resolves the theme and lays the tree out with taffy; this
package measures text for it in batches and paints the result in SwiftUI
at the frames the core computed. iOS 17 and macOS 14. No HTML, no web
view, no downloaded code, nothing from the Exponential app.

| product | what | depends on |
|---|---|---|
| `ExponentialUI` | the painter: `SurfaceModel` + `ExponentialSurface`, the host plugin, extension painters | `ExponentialUIFFI.xcframework` (the UniFFI facade [`exponential-ui-ffi`](../../apps/desktop/crates/exponential-ui-ffi): iOS device + simulator + macOS slices), `ExponentialUIPrimitives` |
| `ExponentialUIPrimitives` | the generic SwiftUI primitives the catalog natives are painted with (pill, segmented control, field chrome, drawn switch, avatar, meter track, ring, markdown model, empty state, disclosure header), themed by `PrimitiveTokens` | nothing |

The Exponential app's `ExpUI` builds its specialised views on
`ExponentialUIPrimitives` (SLOP-18 convergence) without linking the core.

## Build the binary

```bash
bash apps/desktop/crates/exponential-ui-ffi/build-ios.sh   # → Binaries/ExponentialUIFFI.xcframework (gitignored)
swift test                                                 # macOS: the painter, host + conformance tests + the primitives
```

The xcframework holds iOS device (arm64), iOS Simulator (arm64 + x86_64) and
macOS (arm64 + x86_64) slices, all built with the `mobile` profile (LTO, one
codegen unit) and without the facade's `cli` feature; ~200 MB unzipped.

`Package.swift` declares the binary target, the painter and its tests only
when the xcframework is present in `Binaries/` (or `EXPONENTIAL_UI_FFI` names
one elsewhere: an absolute path or one relative to this directory; the
manifest rewrites an absolute one relative, the form SwiftPM accepts); a
checkout that only needs the primitives (the app's Tuist graph, CI) resolves
without a Rust toolchain. Publishing (VAPP-91, below) switches the binary
target to `url:` + `checksum`. The generated binding
`Sources/ExponentialUICore/ExponentialUIFFI.swift` is a committed copy of
`exponential-ui-ffi/bindings/swift` (the script writes both).

## Embedding

```swift
import ExponentialUI

let model = try SurfaceModel(id: "s1", options: SurfaceOptions(theme: ThemeHandle.builtin("exponential"), mode: .dark), host: MyHost())
try model.apply(json: a2uiMessage)                 // or setNested(json:) / setComponents(json:) / setData(path:value:)
ScrollView { ExponentialSurface(model: model) }    // as wide as its container, as tall as the content
```

- **Host.** `HostPlugin` (every method has a default): `icon(name, size)`
  = the catalog's registry concepts → your views (nil = the placeholder
  circle); `onAction` = every A2UI action; `onInput` = host-owned text
  edits (`change` debounced 150 ms with a revision, `commit` on blur /
  Enter); `openUrl`; `resolveUrl`; `onUnknown`; `fontFamily`; `markdown`;
  `onUpload` = the files a FileUpload got (name, size, MIME type, URL,
  bytes; the surface already fired `upload`); `pickFiles` = `true` when
  the host shows its own picker (then `model.filesPicked(componentId:urls:)`),
  else the surface presents `.fileImporter`; `announce` = what the model
  also posted to VoiceOver; `copy` = `true` when the host wrote the
  clipboard (a CodeBlock copy). `ClosureHost` builds one from closures.
- **Settings** (contract §4–6). `SurfaceOptions.settings` /
  `model.setSettings(SurfaceSettings(...))` or one setter each
  (`setLocale`, `setStrings`, `setModeSetting`, `setDensity`,
  `setContrast`, `setFontScale`, `setInsets`, `setHoverCapable`,
  `setReducedMotion`, `setToday`). nil / `system` parts follow the
  platform live: `ExponentialSurface` feeds the colour scheme,
  `colorSchemeContrast`, Dynamic Type, Reduce Motion, the safe area and an
  iPad pointer into the model (`setPlatform` without the view). Density
  and contrast paint with the core's effective theme; week start, text
  direction and the built-in strings are the core's. Formatting (round 2
  §3) = ONE `FoundationFormatter` per surface (the core's foreign
  `HostFormatter`: `NumberFormatter`, `DateFormatter`,
  `RelativeDateTimeFormatter`) in the surface locale and
  `SurfaceSettings.timeZone` (nil = the device's): bound `formatNumber` /
  `formatDate` / … calls, Table cells, NumberField and picker text.
  `setClock(nowMs:)` pins relative times; the view ticks once a minute.
- **Commands.** `model.command(.focus(id:))`, `.announce(text:live:)`,
  `.scrollIntoView(id:)`, `.scrollToIndex(id:index:align:)` (round 2: the
  DATA index of a List / Table); `submitForm(id:)`, `failingChecks(_:)`,
  `setOpen(_:_:)`, `dismissToast(_:)`, `escape()`.
- **Round 2 painting.** Text styles carry `letterSpacing`, `textTransform`
  and italic (measured and painted); hover tracks every node the core marks
  `hoverStyled`; `position: sticky` and pinned section headers paint at the
  core's offsets (`model.sticky`, against the host scroll view's offset,
  `setSurfaceScroll`); Resizable handles drag from the start sizes and take
  the core's keys (VoiceOver: adjustable); `backdropBlur` = a material by
  radius; keyframe `animation`s sample the core's frames in a
  `TimelineView` (Reduce Motion = the rest frame); windowed lists scroll on
  either axis.
- **Fonts.** `ExponentialUI.registerFont(at:)` registers a file a theme
  names (`Inter`); a missing family falls back to the system font.
- **Themes.** `ThemeHandle.builtin(id)` / `ThemeHandle.load(json:)`;
  `model.setTheme` / `setMode`. Painters never read recipes: the core hands
  resolved visuals, and sub-parts it does not synthesize (a Checkbox
  `check`, a Switch / Slider `thumb`, a Segmented `item`…) resolve through the
  facade's `Theme` object (`model.part(component, part, props:)`), cached
  per query.
- **Extensions.** `ExponentialUI.register(extension: json, painters: [kind:
  painter])`; an `ExtensionPainter` answers `measure(leaf, wrap:)` (the
  border box) and `paint(context)` (the view inside the frame; `emit` fires
  the node's `on` handlers).
- **Viewport.** The width comes from the view's geometry;
  `model.setViewport(width:height:maxHeight:)` sets the visible height
  (dialog centring, windowed lists) and a card bound.
- **Keyboard.** The surface root takes keyboard focus and hands every key
  to `model.handleKey` (`catalog/a11y.json`: Tab order = paint order with
  roving stops and the top layer's focus trap, arrows in roving widgets,
  menus, listbox type-ahead, the calendar grid, sliders, charts, scroll
  containers, Escape, Shift+F10). Horizontal arrows mirror in rtl. Focus is
  the model's (`focusedId`); `focusRequest` moves the platform's (a text
  field's first responder, `@AccessibilityFocusState`); the ring paints
  for `focus-visible` (keyboard focus) only.

## The host API (VAPP-91)

`catalog/host.json` on Swift. `ExponentialHost` (`@Observable`, main actor)
owns the transport, the core's `HostRouter` (messages in, ops out), one
`SurfaceModel` per surface, the source subscriptions, the functions and the
policy; `HostSurface(host:surfaceId:)` paints one. The rules (routing,
decoders, the function / URL gates, media requests, source parsing,
packages) are the Rust core's, reached through the bindings.

```swift
let host = ExponentialHost(HostOptions(
    transport: JSONLStreamTransport(url: stream, postUrl: actionURL),
    functions: ["harness.toast": { args, call in /* … */ nil }],
    sources: ["exp": { source, emit in /* subscribe */ return { /* cancel */ } }],
    extensions: [HostExtension(json: catalogJSON, painters: ["TrendLine": TrendLinePainter()])],
    packages: [packageJSON],
    policy: HostPolicy(functions: FunctionPolicy(ask: ["app.*"]), onFunctionCall: { call in await askUser(call) },
                       urls: UrlPolicy(hosts: ["*.example.com"]), media: MediaOptions(baseUrl: "https://app.example.com", rules: [.init(prefix: "https://app.example.com/api/", headers: ["authorization": "Bearer …"])])),
    theme: try ThemeHandle.load(json: themeJSON), mode: .light, plugin: myIcons))
host.connect()
ScrollView { HostSurface(host: host, surfaceId: "main") { ProgressView() } }
```

- **Ops.** `create` = a new empty model (the host's theme, mode,
  extensions), `components`, `data` (no `value` = remove, `""` = the whole
  model), `bind` (the scheme's `SourceResolver`; none = a
  `VALIDATION_FAILED` back), `delete` (+ its subscriptions), `send`
  (`UNSUPPORTED_CATALOG` also sets `unsupportedCatalog`). `receive(json)`
  feeds a message by hand; `status` / `statusDetail` / `hasTransport`.
- **Out.** An `action` becomes the A2UI client message on the transport
  (the plugin's `onAction` still runs); a `functionCall` goes through
  `callFunction`: `openUrl` → `openURL` (the URL policy, relative urls
  against `media.baseUrl`), else `decide` (+ a template surface's package
  `functions`) → `not_found` / `deny` send the error, `ask` asks
  `onFunctionCall`, `allow` runs the function. Images, avatars and video
  posters load through `mediaRequest` (a `URLRequest` with the rules'
  headers), never `AsyncImage`.
- **Transports.** `MemoryTransport` (`feed`, `feedJsonl`, `sent`),
  `JSONLStreamTransport` / `SSETransport` (URLSession `bytes(for:)` through
  the core's `JsonlDecoder` / `SseDecoder`, client messages POSTed to
  `postUrl`, reconnect), `WebSocketTransport` (one message or JSONL per
  frame), `MCPTransport` (`initialize`, `tools/call <tool>`, actions back as
  `a2ui_event` calls). A custom one implements `Transport`.
- **Negotiation.** `supportedCatalogIds`, `clientCapabilities`.

`samples/exponential-ui/ios` is a blank app on this API. Conformance:
`ConformanceTests` replays every suite of
`packages/exponential-ui/conformance/manifest.json` through the painter and
writes `.conformance/exponential-ui-swift.json` (`bun run --filter
@exponential-at/ui conformance:check <report>`).

## Publishing (SwiftPM)

`release/zip-xcframework.sh <outdir>` zips the xcframework and prints the
checksum; `release/make-release-package.sh <version> <zip-url> <checksum>
<outdir>` writes the tagged distribution repo (`Package.swift` with the
`url:` binary target, the products `ExponentialUI` +
`ExponentialUIPrimitives`, `Sources/`, LICENSE, NOTICE, README). `--local
<xcframework>` swaps in `path:` for a local `swift build` (SwiftPM refuses
file URLs for binary targets).

## The painting model

- Nodes are STABLE SLOTS patched from each pass's delta (a removed node
  is a tombstone until the core compacts); `model.order` is paint order =
  accessibility order. The core returns UNSCROLLED frames in surface
  coordinates; `FrameLayout` (a custom `Layout`) places every child at its
  frame relative to its container, so clips and opacity inherit. The
  surface pins `layoutDirection` to left-to-right: an rtl surface comes
  mirrored from the core; text direction and the `rtlMirroredIcons` glyphs
  (`scaleX(-1)` outermost) read `model.isRTL`.
- A box paints every round-1 key: per-side borders (dashed / dotted),
  per-corner radii, multi-stop gradients, shadows, per-axis overflow, the
  paint-only `transform`, `visibility`, `pointerEvents`, `cursor` (macOS),
  inherited text keys (tracking, decoration, transform, italic). Motion:
  only a node's own box animates, with its `transition` and easing; none
  under reduced motion. Hover, press and focus go to the core as states
  (`hover`, `pressed`, `focus`, `focus-visible`, `dragover`), which
  restyles that node.
- Text is shaped by ONE engine (`TextShaper`, CSS font matching in
  `TextFonts`) for the measurer AND the painter: a text leaf draws the
  shaper's own line breaks, so painted wraps equal measured wraps; line
  heights follow CSS (`n` lines = `n × lineHeight`).
- Text colour: the node's own, else the nearest ancestor's, else the
  theme's `foreground`.

## Measurement (the VAPP-4 verdict)

`SurfaceMeasurer` implements the facade's `Measurer`: at most three
crossings per pass (`measureIntrinsics` = min-content, max-content and the
height at max-content for every leaf in ONE call; `measureHeights` for the
leaves whose width was narrower). Every answer is the BORDER box: the
recipe's padding and border around the content, fixed and minimum sizes
winning. Rules mirror the gpui painter (`measure.rs`) where it matches the
web: a `lines: 1` text shrinks to 0 at min-content; picker `trigger`s are
their text + glyph; Markdown measures with the same block layout it
paints. Nothing is measured through SwiftUI's `sizeThatFits`, so a pass
runs headless (tests on macOS).

## Controls

The round-1 natives are the core's PARTS (a picker's `trigger`, a
calendar's `day`, a NumberField's `input`, a Table's `cell`, a CodeBlock's
`code` line, a menu's `itemLabel`): the core builds, lays out and keeps
their values; the painter draws each part and routes the platform input
back. Every host text field (Input / Textarea `.field`, the Composer, the
NumberField / ChipInput `input`, a searchable Select's `search`) is ONE
UIKit/AppKit-owned view (`OwnedTextField`): the view owns the string,
every edit carries a revision, an echo is written in only while the field
is idle and unfocused; Up / Down step a NumberField, Backspace in an empty
ChipInput removes the last chip. Switch and Slider are platform controls
unless the recipe sets `native: false` (the drawn ones are rtl-aware).
FileUpload takes `.fileImporter` picks and drops (`dragover` meanwhile);
a context Menu (`openOn: contextmenu`) opens at a secondary click (macOS)
or a long press; a `bar` Segmented lays each item out as a column (icon
over a caption label) across the full width. Tree guides (a Row's
`guides` part, filled by the core) draw at 14 px columns with a 3 px
rounded elbow and a 1 px bridge above the row.

## Overlays

Overlays are CORE LAYERS (contract §5), painted inside the surface at the
core's frames: base < overlay < toast. A modal layer gets the recipe's
scrim and traps VoiceOver; a press on the scrim, outside a menu / popover
or a drawer drag dismisses only a `dismissible` layer; Escape closes the
top one; focus moves into an opening layer (its `autoFocus` node) and back
to the trigger. Hover-opened cards stay open while hovered; toasts pause
their timer while hovered or focused.

## Scrolling and windowed lists

A scroll container (`overflowX/Y: scroll | auto`, the core's
`FfiScroll`) or a windowed List paints in a native `ScrollView` as large
as the content the core reports; the view reports its offset
(`scrollReported`), and an offset the core moves (`scrollIntoView`, the
keyboard, a clamp) scrolls the view there. A windowed list moves its
window in constant work: no `LazyVStack`, no guessed row heights.

## Accessibility

VoiceOver reads in paint order (`model.accessibilityPriority`). Roles come
from `accessibility.role` / `catalog/a11y.json`: containers `.contain`,
pressables combine into one element, leaves carry label, value, hint (the
linked `<id>.error`), traits and heading levels; Slider and NumberField
are adjustable; live regions and `announce` post
`AccessibilityNotification.Announcement`; a modal layer is the focus
trap. The example app's `-a11yDump` prints the UIAccessibility walk.

## Example app

`Example/KitchenSink.xcodeproj` is a blank iOS app that adds this package
by local path (`..`) and renders `packages/exponential-ui/fixtures/
kitchen-sink.json`: a theme picker (the built-ins + `brand`, the
third-party test theme of `theme-extends.json`), light / dark, a host that
echoes every input after 150 ms. Launch arguments: `-theme <id>`,
`-mode light|dark`, `-rtl`, `-width N`, `-shot <view>`
(no chrome; what `bun run shots --platform ios --views
exponential-ui-kitchen-sink` captures), `-a11yDump`. Icons are SF Symbols
mapped from the kitchen sink's concepts; a real host hands the renderer its
own registry.

**The list bench** (`fixtures/bench-list.json`, `NativeHardeningTests.testListBenchScrollStep`:
100,000 rows, 390 × 800, the real TextKit measure on the main actor, Apple
M-series, `swift test -c release`): `firstPaintMs` 167, `scrollStepMs` 8.3
(the core's own share 2.3 ms; 78 text measurements per one-viewport step, all
rows new to the window, so a per-row cache would not hit; text widths are
already cached per string), `scrollToIndexMs` 12.

## Tests (`swift test`, macOS; `xcodebuild test`, iOS Simulator)

On iOS: `xcodebuild test -scheme ExponentialUI-Package -destination
'platform=iOS Simulator,name=<iPhone>'` (the `#if canImport(UIKit)` paths;
CI's `swift-ios` job, which also builds the KitchenSink app).

`FixtureReplayTests` (the kitchen sink in every theme and mode, all
catalog-component cases, macros, the basic map, the extension fixture
through a probe painter), `GeometryTests` (the EXACT frames of
`layout-geometry.json` at 900/390 LTR/RTL, overlay placement, open layers),
`SnapshotTests` (`__Snapshots__/components.json`: the painted tree of every
component case under the core's fixed measure; `EXPONENTIAL_UI_RECORD=1`
rewrites it), `InteractionTests` (actions, the `:pressed` relayout, tabs /
accordion through the core, the field debounce + revisions + commit + echo
rule, a 40-key burst, layers, the accessibility order, a windowed list),
`Round1ModelTests` / `Round1PaintTests` / `Round1NativesTests` (settings,
OutEvents, commands, keyboard, toasts, style keys, motion, rtl, layers,
every specimen, the natives), `ConformanceTests` (every suite of the
conformance manifest), `RealFontConformanceTests` (the CoreText frame dump
against the web baseline, ratcheted: [`conformance/`](conformance/README.md)).

## Known divergences from the React renderer

- Video and AudioPlayer are static placeholders (poster, duration).
- `ImageRenderer` renders the text leaves blank (they are UIKit/AppKit
  views); capture a real window instead.
- The real-font layout gaps left are core-side (`conformance/README.md`).
