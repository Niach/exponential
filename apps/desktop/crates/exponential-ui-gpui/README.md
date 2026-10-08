# `exponential-ui-gpui`: the desktop painter

The gpui painter of the Exponential UI SDK (VAPP-90). The core crate
[`exponential-ui`](../exponential-ui) reduces an A2UI surface, resolves the
theme and lays the tree out with taffy. This crate measures text for it
in-process and paints the result in gpui. It depends on the core, `gpui`,
`gpui-component`, `serde` and `serde_json`, and on nothing from the
Exponential app (`ui`, `theme`, `domain`…). No HTML, no webviews.

## The painting model (the VAPP-4 verdict)

- The core returns ABSOLUTE frames in surface coordinates. Pre-order is paint
  order and accessibility order.
- The painter draws ONE absolutely positioned gpui `div` per `PlacedNode`, at
  its frame RELATIVE to its parent's frame. The divs nest like the tree, so
  `overflow_hidden`, `opacity` and the text style (colour, font) inherit the
  way CSS does. Catalog styles are never mapped onto gpui's `Styled`
  flex/grid layout. gpui only rasterises.
- A container paints only its box: background, radius, shadow, opacity and
  clip. Its border is an overlay child, so it never offsets the children:
  their frames already include the border.
- A measured leaf paints its content inside the box, inset by the recipe
  padding and border the measurer counted (`Visual::padding_*`).
- Text colour: `visual.color`, else the nearest ancestor's colour, else the
  theme's `foreground`. Font: `visual.font_family` (a family NAME) goes
  through `HostPlugin::font_family`. A missing family means the theme's
  `sans`.
- Hidden nodes (inactive tabs, closed accordion panels) paint nothing.

### Sub-parts painted from recipes

The core synthesizes `<id>.<part>` nodes for labels, fields, tabs, triggers,
dialog parts and so on. Parts it does NOT synthesize are drawn from the
owner's recipe with `paint::parts::part_visual(theme, mode, owner_component,
part, owner_props, states)`, which resolves them exactly like the core does:

- Checkbox `check`, Switch `thumb`, Slider `range`/`thumb`
- the Select / DatePicker `trigger` (the `.field` leaf has no `field` recipe)
- Button `icon`, Ring `track`/`fill`, Chart `axis`/`grid`/`legend`
- TreeGuides `line`, Composer `field`/`send`/`attachment`
- Carousel `indicator`, the `Markdown` block parts

Chrome glyphs (check, chevrons, close, search, calendar, send, attach,
loader, play, image) come from gpui-component's `IconName`, so a host with
no icon registry still gets working controls. Catalog icon names are
registry CONCEPTS resolved through `HostPlugin::icon(name)` (an SVG asset
path). Unknown names paint the placeholder circle.

## Embedding

```rust
let view = cx.new(|cx| SurfaceView::new(SurfaceViewOptions {
    surface_id: "s1".into(),
    theme: Some(exponential_ui::themes::builtin_theme("exponential").unwrap()),
    mode: Mode::Dark,
    host: Rc::new(MyHost),            // impl HostPlugin
    ..Default::default()
}, window, cx));
view.update(cx, |v, cx| {
    v.register_painter("Sparkline", Box::new(MySparkline));   // extension natives
    v.apply(&message, cx).unwrap();                           // A2UI messages
    v.set_viewport_height(visible_height, cx);                // windowed lists, Dialog centring
});
// render: wrap `view` in YOUR scroller; it is `w_full()` × the content height.
```

- **Width.** The surface width is the element's own width, probed by a
  canvas at prepaint (one frame of lag; 900 until known). `set_width`
  overrides it.
- **Fonts.** Register the fonts with the text system before the first frame:
  `cx.text_system().add_fonts(vec![…])`. The IDE registers Inter. Geist
  (neutral) and Nunito (playful) fall back to the system font unless the
  host adds them or maps them in `font_family`.
- **Icons.** gpui-component's icon SVGs must be in the app's `AssetSource`
  (the IDE's `Assets` falls back to `gpui_component_assets`).
- **Host.** `HostPlugin` callbacks:
  - `on_action` gets every A2UI `action` (with `surface_id`).
  - `on_input` gets host-owned text edits (below).
  - `open_url` handles `openUrl` and `Link`.
  - `resolve_url` maps image/video/avatar sources.
  - `on_unknown` fires once per structure version for each `Unknown`
    placeholder.
  - `markdown` may return a richer renderer (`None` = the built-in one).

## The measure path

`GpuiMeasure` implements `exponential_ui::measure::Measure` and answers per
request, like `DirectMeasure`:

- widths come from `window.text_system().shape_line`
- wrapped heights come from `shape_text(.., Some(px(wrap)), clamp)`
- `None` wrap means max-content; `Some(0.0)` means min-content (the widest
  word)

Every answer is the BORDER box. The leaf's `ControlBox` padding and border
are added around the shaped content, and its fixed and minimum sizes win.
A few cases differ:

- A one-line (`lines: 1`) text shrinks to 0, like `white-space: nowrap;
  min-width: 0`.
- Select/DatePicker fields size from their `trigger` recipe.
- `Extension` leaves ask their `ExtensionPainter::measure`.
- `Markdown` runs the SAME block layout the painter places blocks with, so
  the reserved height is the painted height.

gpui's line-layout cache absorbs taffy's repeats. The core's memo means a
pass with nothing changed makes 0 measure calls.

## Interaction

- **Presses.** On a pressable node, mouse down sets `pressed`; mouse up inside
  fires `event(index, "press")`. The returned OutEvents are dispatched:
  `Action` → `on_action`, `OpenUrl` → `open_url`, `Input` → `on_input`.
  Hover sets `hover`. Disabled nodes ignore presses. `SurfaceView::press`
  is the same press from code.
- **Unbound controls.** The core writes only BOUND values through. The painter
  keeps a local mirror for unbound Checkbox, Switch, Radio, Toggle,
  ToggleGroup, Select, DatePicker and Slider values, and re-resolves their
  part visuals from it until the prop changes.
- **Keyboard.** Every focusable node gets a `FocusHandle`. Text fields use their
  input's own handle.
  - `Tab`/`Shift-Tab` move in pre-order. While a non-tooltip layer is open,
    only inside it (focus trap).
  - `Enter`/`Space` press the focused node.
  - Arrow keys step a focused Slider or Carousel indicator.
  - The focused node carries the `focus` state.
  - A newly opened layer focuses its first focusable (never a text field).
  - `Escape` (`SurfaceView::escape`) closes the select/date popup, else the
    TOP layer, and returns focus to its trigger.
- **Text fields.** Input and Textarea `.field` leaves and the Composer are
  host-owned. A gpui-component `InputState`/`TextareaState` per node id OWNS
  the text.
  - Each edit bumps the field's revision and re-arms a 150 ms debounce. The
    debounce sends the core `change {value}` (bound values write through) and
    the host `InputEvent { kind: Change, revision }`.
  - Blur and Enter flush and send `Commit`. Composer Enter / send = `submit`,
    then the field empties; a busy composer sends `stop`.
  - An echo (a changed `value` prop) is written in only when the field is not
    focused and no newer edit is outstanding.
  - The input draws with `.appearance(false)` inside the node box, which
    paints the recipe chrome.
  - `type_into` drives a field through the platform text-input entry point
    for automation.
- **Popups.** Select opens an options popup (`searchable` adds a filter,
  `multiple` toggles); DatePicker a month grid from the `calendar`/`day`
  recipes. Both fire `change {value}` and close on an outside click.
- **Slider** drags snap to `step` in `[min, max]`; the release fires `change`.
  **ToggleGroup** fires `change {value}` (an array for `multiple`),
  **Carousel** dots `change {page}` on the Carousel.

## Overlays

Open layers (`LayoutOutput::layers`, in order) paint ABOVE the tree as gpui
`deferred` elements at their root frame (surface coordinates).

- Dialog and Drawer add a scrim over the surface. The colour comes from the
  `overlay` recipe. A click dismisses unless `dismissible: false`.
- Popover and DropdownMenu close on an outside click. A click on their own
  trigger does not reopen them.
- A Tooltip opens 300 ms after the pointer enters its anchor and closes on
  leave.
- Dialogs centre in the host's visible height (`set_viewport_height`) from
  the surface top: a host that scrolls should keep the overlay's region in
  view.

## Windowed lists

A `List` past 24 rows is windowed by the core. Its rows sit at CONTENT
offsets inside the list node (paddingTop/Bottom stand in for the off-screen
rows).

- The painter puts the rows in an inner div of the list's `content_height`.
- A canvas probe reports the visible offset: the current content mask, which
  is the host scroller's clip or the list's own `overflow_y_scroll`.
- The probe calls `surface.scroll(id, offset)` and relays out only when the
  offset moves by at least 1 px.
- gpui's `list`/`uniform_list` are never used.

## Extension painters

`register_painter(kind, Box<dyn ExtensionPainter>)` covers the
`Extension` nodes whose `extension_kind` is `kind`.

- `measure(leaf, wrap, window, cx)` answers the border box (`None` = 0×0).
- `paint(PaintContext { node, visual, text_style, width, height, children,
  theme, mode, emit }, window, cx)` returns the element drawn INSIDE the
  frame div. `emit(event, payload, …)` routes through `event(index, …)` like
  a native.
- An unregistered kind paints its name as a note.
## Public API

- `host`: `HostPlugin`, `NoHost`, `ActionEvent`, `InputEvent`, `InputKind`.
- `extension`: `ExtensionPainter`, `PaintContext`, `EmitFn`.
- `view`:
  - Types: `SurfaceView`, `SurfaceViewOptions` (Default: core catalog,
    `default_theme()`, dark, `NoHost`), `PassStats`, and the focus helpers
    `focus_order` / `next_focus` / `is_focusable`.
  - Spec methods on `SurfaceView`: `new`, `apply`, `set_nested`, `set_data`,
    `set_theme`, `set_mode`, `register_painter`, `surface`, `surface_mut`,
    `stats`, `set_viewport_height`.
  - Additions: `set_width`, `index_of`, `layout_now`, `layers`, `focused`,
    `focus_order`, `focus_next`, `press`, `fire`, `escape`, `flush_field`,
    `submit_composer`, `type_into`, `field_text`.
- `measure`: `GpuiMeasure` (built by the view per pass) and unit-free
  helpers: `border_box`, `inner_wrap`, `insets`, `len_of`, `select_label`,
  `date_label`, `chart_legend`.
- `paint`: `PaintStyle`, `color::parse_hex`, `parts::part_visual` /
  `part_props`, the `markdown` parser and layout, `date`, `icons`, `natives`.

Set `EXP_UI_TRACE=1` to print one `[exponential-ui gpui] …` line per pass.
## Numbers

Measured with `cargo test -p exponential-ui-gpui --release --test render bench
-- --nocapture` on an M-series Mac while another lane was compiling. The
headless test window uses gpui's NoopTextSystem, so these isolate the core
and the painter, not CoreText shaping. The spike measured CoreText:
~1900 calls, 3.0 ms cold.

| pass | release | debug |
|---|---|---|
| bench 205 nodes, first pass of a fresh view (best of 5) | 1.5–1.8 ms · 398 measure calls · 2 upcalls (the process's first view: ~3.5 ms) | 19 ms |
| bench, steady (nothing changed) | 3–8 µs · 0 measure calls | 60 µs |
| bench, full frame (layout + element tree + paint) | ~1.0 ms | ~3 ms |

Almost all of the cold pass is the core's own rebuild, restyle and taffy
(`PassStats::layout_ns` ≈ `wall_ns`). The core alone on `FixedMeasure`
takes 1.4 ms on the same tree.

## Known divergences from the React renderer

- Line breaks follow gpui's shaper. It breaks at `/`, which CSS does not.
- Skeleton is static (no pulse). Inline code uses the body size, not CSS's
  `.925em`.
- Markdown headings share one size (the recipe's), as in React. Underscore
  emphasis never splits a word (GFM; the React regex does).
- The Slider track frame is the recipe's 6 px bar (taffy knows the height).
  The thumb overflows it vertically; React's slider root is 16 px tall.
- The Carousel `indicator` leaf is the recipe's single 8×8 dot. The painter
  centres the whole dot strip on it, overflowing its frame.
- Video and AudioPlayer are static placeholders: nothing plays.
- Images load through gpui `img()` for http(s)/file sources, with a tinted
  placeholder while loading and on failure.
- The DatePicker popup is its own month grid (Monday first, UTC "today"),
  not gpui-component's `Calendar`.
- Overlays are positioned in surface coordinates (see Overlays).
