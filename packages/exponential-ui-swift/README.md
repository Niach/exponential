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

`Package.swift` declares the binary target, the painter and its tests only
when the xcframework is present (or `EXPONENTIAL_UI_FFI` names one); a
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
  Enter); `openUrl`; `resolveUrl`; `onUnknown`; `fontFamily`; `markdown`.
  `ClosureHost` builds one from closures.
- **Fonts.** `ExponentialUI.registerFont(at:)` registers a file a theme
  names (`Inter`); a missing family falls back to the system font.
- **Themes.** `ThemeHandle.builtin(id)` / `ThemeHandle.load(json:)`;
  `model.setTheme` / `setMode`. Painters never read recipes: the core hands
  resolved visuals, and sub-parts it does not synthesize (a Checkbox
  `check`, a Switch `thumb`, a Select `trigger`…) resolve through the
  facade's `Theme` object (`model.part(component, part, props:)`), cached
  per query.
- **Extensions.** `ExponentialUI.register(extension: json, painters: [kind:
  painter])`; an `ExtensionPainter` answers `measure(leaf, wrap:)` (the
  border box) and `paint(context)` (the view inside the frame; `emit` fires
  the node's `on` handlers).
- **Viewport.** The width comes from the view's geometry;
  `model.setViewport(width:height:maxHeight:)` sets the visible height
  (dialog centring, windowed lists) and a card bound.

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
    extensions: [HostExtension(json: catalogJSON, painters: ["Sparkline": SparklinePainter()])],
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

- The core returns absolute frames in surface coordinates; pre-order is
  paint order and accessibility order. `FrameLayout` (a custom `Layout`)
  places every child at its frame relative to its container; the
  containers nest like the tree, so `overflow: hidden`, radius clips and
  opacity inherit without emulation. The surface root pins
  `layoutDirection` to left-to-right: an RTL surface, already resolved by
  the core, is not mirrored a second time.
- A container paints its box (background, radius, shadow, opacity, clip);
  its border is an overlay, so children are never offset. A measured leaf
  paints inside the recipe padding the measurer counted.
- Text is shaped and painted by ONE TextKit engine (`TextShaper` + a
  `UILabel` / `NSTextField` label): the measured height is the painted
  height, line heights follow CSS (`n` lines = `n × lineHeight`).
- Text colour: the node's own, else the nearest ancestor's, else the
  theme's `foreground`.

## Measurement (the VAPP-4 verdict)

`SurfaceMeasurer` implements the facade's `Measurer`: at most three
crossings per pass (`measureIntrinsics` = min-content, max-content and the
height at max-content for every leaf in ONE call; `measureHeights` for the
leaves whose width was narrower). Every answer is the BORDER box: the
recipe's padding and border around the content, fixed and minimum sizes
winning. Rules mirror the gpui painter: a `lines: 1` text shrinks to 0 at
min-content; Select / DatePicker fields size from the `trigger` recipe;
Markdown measures with the same block layout it paints; controls that are
platform views have recipe-fixed boxes. Nothing is measured through
SwiftUI's `sizeThatFits`, so a pass runs headless (tests on macOS).

## Controls

Platform-native where users expect it, drawn when the theme says so
(`native: false` on the part's recipe, which the built-in themes set on the
Switch track): Switch = a `Toggle` scaled into the track frame or the drawn
thumb; Slider = a `Slider` or the drawn range + thumb with a drag that
snaps to `step`; DatePicker = a graphical `DatePicker` in a popover
anchored at the field (date only); Select and DropdownMenu = a native
`Menu` on the trigger (a Select's `searchable` has no native equivalent:
the menu lists the options). Input / Textarea / Composer are
UIKit/AppKit-owned text views (`OwnedTextField`): the view owns the string,
every edit carries a revision, an echo (a changed `value` prop) is written
in only while the field is idle and unfocused. Unbound Checkbox / Switch /
Radio / Toggle / ToggleGroup / Select / DatePicker / Slider values live in a
local mirror the painter re-resolves part visuals from until the prop
changes.

## Overlays

`SurfaceOptions.overlays`: `.native` (default) presents a Dialog / Drawer
as a sheet (a detent as tall as the layer, swipe-to-dismiss unless
`dismissible: false`, a second modal stacks), a Popover as a `.popover`
anchored at the core's anchor frame (`presentationCompactAdaptation` keeps
it a popover on iPhone), a DropdownMenu as a `Menu`; a Tooltip is painted
in the surface at the core's frame (a long press on touch, hover with a
300 ms delay on macOS). `.painted` draws every layer inside the surface at
the core's frames with a scrim (snapshots, hosts that own their windows).
Escape (macOS) and the sheet's dismissal return focus to the trigger
through the core's `set_open`.

## Windowed lists

A `List` past 24 rows is windowed by the core: the rows sit at content
offsets inside a scroll view whose content is as tall as the core says; the
visible offset goes back through `scroll(list, offset)`, which moves the
window in constant work. No `LazyVStack`, no guessed row heights. A
scroll never bumps the structure version, so the model re-reads `nodes()`
after one (the core trap VAPP-90 found).

## Accessibility

VoiceOver reads pre-order: every node carries
`accessibilitySortPriority(count − index)`; containers `.contain`,
pressable containers `.combine` into one button, leaves carry their label
and trait (tab, toggle, slider with an adjustable action, link, image,
header). A sheet is modal to VoiceOver (the focus trap). The example app's
`-a11yDump` prints the UIAccessibility walk VoiceOver consumes.

## Example app

`Example/KitchenSink.xcodeproj` is a blank iOS app that adds this package
by local path (`..`) and renders `packages/exponential-ui/fixtures/
kitchen-sink.json`: a theme picker (the built-ins + `brand`, the
third-party test theme of `theme-extends.json`), light / dark, a host that
echoes every input after 150 ms. Launch arguments: `-theme <id>`,
`-mode light|dark`, `-rtl`, `-width N`, `-overlays painted`, `-shot <view>`
(no chrome; what `bun run shots --platform ios --views
exponential-ui-kitchen-sink` captures), `-a11yDump`. Icons are SF Symbols
mapped from the kitchen sink's concepts; a real host hands the renderer its
own registry.

## Tests (`swift test`, macOS)

`FixtureReplayTests` (the kitchen sink in every theme and mode, all
catalog-component cases, macros, the basic map, the extension fixture
through a probe painter), `GeometryTests` (the EXACT frames of
`layout-geometry.json` at 900/390 LTR/RTL, overlay placement, open layers),
`SnapshotTests` (`__Snapshots__/components.json`: the painted tree of every
component case under the core's fixed measure; `EXPONENTIAL_UI_RECORD=1`
rewrites it), `InteractionTests` (actions, the `:pressed` relayout,
mirrors, tabs / accordion through the core, the field debounce +
revisions + commit + echo rule, a 40-key burst with a 150 ms echo, layers,
links, the measurer rules, the accessibility order, a windowed list scroll).

## Known divergences from the React renderer

- Line breaks follow TextKit; the `lines` clamp truncates with an ellipsis.
- Skeleton is static. Images load through the host's media request; Video and
  AudioPlayer are static placeholders.
- A Select's `searchable` opens the plain native menu; `multiple` toggles
  per pick (the menu closes between picks).
- The Carousel `indicator` leaf is the recipe's single dot; the strip is
  centred on it. The Slider track frame is the 6 px bar; the thumb (and the
  native slider) overflows it.
- `ImageRenderer` renders the text leaves blank (they are UIKit/AppKit
  views); capture a real window instead.
