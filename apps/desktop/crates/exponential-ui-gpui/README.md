# `exponential-ui-gpui`: the desktop painter

The gpui painter of the Exponential UI SDK (VAPP-90, hardened in round 1).
The core crate [`exponential-ui`](../exponential-ui) reduces an A2UI surface,
resolves the theme and lays the tree out with taffy. This crate measures text
for it in-process and paints the result in gpui. It depends on the core,
`gpui`, `gpui-component`, `serde`, `serde_json` and `unicode-linebreak`, and on
nothing from the Exponential app (`ui`, `theme`, `domain`…). No HTML, no
webviews.

## The painting model (the VAPP-4 verdict)

- `SurfaceView` renders a `SurfaceElement`. Its LAYOUT is a gpui measured
  node: gpui hands it the available width, the core lays the surface out at
  that width and answers its height. The first frame is laid out at the
  element's real width (no probe, no 900 px frame).
- Its PREPAINT knows the bounds and the host's clip. In the same frame it
  moves the windowed lists the host scrolls and the visible region, laying
  out again only when they moved, then builds the element tree.
- The core returns ABSOLUTE, UNSCROLLED frames in surface coordinates in
  paint order (= accessibility order). Node indices are STABLE slots, not
  pre-order: the painter walks `PlacedNode::children` and patches its node
  cache from each pass's `NodeDelta`.
- ONE absolutely positioned gpui `div` per node, at its frame relative to its
  parent's (scrolled) frame. The divs nest like the tree, so clipping,
  opacity and text style inherit. Catalog styles are never mapped onto gpui's
  flex/grid layout; gpui only rasterises.
- A container paints its box: background, gradient, per-corner radii, shadow,
  opacity, clip (per axis). Its border (per-side widths, one colour, solid or
  dashed; `dotted` paints dashed) is an overlay child, so it never offsets
  the children: their frames already include it.
- A measured leaf paints its content inside the box, inset by the per-side
  padding and border the measurer counted (`PaintStyle::insets`).
- Text colour: the node's `color`, else the nearest ancestor's, else the
  theme's `foreground`. Fonts: see "Fonts" below.
- `letterSpacing`: gpui text runs have no tracking, so tracked text is
  shaped once per line (kerning kept) and its glyphs are painted one by one
  at their shaped positions plus the spacing per preceding character
  (`natives::tracked_text`); the measurer adds the same `ls × characters`,
  so the painted width IS the measured one. Underline / line-through and
  the one-line ellipsis follow. Text, Button, Link, picker triggers and
  Segmented items paint tracked; the host's editable inputs cannot, so
  inline fields MEASURE untracked too (a divergence from CSS, below).
- Hidden nodes (inactive tabs, closed panels, falsy `visible`) paint nothing;
  `visibility: hidden` keeps the box and paints nothing.
- Rounded clipping: gpui's content masks are rectangular, so a descendant
  whose corner lies inside a clipping ancestor's rounded corner rounds its
  own to match (`paint::inherit_radii`). An image flush with a card's corner
  is clipped round.
- Images: `fit` maps to gpui's `object_fit`; `focalX` / `focalY` (CSS
  `object-position`) paint the decoded image at its focal bounds
  (`natives::focal_bounds`) inside the leaf's rounded corners.
- `transform` is paint-only: the translate moves the box (and its
  subtree). A LEAF's scale grows its box about the centre. An `Icon`'s
  rotation turns the glyph (the Collapsible chevron). Containers do not
  scale or rotate (see divergences).
- `pointerEvents: none` drops the node's listeners. `cursor` maps to gpui's
  `CursorStyle`. `native: true` on a Switch track or a Checkbox box paints
  gpui-component's own control in place of the recipe chrome.

### Sub-parts painted from recipes

The core synthesizes `<id>.<part>` nodes for nearly everything (labels,
fields, triggers with their `text` and `icon`, list options with their
`check`, calendar days, menu rows, toast parts, table cells, code lines with
their tokens, steppers, chips, file rows). The few parts a leaf draws
itself come from the owner's recipe through `paint::parts::part_visual`,
which resolves them exactly like the core does:

- Checkbox `check`, Switch `thumb`, Slider `range` / `thumb`
- Button `icon`, Ring `track` / `fill`, Chart `axis` / `grid` / `legend` /
  `tooltip` / `title` / `valueLabel`
- the Tabs `indicator`, Carousel `indicator`, TreeGuides `line` (round 3:
  14 px columns, the line of column i at x = i·14 + 7, the elbow a 3 px
  ROUNDED corner drawn as one stroked path, every vertical overshooting the
  row's top by 1 px to bridge a Section divider)
- the Composer `field` / `send` / `attachment`, the `Markdown` block parts
- the CodeBlock `token` per kind (`when: {kind}`)

Glyphs: catalog `Icon` names are registry CONCEPTS resolved through
`HostPlugin::icon(name)` (an SVG asset path). Without a host registry every
name of the core's `builtinIcons` table still draws (gpui-component's
`IconName`, a drawn clock and stop square). Unknown names paint the
placeholder circle. Chevrons and back arrows mirror in RTL.

## Embedding

```rust
let view = cx.new(|cx| SurfaceView::new(SurfaceViewOptions {
    surface_id: "s1".into(),
    theme: Some(exponential_ui::themes::builtin_theme("exponential").unwrap()),
    mode: Mode::Dark,
    host: Rc::new(MyHost),                  // impl HostPlugin
    settings: Some(SurfaceSettings { locale: "ar".into(), mode: ModeSetting::System, ..Default::default() }),
    ..Default::default()
}, window, cx));
view.update(cx, |v, cx| {
    v.register_painter("TrendLine", Box::new(MyTrendLine));   // extension natives
    v.apply(&message, cx).unwrap();                           // A2UI messages
});
// render: put `view` in YOUR scroller; it is `w_full()` × the content height.
```

- **Width.** gpui's layout gives the element its width every frame;
  `SurfaceViewOptions::width` / `set_width` FIX it instead.
- **Visible region.** Read from the element's clip in prepaint: the part of
  the surface the host shows. Windowed lists without a bounded height window
  off it. Dialogs, drawers and toasts are placed in it (centred in what the
  user sees, not from the surface top). `set_viewport_height` overrides the
  height.
- **Settings** (`SurfaceSettings`, `set_settings`): locale (week start, RTL
  for `ar`, `he`…), string overrides, mode `light | dark | system`
  (`system` follows the window appearance live), density, contrast, font
  scale, hover capability, reduced motion, safe-area insets, today. When
  `settings` is given, its `mode` wins over `SurfaceViewOptions::mode`.
- **Fonts.** Register them with the text system BEFORE the first frame
  (`cx.text_system().add_fonts`). gpui caches a failed lookup, so a font
  registered later needs `SurfaceView::fonts_changed` (every family
  re-resolves, every leaf is measured again). A theme family the text system
  cannot load becomes the platform's sans (`.SystemUIFont`) or the first
  installed mono, never gpui's last-resort stack. The CSS generic names
  (`ui-monospace`, `system-ui`, `sans-serif`) resolve the same way. Every
  font carries a glyph fallback chain (Noto, PingFang, Segoe, the emoji
  fonts). `fontStyle: italic` and the weights 300–800 reach the font.
- **Icons.** gpui-component's icon SVGs must be in the app's `AssetSource`
  (the IDE's `Assets` falls back to `gpui_component_assets`).
- **Host** (`HostPlugin`, every method defaulted):
  - `on_action` gets every A2UI `action` (with `surface_id`).
  - `on_input` gets host-owned text edits (below).
  - `open_url` handles `openUrl` and `Link`; the default opens only what
    `url_policy()` allows (a denied `Link` or markdown link paints as
    text). `resolve_url` rewrites a media src BEFORE the media policy.
  - `on_function_call` gets host functions (a `functionCall` to a
    non-built-in name; the runtime gates them).
  - `media_request` maps a media source to a url plus headers (default:
    `resolve_url` then `media_options()`: https/http/data, `file` only when
    listed; `None` = denied). Whatever it returns is re-checked against
    `media_options()`'s schemes and hosts.
  - `open_media_file` opens a fetched (or host-allowed `file:`) Video /
    AudioPlayer source in the system player (default
    `cx.open_with_system`); see Playback.
  - `on_paint_error` hears a painter that panicked (it paints an empty
    box); the host runtime forwards it as an A2UI `RENDER_FAILED` error.
  - `announce(text, live)` speaks live regions, Form errors, `copied`. The
    painter also exposes the latest announcement as a `status` a11y node;
    gpui has no live-region API.
  - `pick_files` may open its own picker for a FileUpload (return `true`,
    then call `SurfaceView::files_picked`). The default is gpui's
    `prompt_for_paths`.
  - `on_upload` gets the PATHS of picked or dropped files (the bytes never
    travel in an event; the surface's `upload {files}` carries name, size,
    type).
  - `on_unknown` fires once per structure version for each `Unknown` node.
  - `markdown` may return a richer renderer (`None` = the built-in one).
  - `font_family` maps a theme family name to a loaded family.

## The measure path

`GpuiMeasure` implements `exponential_ui::measure::Measure` and answers per
request, with first baselines (`alignItems: baseline` aligns text):

- Plain text breaks where the web breaks (`text`, UAX #14 through
  `unicode-linebreak`, tailored like the browsers: no break inside
  `domain.tld/path`). Min-content is the widest unbreakable segment (one
  ideograph in CJK), max-content the widest hard line, trailing spaces hang.
- The painter draws the very lines the measurer counted (hard breaks,
  `white-space: nowrap`), so the reserved height IS the painted height.
  `lines: 1` truncates with an ellipsis, `lines: n` clamps with an ellipsis
  on the last line. `textTransform` applies before both.
- Every answer is the BORDER box: the leaf's per-side padding and border are
  added and the recipe's fixed and minimum sizes win.
- One-line text parts reserve their chrome: a tab's icon and count, an
  accordion chevron, a Select option's check slot, a menu row's icon, a
  sortable header's arrow (the request's `owner_component` says whose part
  it is).
- Fields: Input 160 wide; Textarea `rows` lines, or with `autosize` its LIVE
  text's wrapped lines clamped to `rows..maxRows`; the Composer its live
  text. A growing field is re-measured on every edit, not after the
  debounce. Host one-line fields (`search`, NumberField / ChipInput
  `input`) are their text or placeholder wide.
- Picker triggers (Select, DatePicker, DateRangePicker, TimePicker) are the
  core's `text` plus their glyph, at least 160 wide.
- `Markdown` runs the SAME block layout the painter places blocks with;
  `lines` clamps it.
- `Extension` leaves ask their `ExtensionPainter::measure`.
- Identity: the resolved font families and the font epoch, so a theme
  change or `fonts_changed` invalidates the core's memo.

`SurfaceViewOptions::fixed_measure` swaps in the core's `FixedMeasure`
(golden geometry: the painter replays `layout-geometry*.json` with it).

## Interaction

- **Presses.** On a pressable node, mouse down sets `pressed`; mouse up
  inside fires `event(index, "press")`. The core owns every control's local
  value now (checkbox, radio, select, toggle, toggle group, pickers, chips,
  table sort and selection): the painter keeps no mirrors. `OutEvent`s
  dispatch to the host (`Action`, `OpenUrl`, `FunctionCall` → `on_function_call`, `Input`, `Announce`,
  `Copy` → the clipboard, then `reset` after 2 s, `PickFiles`, `Focus` → a
  focus move after the next pass).
- **States.** `hover`, `pressed`, `focus` and `focus-visible` (keyboard
  focus only; any pointer press ends keyboard mode) reach the core through
  `set_states`, so recipes and `:hover` / `:focus-visible` / `:pressed`
  blocks restyle. A keyboard-focused node with no recipe ring gets a
  fallback ring in the theme's `ring` colour. A file drag over a FileUpload
  drop zone sets `dragover`.
- **Keyboard** (`catalog/a11y.json`, arrows mirrored in RTL):
  - `Tab` / `Shift-Tab` move in PAINT order with roving stops (the selected
    tab, the checked radio). An open modal or menu layer traps focus.
  - `Enter` / `Space` press. A focused scroll container scrolls with the
    arrows, PageUp/PageDown, Home/End and Space.
  - Tabs: arrows move and activate (wrapping), Home/End. Radio: arrows move
    and select. Segmented: arrows move the roving item, Space/Enter
    toggle it (a `bar` is a navigation of button items). Accordion: Up/Down/Home/End between headers.
  - Menus and listboxes: ArrowDown on a trigger opens it; focus lands on the
    selected option (else the first). Up/Down wrap, Home/End jump, a printed
    character jumps to the next match (type-ahead), Enter chooses.
    ArrowRight opens a submenu, ArrowLeft closes it.
  - Calendars: arrows ±1 day / ±1 week, PageUp/PageDown a month (Shift: a
    year), Home/End the row; off the shown month the calendar pages and
    focus follows.
  - Chart (a tab stop): ArrowLeft/Right move the tooltip between
    categories (slices), wrapping; Home/End; Escape hides it.
  - Slider: arrows ± step, PageUp/PageDown ± 10 steps, Home/End.
    NumberField: Up/Down ± step (Shift ×10), PageUp/PageDown ×10.
    ChipInput: Backspace in the empty field removes the last chip.
  - Shift+F10 opens the context Menu (`openOn: contextmenu`) around the focused node; a right click
    opens it at the pointer.
  - `Escape` closes the top DISMISSIBLE layer and returns focus to its
    trigger; a non-dismissible AlertDialog presses its cancel; then it
    hides a tooltip, then dismisses a focused toast; a busy Composer stops.
- **Text fields.** Input and Textarea `.field`, the Composer, NumberField and
  ChipInput `input` and the Select `search` are host-owned: a gpui-component
  `InputState` / `TextareaState` per node OWNS the text.
  - Each edit bumps the field's revision and re-arms a 150 ms debounce. The
    debounce sends the core `change {value}` (bound values write through;
    `validateOn: change` checks run) and the host
    `InputEvent { kind: Change, revision }`.
  - Blur sends `commit` + `blur` (`validateOn: blur`); Enter sends `submit`
    (a single-line field submits its Form). Composer Enter / send =
    `submit`, then the field empties; a busy composer sends `stop`. A comma
    or Enter in a ChipInput adds the chip and empties the field.
  - An echo (a changed prop) is written in only when the field is not
    focused and no newer edit is outstanding.
  - `type_into` drives a field through the platform text-input entry point
    for automation.
- **Slider** drags snap to `step` in `[min, max]` (the minimum on the right
  in RTL); the release fires `change`. **Drags** (slider, scrollbar thumb,
  drawer sheet) follow the pointer anywhere in the window.
- **Markdown text selects.** A press-drag selects across paragraphs,
  list items, quotes, table cells and code blocks (the drag follows the
  pointer anywhere in the window), Shift-press extends, the platform copy
  shortcut (Cmd-C / Ctrl-C) copies the text with a line feed between
  units; any other press drops it. The selection paints on the theme's
  `ring` colour at 30 %. `selected_text`, `select_all_markdown` and
  `copy_selection` drive it for automation.
- **Hover-opened overlays.** A Tooltip or `openOn: hover` Popover
  trigger gets its `hover` state 300 ms after the pointer enters (the core
  opens it), keeps it while the pointer is over the card, and loses it
  120 ms after leaving. Keyboard focus inside the trigger opens it at once.

## Overlays

Open layers (`LayoutOutput::layers`, in order) paint ABOVE the tree as gpui
`deferred` elements (overlays first, toasts above them).

- Layers placed against the viewport (Dialog `centered`, Drawer edges,
  toasts) move into the visible region; anchored and pointer layers sit at
  their surface-coordinate frames.
- `modal` layers get a scrim over the visible region (the `overlay` recipe's
  colour). A scrim press is `event(root, "dismiss")`, which the core refuses
  for a non-dismissible layer.
- Non-modal overlay layers dismiss on a press outside every open layer (a
  press in a submenu or nested popup does not close its parent). A press on
  their own trigger does not reopen them.
- A Drawer's handle or sheet drags toward its edge; past 64 px it dismisses
  (when `dismissible` and `dragToDismiss`).
- A tall Dialog or Drawer scrolls its `body` (a core scroll container).
- Toasts: the painter runs each toast's `duration` (0 = sticky), paused while
  hovered, then `dismiss_toast` (the core fires `dismiss` + `change`).
- New layers enter with a fade and a short rise over the theme's
  `motion.fast`; a closed layer EXITS the same way (it keeps painting from
  its last state, scrim included, fading and sinking back, never hit-tested
  and absent from the a11y tree; a reopen drops the ghost). The ghost is a
  snapshot renumbered into its own slots (`GhostWorld`: nodes, frames,
  styles, fonts, inks, scroll offsets, list windows, chart tooltip) taken
  from the PRE-pass caches, so a compaction or a `surface_mut` rebuild in
  the closing pass cannot paint other nodes in its place. None of it runs
  under reduced motion. Focus moves into a newly opened modal or menu
  layer, never into a toast or tooltip.

## Scroll containers and windowed lists

- Every `ScrollOutput` (any `overflow: scroll | auto` node, a windowed
  list, a dialog body, a CodeBlock body) clips, translates its descendants by
  the core's offset and draws overlay scrollbars (draggable thumbs) when it
  overflows. Nested containers compose.
- The wheel / trackpad moves the core's offset (`scroll_to`); a scroll that
  hits an edge is not consumed, so it chains to the host.
- A windowed List or Table WITH a bounded size scrolls itself; one without
  windows against its nearest scrolling ancestor, else the host's scroller:
  prepaint reports the visible offset (`Surface::set_surface_scroll`) and lays
  out again in the same frame when it moved by at least 1 px.
- Round 2: horizontal lists window on x; `sectionBy` + the `section` slot
  build section headers (level-3 headings), `stickyHeaders` pins the current
  one; `position: sticky` nodes paint at the core's sticky offsets, on top of
  their siblings, but keep their DOM place in the element (AccessKit) order;
  the siblings' hit areas stop at a pinned box. A windowed item carries its
  place in the whole list (`posInSet` / `setSize`). `scroll_to_index` (or `command(ScrollToIndex)`) moves the list's,
  its ancestor's or the host's offset; for the host's it calls
  `HostPlugin::scroll_surface(x, y)`.

## Round 2

| feature | gpui |
|---|---|
| Resizable | handles are hairlines with an 8 px hit area (`resizeHandleHit`); a drag sends `drag {phase, delta}` from the start, arrows / Home / End / Enter send `key`; handles are `Splitter` nodes with value, range and orientation |
| `position: sticky` | painted at `LayoutOutput.sticky` offsets, last (on top), in DOM order for AccessKit |
| `backdropBlur` | gpui samples nothing behind a box: the translucent background paints alone |
| `animation` | the core's frames (`frame_with_timing`) from the node's first paint: opacity, translate, a leaf's scale, an Icon's rotation, the shimmer band; reduced motion = the rest frame |
| formatting | `SurfaceViewOptions::formatter` / `set_formatter` (else the core's English one), also for chart ticks, values and tooltips; a view re-binds once a minute only while it shows a relative time (`ticks_minutely`); `set_clock` pins `now` |
| direction | a leaf's text aligns by the core's physical `text_align` and shapes with the node's direction as its bidi paragraph direction (a leading RLM / LRM when the first strong character disagrees) |
| strings | every built-in label a surface paints comes from the strings table (`controls::*` are IDE chrome, English) |

## Motion

A node whose resolved style has a `transition` animates from the style it
SHOWED to the new one: colours (sRGB), opacity, radii and the transform, and
a leaf's size (a progress fill), eased by `transitionEasing`'s cubic bezier.
The view requests animation frames while anything runs. Reduced motion: the
core resolves transitions to 0 ms, the view starts no layer enter or exit
animation and the Skeleton stops pulsing. The Skeleton pulses like the web's
`xui-pulse` (opacity 1 → .5 → 1 over 2 s).

## Charts

`Chart` draws bar, stackedBar, line, area, pie, donut and sparkline with
gpui paths from the core's `props.chart` numbers: grid lines and y labels at
the nice ticks, category labels, `xLabel` / `yLabel`, `showValues` labels
(ticks, values and tooltips through the surface's formatter),
the series colours the core resolved into the visual, the donut hole, a
legend for 2+ series or slices, and a tooltip (the `tooltip` recipe) for the
category or slice under the pointer (or the keyboard's). The plot fills the leaf's frame (its
height is fixed by the core) minus the title and legend rows.

## Extension painters

`register_painter(kind, Box<dyn ExtensionPainter>)` covers the
`Extension` nodes whose `extension_kind` is `kind`.

- `measure(leaf, wrap, window, cx)` answers the border box (`None` = 0×0).
- `paint(PaintContext { node, visual, text_style, width, height, children,
  theme, mode, emit }, window, cx)` returns the element drawn INSIDE the
  frame div. `emit(event, payload, …)` routes through `event(index, …)` like
  a native.
- An unregistered kind paints its name as a note.
## The host API (VAPP-91)

`runtime::ExponentialHost` is the gpui host runtime, the Rust mirror of the
TS reference (`packages/exponential-ui/src/host/runtime.ts`) over the core's
pure `exponential_ui::host` (router, decoders, policy, sources, packages).
It is a gpui entity that owns:

- the transport, with an observable `status()` (`connecting|open|closed|error`),
  `status_detail()` and `has_transport()`;
- the router, plus ONE `SurfaceView` entity per surface (`surface(id)`,
  `surface_ids()`). Ops: `create` makes a fresh view with the host's theme,
  mode, extensions and painters; `components` merges by id; `data` sets or
  removes a path (`""` = the whole model); `bind` subscribes the scheme's
  `SourceResolver`, or sends `VALIDATION_FAILED`; `delete` drops the view
  and cancels its sources; `send` forwards and records `unsupported_catalog()`;
- the function registry (`HostFunction` returns a gpui `Task`, so it may be
  async; `sync_function` wraps a plain closure), the `HostPolicy`
  (`functions`, the `on_function_call` consent hook returning `Task<bool>`,
  `urls`, `open_url` (default `cx.open_url`), `media`), and installed
  packages (a template's surface narrows the gate with its package's
  `functions`).

```rust
let host = cx.new(|cx| ExponentialHost::new(HostOptions {
    transport: Some(Box::new(JsonlStreamTransport::new(
        HttpTransportOptions::new(format!("{server}/a2ui.jsonl")).post_url(format!("{server}/action")),
    ))),
    extensions: vec![parse_extension(&ext_json)?],
    painters: vec![("TrendLine".into(), Rc::new(TrendLine))],
    theme: Some(Arc::new(theme)),
    mode: Mode::Light,
    ..Default::default()
}, cx));
host.update(cx, |h, cx| h.connect(cx));
// render: host.read(cx).surface("greenhouse") is an Entity<SurfaceView>.
```

- **Threading.** A transport or a source emits from any thread. One channel
  lands everything on the main thread (the host's pump task); after `close`
  or a re-`connect`, stale events are dropped.
- **Painter events.** The painter's `HostPlugin` for host surfaces is
  `ExponentialHost::plugin(base)` / `runtime::host_plugin`:
  - `on_action` sends the A2UI `action` client message (and still calls
    `base.on_action`);
  - `on_function_call` (`OutEvent::FunctionCall`) runs `call_function`:
    `not_found` sends `FUNCTION_NOT_FOUND`, `ask` asks the consent hook,
    `deny` sends `FUNCTION_DENIED`, `allow` runs the handler;
  - `open_url` goes through the URL policy (`url_policy()` = the host's);
  - `media_request` applies the media policy + rules;
  - `on_paint_error` → `ExponentialHost::paint_error` (once per surface +
    component + message, reset by new components).

  Everything else comes from `base`. A plain `SurfaceView` (no host) gets
  `HostPlugin::on_function_call` (a default no-op).
- **Transports** (`transport`). `MemoryTransport` (`feed`, `feed_jsonl`,
  `sent`) is always there. With the default `net` feature you also get:
  - `JsonlStreamTransport` / `SseTransport`: a streamed GET on a std thread
    (blocking reqwest, no tokio under gpui) plus ordered POSTs of client
    messages to `post_url`. A clean end of stream closes the transport; it
    reconnects after an error, or after a clean end only with
    `resumable(true)` or an SSE `retry:` field, waiting `reconnect_ms`
    (unset = the `retry:` value or 2000, 0 = never). POSTs and MCP calls
    are bounded (`POST_TIMEOUT`, `MCP_CALL_TIMEOUT`); a failed send drops
    that message and reports through the status sink;
  - `WebSocketTransport`: one message or JSONL per frame, on a private
    current-thread tokio runtime;
  - `McpTransport`: JSON-RPC `initialize` then `tools/call`; client messages
    go back as `mcp_action_call`.

  All decoding is the core's (`JsonlDecoder`, `SseDecoder`,
  `messages_from_mcp_result`). A stream's `close` stops delivery at once;
  its blocked read returns at the next chunk.
- **Media** (`media`). `Image`, `Avatar`, `Video` posters and markdown
  block images load through `HostPlugin::media_request` (the media policy:
  no local file unless the host lists `file`): the absolute url plus the
  media rules' headers, fetched by `media::MediaLoader` (a gpui `Asset`,
  cached per url + headers) under the contract's `media.limits`: the whole
  request within `MEDIA_TIMEOUT_MS`, Content-Length and the body within
  `MEDIA_MAX_BYTES`, the header's width × height within `MEDIA_MAX_PIXELS`
  before any decode; every redirect hop passes the policy again. gpui's own
  `img(url)` needs an app-installed `http_client` (the IDE has none) and
  cannot carry headers.
- **Playback** (`media::play`). gpui has no audio or video pipeline, so
  `Video` and `AudioPlayer` do not play inline: the play press hands the
  policed `src` to the SYSTEM player. An http(s) src without headers goes
  through `HostPlugin::open_url` (the URL policy); a request with headers
  or a `data:` url is fetched under `media.limits` into a temporary file
  (named for its MIME type), which `HostPlugin::open_media_file` opens
  (default: `cx.open_with_system`), as does a host-allowed `file:` src. A
  denied src leaves the play control inert. `autoplay` opens nothing, and
  the AudioPlayer bar stays `0:00 / durationMs` (the system player has the
  controls).
- **Tests.** `tests/host.rs` covers ops through the host, sources and
  cancel, the gate, consent and package narrowing, the URL policy, media
  headers, a MemoryTransport round trip, presses becoming client messages
  and function calls, and unsupported catalogs.

## Conformance

`cargo test -p exponential-ui-gpui --test suites` runs all 24 suites of
`packages/exponential-ui/conformance/manifest.json` (every case; the round-2
pure suites through the core's shared cases) and writes a pass/fail report,
`<repo>/.conformance/exponential-ui-gpui.json` (or
`$EXPONENTIAL_UI_CONFORMANCE_REPORT`). Check it with `bun run --filter
@exponential-at/ui conformance:check <abs path>`.

The painted suites go through a `SurfaceView` in a headless window:

- catalog, extension, replay: every visible node is painted, nothing is
  `Unknown`, and the paint order is the pre-order;
- layout and overlay: `SurfaceView::set_measure` puts in the fixed geometry
  measure, and the frames the divs are placed at are checked;
- control-geometry: the measurer's border box (the frame) plus the painted
  part visual.

The headless window's NoopTextSystem shapes no text. So control-geometry
checks only the ControlBox-derived box (fixed and minimum sizes, padding,
border, radius) and never shaped label widths.

Real text is the other half (round 1, VAPP-98): `tests/conformance.rs` lays
out every case of `fixtures/conformance-cases.json` with gpui's Linux text
system holding ONLY the conformance fonts (`examples/conformance_dump.rs`)
and compares the frames with the committed web baseline
(`fixtures/conformance-baseline.json`). It is a ratchet over
`fixtures/conformance-known.json`: a case that gets worse fails, and so does
an origin not listed in the test's `SURVIVORS` (each with a reason). The
writer (`EXP_UI_WRITE_FIXTURES=1`) keeps `causes` and `rules` and rewrites
`renderer`, `survivors`, `coverageGaps` and `cases`. A node only one side
places fails unless the test's `COVERAGE_GAPS` lists it with a reason.
Today: 48 cases, 0 size / position / wrap divergences, 0 origins, 0
survivors; 9 coverage gaps (8 parts gpui paints inside a leaf, the
Resizable grip the DOM does not tag).

## Public API

- `host`: `HostPlugin`, `NoHost`, `ActionEvent`, `InputEvent`, `InputKind`,
  `FilePickRequest`, `UploadEvent`, `FunctionCallEvent`.
- `runtime` (VAPP-91): `ExponentialHost`, `HostOptions`, `HostPolicy`,
  `HostFunction` / `sync_function`, `FunctionCallInfo`, `FunctionOutcome`,
  `SourceResolver` / `Emit` / `Cancel`, `ConsentHook`, `OpenUrlHandler`,
  `host_plugin`, `now_iso`.
- `transport`: `Transport`, `TransportSink`, `TransportEvent`,
  `TransportStatus`, `MemoryTransport`; with `net`: `JsonlStreamTransport`,
  `SseTransport`, `WebSocketTransport`, `McpTransport`,
  `HttpTransportOptions`, `McpTransportOptions`.
- `media`: `image_source`, `MediaLoader`, `MediaKey`, `sniff_format`.
- `extension`: `ExtensionPainter`, `PaintContext`, `EmitFn`.
- `view`:
  - Types: `SurfaceView`, `SurfaceElement`, `SurfaceViewOptions` (Default:
    core catalog, `default_theme()`, dark, `NoHost`), `PassStats`,
    `VisibleRegion`, `NodeFlags`, and the focus helpers `focus_order` /
    `next_focus` / `is_focusable`.
  - `SurfaceView`: `new`, `apply`, `set_nested`, `set_data`, `set_theme`,
    `set_mode`, `set_settings`, `fonts_changed`, `register_painter`,
    `surface`, `surface_mut`, `stats`, `set_viewport_height`, `set_width`,
    `index_of`, `layout_now`, `layers`, `focused`, `focus_order`,
    `focus_next`, `handle_key`, `press`, `fire`, `escape`, `flush_field`,
    `submit_composer`, `type_into`, `field_text`, `files_picked`,
    `node_states`, `is_rtl`, `visible_region`, `announcement`, `origin`,
    `exiting_layers`, `exiting_nodes`, `chart_point`, `selected_text`,
    `select_all_markdown`, `copy_selection`, `accessible_info` (role,
    name, description per node and per built-in sub-control: gpui only
    builds the platform tree while a reader is on), `advance_clock` (the
    motion clock), `record_bounds` / `painted_bounds` (automation and
    tests); `natives::record_tracked` / `take_tracked` log every tracked
    line's painted glyph extent. VAPP-91: `without_window` (a host creating
    views outside a window update), `set_components`,
    `register_painter_rc`, `set_measure`, `placed_nodes`, `frame`,
    `surface_height`, `trace_paint` / `paint_trace`. Round 2: `command`,
    `scroll_to_index`, `set_formatter`, `set_clock`, `painted_lines` (the
    lines a Text paints: the conformance dump's count);
    `SurfaceViewOptions::formatter`, `HostOptions::formatter`,
    `HostPlugin::scroll_surface`.
- `measure`: `GpuiMeasure`, `tracking_width` and unit-free helpers: `border_box`,
  `inner_wrap`, `insets`, `top_inset`, `len_of`, `text_chrome`,
  `chart_legend`, `display_text`.
- `text`: `segments`, `wrap`, `min_content`, `max_content`, `hard_lines`,
  `transform`, `hang`, `with_paragraph_direction`.
- `paint`: `PaintStyle`, `Affine`, `GradientPaint`, `cubic_bezier`,
  `color::{parse_hex, mix}`, `parts::part_visual` / `part_props`, `chart`,
  the `markdown` parser, layout and selection (`MdSelection`, `MdUnits`,
  `highlight`), `date`, `icons`, `natives` (incl. `focal_bounds`,
  `tracked_text` / `tracked_width`).

`examples/kitchen_sink.rs` opens the kitchen sink in a real window
(`XUI_THEME`, `XUI_MODE`, `XUI_LOCALE=ar` for RTL, `XUI_OPEN=1`,
`XUI_SCROLL`). Set `EXP_UI_TRACE=1` to print one `[exponential-ui gpui] …`
line per pass (and every announcement).

## Numbers

`cargo test -p exponential-ui-gpui --release --test render bench --
--nocapture` on an x86-64 Linux desktop (2026-10-08). The headless test
window uses gpui's NoopTextSystem, so these isolate the core and the
painter, not real shaping.

| pass | release |
|---|---|
| bench 205 nodes, first pass of a fresh view (best of 5) | ~2.3 ms · 398 measure calls · 2 upcalls (the core alone on `FixedMeasure`: ~2.0 ms cold on the same machine) |
| bench, steady (nothing changed) | 14–18 µs · 0 measure calls |
| bench, full frame (layout + element tree + paint) | ~1.4 ms |

Almost all of the cold pass is the core's own rebuild, restyle and taffy.

The round-2 list bench (`fixtures/bench-list.json`: 100,000 `ListRow`s,
390 × 800, neutral), headless with gpui's Linux text system and the
conformance fonts, every step a full frame (core pass + gpui layout and
paint): `RUSTUP_TOOLCHAIN=1.96.0 cargo run --release -p exponential-ui-gpui
--example bench_list` on an i9-13900K, Linux (2026-10-08).

| metric | release |
|---|---|
| firstPaintMs | 87.2 |
| scrollStepMs (100 viewport steps) | 6.52 |
| scrollToIndexMs (index 50000, start) | 9.25 |
| renderedItems | 30 |

## Tests

- `tests/render.rs`: every catalog component and macro case in every
  built-in theme × mode with no `Unknown`, and again (neutral light) with
  every overlay open; the basic catalog; the extension fixture
  (`TrendLine`); the kitchen sink in every theme × mode plus geometry mode,
  with every overlay open; `layout-geometry.json` and
  `layout-geometry-round1.json` replayed through the PAINTER with their
  fixed measures, every node's painted bounds = the fixture frame (LTR and
  RTL, ± 0.5 px of gpui's pixel snapping); presses and overlays; typing and
  echoes; the bench.
- `tests/interaction.rs`: the keyboard spec (tabs, radios, sliders, menus,
  selects, RTL arrows), `:focus-visible`, dismissal rules, hover and
  keyboard tooltips, scroll containers and wheel chaining, a tall dialog's
  body scroll, a dialog centred in a host scroller's visible region, a
  windowed list following the host scroller, toast timers and the hover
  pause, Form errors (announce + focus), file picks, the clipboard and its
  reset, the context menu at the pointer, chips, drawer drags, the first
  frame's width, a closed dialog's exit fade (not hit-tested, driven by
  `advance_clock`; none under reduced motion; correct through a compaction
  and a `surface_mut` close), `letterSpacing` in the painted glyphs (Text,
  Button, Link), the chart's keyboard tooltip (LTR and RTL, cleared on
  blur, sparklines no tab stop), macro roles (Card/Group = group, Alert =
  alert/status), built-in labels from the strings table, Markdown
  drag-select + copy and a selection dropped when its text changes.
- `tests/round2.rs`: Resizable drag from the start sizes, keys, hover on
  the hit area and the splitter node, in ltr and rtl; a sticky box and a
  pinned section header (element order, presses, heading level); a slide-in
  animation and its rest frame under reduced motion; `scroll_to_index` on
  the list's scroller and the host's (`HostPlugin::scroll_surface`); a
  windowed row's place in set; horizontal windowing in ltr and rtl; the
  minute tick; the §7 sizes (Video 16:9, Image ratio, Badge,
  Segmented, Radio gap, Slider row); the Accordion count.
- Unit tests for text breaking, paint styles, transforms, easing, motion,
  rounded clips, scrollbar geometry, RTL natives, data URIs, image focal
  bounds, Markdown selection ranges and highlighted runs, icons (every
  `builtinIcons` slot has a glyph), charts, the node cache's delta patch and
  the focus order.

## Known divergences from the React renderer

- Inline Markdown code uses the body size (gpui text runs share one font
  size; CSS uses `.925em`). Headings share the recipe's one size, as in
  React. Selection is drag/Shift-press only: no double-click word or
  triple-click paragraph selection, no select-all shortcut inside the
  surface (`select_all_markdown` exists for hosts).
- `letterSpacing` paints glyph by glyph: ligatures stay as shaped, and a
  joining script (Arabic) keeps its joined glyph shapes with gaps between
  them (CSS Text asks user agents not to space cursive scripts at all).
  Editable inputs (Select search, NumberField, ChipInput, Input, Textarea,
  Composer) show their text untracked.
- `transform` on a CONTAINER only translates; scale and rotation apply to
  leaves (scale) and icons (rotation). gpui transforms only sprites.
- Images: `loading: lazy` loads at once. Video and AudioPlayer are static
  placeholders: nothing plays.
- Numbers and dates use the core's English formatter unless the host passes
  one (`SurfaceViewOptions::formatter`).
- `backdropBlur` paints the background alone (no backdrop sampling in gpui).
- Animations: containers translate and fade but do not scale or rotate;
  `spin` turns Icons only.
- The Accordion count paints inside its trigger (measured as its own muted
  run) but is not a laid-out node, so the dump has no `accordion.count.<i>`;
  likewise Image/fallback, Ring/label, Video/controls and AudioPlayer
  track/controls are painted, not placed.
- Textarea keeps gpui-component's inner padding (single-line fields start at
  border + `paddingHorizontal`).
- Charts are not mirrored in RTL (neither are React's SVG charts); their
  arrow keys are.
- Two paths cannot be exercised headless: OS file drops (the drop handler
  calls the same `files_picked` the tests drive) and native platform
  pickers.
- Images load through the host's media request when it has one (headers;
  `media::MediaLoader`), else from urls, files and `data:` URIs.
