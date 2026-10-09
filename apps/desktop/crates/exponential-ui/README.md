# `exponential-ui` — the Exponential UI core

The Rust engine behind every native Exponential UI painter (gpui, SwiftUI,
Compose), VAPP-86. It is the native half of [`@exponential-at/ui`]
(`packages/exponential-ui`): the same catalog, macro table, basic-catalog map
and built-in themes, EMBEDDED by the same generator, replaying the same
fixtures byte for byte. The mobile facade is the sibling crate
[`exponential-ui-ffi`](../exponential-ui-ffi) (UniFFI → Swift + Kotlin).

Standalone and publishable (VAPP-91): it depends on `taffy`, `serde`,
`serde_json` (with `preserve_order`: JSON objects keep source order like the
TS reference) and `indexmap`, nothing from the Exponential app.
`cargo publish --dry-run` is clean.

## What a painter gets

```text
A2UI messages (JSON) ─▶ reducer ─▶ UiNode tree ─▶ bind pass + layout tree ─▶ engine (taffy) ─▶ frames + visuals
                        (core + basic map,   (bindings, calls,        (stable slots, parts,   (batched measure,
                         macros, extensions)  $string, visible)        layers, lists)          baselines, ≤ 3 upcalls)
```

1. **Reducer** (`reducer`, `macros`, `basic_map`, `validate`, `extension`,
   `expr`): a flat `updateComponents` list or the nested authoring form →
   ONE normalized `UiNode` tree in the core vocabulary. Macros expand
   through `macros.json`; a macro input that is BOUND (`{path}` or a call)
   is never evaluated at expansion: the expander emits a core-function call
   (`percent`, `concat`, `fallback`, `cond`, `eq`, …) the bind pass
   evaluates, a bound `$if`/`$any` becomes the part's `visible`, `$set`
   adds a `set` write-back to a part's action (two-way binding), responsive
   props (`{base, sm?, md?, lg?, xl?}`) become `@media (min-width:
   $breakpoint.*)` blocks. `visible`, template `key`, `accessibility` and
   slot names (`*` = any) are reduced and validated.
2. **Bind pass** (`data`): `bind_tree` / `resolve_value` resolve bindings
   (absolute, or relative to the template item), the core + basic function
   table and `$string.<id>` built-in copy (`strings`) at ANY depth in props,
   styles and recipe props; a falsy `visible` drops the node and its
   subtree. `run_action`: the action's context and function args resolve
   against the data AS IT IS, then `set` writes, then the event goes out.
   `fixtures/bind-time.json` locks both.
3. **Theme** (`theme`, `recipes`, `themes`, `geometry`): `load_theme`
   flattens an `extends` chain over the built-ins (`neutral` →
   `exponential` → `playful`), `resolve_recipe` answers a part's visuals for
   one mode and state set. Round 1: `breakpoint` (px), `ease` (cubic bezier)
   and `density` token groups, `contrast` overlays, `resolve_mode`
   (`system`), `apply_density`, `apply_contrast`, `resolve_condition_key`.
4. **Conditions** (`conditions`): the one media grammar (min/max width and
   height in px or `$breakpoint.*`, `orientation`, `hover`,
   `prefers-reduced-motion`) and the state blocks `:hover`,
   `:focus-visible`, `:pressed`, flattened in SOURCE order then the states
   (`resolve_conditions`, `fixtures/style-conditions.json`).
5. **Layout tree** (`layout_tree`): the bound tree becomes layout nodes;
   natives with structure get SYNTHETIC PARTS (`<owner>.<part>`, the
   owner's recipe for the part): Tabs, Accordion, Carousel, List (windowed
   past 24 rows, spacers stand in for the rows outside the window), Table
   (header cells, rows, slot cells with the ROW as their data scope,
   sort, selection, windowing past 50), the form controls (label / field /
   description / `<id>.error`), NumberField (steppers + host `input`),
   ChipInput (chips + host `input` + a suggestions layer), FileUpload
   (drop zone + file rows), CodeBlock (header, one row per line with the
   tokenizer's tokens), Chart (one leaf carrying `extent`, `nice_ticks`,
   colour tokens), Form (`summary`, failing fields `invalid`). Every popup
   is a LAYER with its own root: Dialog/Drawer (scrolling `body`), Popover,
   Tooltip, a hover Popover (open on hover / keyboard focus of the trigger, held
   open while the pointer is on the trigger OR anywhere in its content;
   leaving both closes at once, or — `settings.hover_close_ms > 0`, 150
   like the web — raises `OutEvent::HoverTimer {owner, delay_ms}` and
   closes on `hover_timeout(owner)` unless the pointer came back),
   Menu (checkbox / label / separator entries, one level of submenus
   whose rows may be a bound `{path}`; `openOn: press` = its ONE child is
   the trigger, no child = a default outline Button; `openOn: contextmenu`
   = the child is the target region and the menu opens at the pointer), the
   Select combobox
   (search + options), the DatePicker/DateRangePicker calendar (week start
   by locale), the TimePicker list and the Toast band.
6. **Surface** (`surface`): nodes live in STABLE SLOTS keyed by id. A
   rebuild (a data write, an opened overlay) RECONCILES by id: unchanged
   nodes keep their slot, style, visual and measurement; the change goes
   out as a `NodeDelta` (`added`, `removed`, `changed`, `renumbered` after a
   compaction). A scrolled window and LOCAL state (a tab, an accordion
   section, a carousel page, a toggle, a copy button) rebuild only that
   node's SUBTREE (`LayoutOutput::built_nodes` says how many). Restyle is PER NODE: a state
   change restyles that node, a viewport change only nodes with conditions,
   a theme / mode / settings change everything. Settings: locale + strings,
   mode `light|dark|system`, density, contrast, font scale, safe-area
   insets, pointer hover, reduced motion.
7. **Engine** (`engine`): taffy's flexbox/grid/block over the surface's own
   slots (taffy's custom-tree API): measured leaves answer their size AND
   first baseline (`alignItems: baseline` aligns text by its first line),
   `mark_dirty` clears a node and its ancestors only, `content_size` feeds
   scroll containers.
8. **Output**: `LayoutOutput { frames, layers, lists, scrolls, toasts,
   visual_changes, delta, direction, breakpoint, upcalls, restyled,
   rebuilt, … }` (frames in SURFACE coordinates, UNSCROLLED, in paint order =
   accessibility order), plus `nodes()` (every slot; removed ones as
   tombstones so `nodes()[i].index == i`), `nodes_at(indices)` and
   `visual(i)`. The painter never reads the catalog or the theme.

## The measure protocol (the VAPP-4 verdict)

Never one FFI call per taffy measure request. `Measure` is batched:

| phase | call | when |
|---|---|---|
| 1 | `measure_intrinsics(leaves)` → min-content width, max-content width, height at max-content, FIRST BASELINE | every leaf without a memo entry (new, changed props, new font scale) |
| 2 | the engine against the Rust-side memo | a definite width clamps between min and max content; a width ≥ max-content (or no wrap opportunity) needs no height call |
| 3 | `measure_heights(leaves, [(index, width)])` | only leaves whose decided width is narrower than their max-content width and unknown in the memo; a third round only when a height changed a width decision |

`LayoutOutput::upcalls` counts the crossings (≤ 3 by construction). Every
`LeafRequest` carries the resolved `TextStyle` (font size × font scale,
weight, line height, family, and the width-changing `fontStyle`,
`textTransform`, `letterSpacing`), the part's `owner_component` and the
`ControlBox` (padding on EVERY side,
logical keys resolved; `padding_horizontal/vertical` are the averages, so
`2 ×` gives the total; border, gap, fixed or minimum sizes). `baseline:
None` = the bottom edge stands in (CSS). Memo entries survive a rebuild by
slot and by id (`archive`), so a data change or a scroll never re-measures
unchanged text.

- In-process painters (gpui) answer per request through `DirectMeasure`
  (`DirectMeasureWithBaseline` adds baselines).
- A host that must not be called back while the surface is locked (the
  FFI) uses the same pass as STEPS: `layout_begin(measure_id)` →
  `LayoutStep::Intrinsics(leaves)` / `Heights(leaves, requests)` answered by
  `layout_intrinsics` / `layout_heights` → `Done(output)`.
- `FixedMeasure` is the geometry-test measure (8 px per character on one
  20 px line, a 15 px ascent, control boxes, explicit sizes per id; host
  text fields — `input` / `search` parts — one line of text or
  placeholder).
- The `shaping` feature adds `ShapingMeasure`: text shaped in-process on the
  system fonts with cosmic-text (baselines from the first layout run).

## Surface API

```rust
let mut s = Surface::new("s1", SurfaceOptions { catalog_id: CORE_CATALOG_ID.into(), ..Default::default() });
s.apply(&json!({"version": "v0.9", "updateComponents": {"surfaceId": "s1", "components": [...]}}))?;
s.apply(&json!({"version": "v0.9", "updateDataModel": {"surfaceId": "s1", "path": "/rows", "value": [...]}}))?;
s.set_viewport(390.0, 844.0, None);            // height ≤ 0 = as tall as the content; Some(max) bounds a card
s.set_insets(Insets { top: 47.0, bottom: 34.0, ..Default::default() });
s.set_mode_setting(ModeSetting::System, true); // or set_settings(SurfaceSettings { locale, strings, density, contrast, font_scale, hover, reduced_motion, today, .. })
let out = s.layout(&mut measure);              // frames, layers, lists, scrolls, toasts, delta, upcalls
let nodes = s.nodes();                         // once; then patch: s.nodes_at(&[out.delta.added, out.delta.changed].concat())
let events = s.event(index, "press", None);    // → Action | OpenUrl | Call | DataChanged | Input | Focus | Announce | Copy | PickFiles | Relayout | HoverTimer
                                               // (a disabled / loading control, or one inside a disabled Form, answers nothing)
s.set_states("btn", vec!["hover".into()]);     // hover / pressed / focus / focus-visible / disabled …; restyles that node
s.scroll("list", 1200.0); s.scroll_to("strip", 300.0, 0.0);
s.set_open("dlg", true); s.dismiss_toast("t1"); s.submit_form("signup");
s.command(&SurfaceCommand::ScrollIntoView { id: "row-400".into() });
let late = s.take_events();                    // a hover opening a tooltip, a hover-close timer, a live region, a re-clamped window's Relayout
s.hover_timeout("hc");                         // (hover_close_ms > 0) a HoverTimer fired: closes unless the trigger or content is hovered again
```

Local UI state the core owns (never sent to the producer unless the prop is
bound): Tabs selection, Accordion open set, Carousel page, overlay and popup
`open`, Toggle `pressed`, scroll offsets (any scroll container), a Select's
search, a calendar's month and range anchor, a menu's open submenu, the
context-menu point, the host's latest field values, form errors, unbound
Table sort/selection, ChipInput values, FileUpload files, CodeBlock
`copied`. A bound prop writes through to the data model and yields
`OutEvent::DataChanged`; an `Action`'s context = the author's context with
the event payload merged over it (`{value}`, `{checked}`, `{open}`, `{sort}`,
`{selected}`, `{values}`, `{files}`, `{errors}` …, contract §3).

Events resolve against the SOURCE of the node they hit, at any depth: a
control inside a template row or a Table slot cell writes its row's
binding and fires its own `on` with the row (a literal Table row included)
as its data scope. Template item ids follow the reference's
`templateItems` (`.<key>`, `.#<index>` for a missing/empty/duplicate key,
`.<index>` without a `key`), so they never collide. NumberField rounds
like the reference: `precision` (else the step's decimals) via `toFixed`
for the value, the shown text like `Intl.NumberFormat` (`en`, grouped); a
stepper on an unset value starts from `min` when it is above 0, else 0.
Checks fail when their condition resolves to nothing, `null`, `false` or
`""` (`failingChecks`). The unbound Table sort is the reference's
`sortRows` (typed columns, a numeric, case- and accent-insensitive
collator stand-in). `direction` inherits per node (logical keys,
`textAlign: start|end` and flex rows follow the nearest `direction`).

Scroll containers: frames are UNSCROLLED; each `ScrollOutput` gives the
container's clamped offset (`scroll_to` clamps to the last pass's content,
and a clamped offset becomes the state, re-windowing a list) and content size — translate its descendants by
`-offset` and clip to its frame. A Dialog or Drawer taller than the
viewport scrolls its `body`. Layers stack base < overlay < toast (no
z-index) and stay inside the safe-area insets; anchored popups flip and
shift (`place_overlay`), `start`/`end` alignment and submenus follow the
direction.

## Round 2 (`docs/round-2-contract.md`)

| area | API |
|---|---|
| templates | `ReduceResult::templates`: template nodes are LIFTED out of the tree; items instantiate them. Keys `#<index>` for missing / empty / duplicate (`list::template_item_keys`, `table_row_keys`); instance suffixes accumulate (`issue.title.ops.1`; `.`/`~` in keys escape as `~1`/`~0`); a template that would instantiate itself stays in place with an issue |
| props | layout resolves props along their schema (`data::resolve_node_props`: a literal Table `rows` holding `{path}` is data); a bound number or boolean in a text prop shows its `format::display_string` |
| formatting | `format::Formatter` (number, currency, percent, date, relative time, plural, `bytes`); `EnglishFormatter` default (UTC), `ZonedEnglishFormatter::new(offset)` = the fallback at the host's UTC offset per instant; `Surface::set_formatter`, `set_clock`, `tick`, `uses_clock` (tick once a minute while true); the zone reaches the core ONLY through the formatter (no zone database, no `time_zone` setting). The core parses and decides; the formatter localizes. Table cells, NumberField (display and typed text: `format::parse_number`), chart `tickLabels` + `valueLabels`, picker text, calendar names and day labels, FileUpload sizes go through it |
| style | `position: sticky` (`LayoutOutput::sticky` = `{index, dx, dy}` paint offsets inside the nearest scroller, else the host viewport), `backdropBlur` (`Visual::backdrop_blur`), `animation` + `animationDuration` (`Visual::animation` = name + resolved timing; sample with `animation::frame_with_timing`), `direction` on any node (`Visual::direction` + physical `text_align` on leaves) |
| Resizable | parts `panel` / `handle` / `grip`; `event(handle, "drag", {phase: start\|move\|end, delta})` from the start sizes, `event(handle, "key", {key})` (`resizable::RESIZE_KEYS`; others are ignored); a bound `sizes` writes and `change {sizes}` fires on a drag end or key that changed the sizes. Arithmetic: `resizable::*` |
| lists | `direction: horizontal` windows on x; `divided` = gap + hairline at every boundary (`<list>.divider.<item>` between items); `sectionBy` + slot `section` (`<list>.section.<i>`, a level-3 heading) + `stickyHeaders` (`list::scroll_offset_for_item`); an unbounded list windows against its scrolling ancestor or the host viewport (`Surface::set_surface_scroll`); `scroll_to_index(id, index, align)` / `SurfaceCommand::ScrollToIndex` (the host viewport case emits `OutEvent::ScrollSurface`); window past 50 items |
| strings | `$string.invalidValue`, `dialog`, `codeBlock`, `table`, `carousel` / `slide`, `resize` on the nodes' `accessibility` |

## Round 3 (`docs/round-3-contract.md`, VAPP-102)

| area | API |
|---|---|
| vocabulary | natives `Segmented` (ToggleGroup renamed; `variant` segmented \| toggles \| outline \| `bar`, content-sized unless `fill` or `bar`) and `Menu` (DropdownMenu + ContextMenu, `openOn` press \| contextmenu, glyphs `Menu.check` / `Menu.submenuIndicator`); macros `Row`, `Section`, `Chip` |
| removed names | round 4 (VAPP-103) removed the folded names outright: no aliases; `is_offered()` = not hidden (`catalog::component_names` lists only offered ones) |
| tree guides | `tree_guides::tree_guides(depths)` (the ×4 app rule) and `apply_tree_guides(root)`, run at the end of `macros::expand_macros`: every Row root's `guides` part (a hidden `TreeGuides`) gets `{depth, elbowAt?, tee, passThrough}` from its siblings; `layout::TREE_GUIDE_COLUMN` 14, `TREE_GUIDE_RADIUS` 3, `TREE_GUIDE_BRIDGE` 1 (paint only) |
| bindable submenus | `menuItem.items` resolves along its schema: a `{path}` there becomes the submenu's rows |

`tests/round3_fixtures.rs` replays `tree-guides.json` and the `Row/tree:*`
macro cases; `tests/round3_layout.rs` covers the Menu triggers, a bound
submenu and Segmented sizing.

## Host API (`host`, VAPP-91)

The pure half of the host API, JSON-equal to the TS reference
(`packages/exponential-ui/src/host`): `HostRouter` (messages → ops),
`JsonlDecoder` / `SseDecoder` / `messages_from_mcp_result` /
`mcp_action_call`, `decide_function` / `combine_decisions` /
`package_policy`, `decide_url`, `media_request`, `parse_source`,
`supported_catalog_ids` / `client_capabilities`, `validate_package` /
`template_messages`, `action_message` / `error_message`; `contract.rs`
mirrors `catalog/host.json` (drift-tested). An `on.<event>`
`{functionCall: …}` (or the legacy `function`) to a non-built-in name yields
`OutEvent::FunctionCall { component_id, name, args }` (args resolved); the
facade exposes all of it as JSON-string functions and a `HostRouter` object.
`tests/conformance.rs` runs the whole conformance suite against the core.

## Fixtures (`packages/exponential-ui/fixtures`, replayed in `tests/`)

| file | test |
|---|---|
| `catalog-components/macros/basic-map/extension.json`, `kitchen-sink*.json` | `tests/catalog_fixtures.rs` (the TS test names; 189 macro cases incl. `bound:*` / `responsive:*`) |
| `bind-time.json`, `style-conditions.json`, `code-tokens.json` | `tests/round1_fixtures.rs` |
| `format`, `template-items`, `text-direction`, `resizable`, `virtual-list`, `animations.json`, `bench-list.json` | `tests/round2_fixtures.rs` (cases in `tests/support/round2.rs`, shared with the conformance runner); `tests/round2_surface.rs` covers the surface side |
| `theme-resolved/recipes/extends/invalid.json`, `control-geometry.json` | `tests/theme_fixtures.rs` |
| `layout-geometry.json` (900/390, LTR/RTL, EXACT frames), `overlay-geometry.json` | `tests/layout_fixtures.rs` (+ layers flipping at the four edges, a 10,000-row list, the A2UI message flow, extension leaves, a theme switch) |
| `layout-geometry-round1.json` (WRITTEN here: the kitchen sink's responsive section at 390/600/700/768/900/1280, LTR/RTL, theme neutral, fixed measures per case; replayed by gpui, not yet by React) | `tests/round1_geometry.rs` (`EXP_UI_WRITE_FIXTURES=1` regenerates) |
| `interactions-round1.json` (HAND-AUTHORED: NumberField stepping/rounding/text, check semantics, the unbound Table sort; the React half is `interactions-fixture.test.tsx`) | `tests/interactions_fixture.rs` |
| the generated constants | `tests/generated_drift.rs` |
| `host-transport/policy/router.json` | `tests/host_fixtures.rs` |

`tests/round1_layout.rs` covers the rest of round 1 end to end (bound
macros at layout time, `visible`, baselines, per-side padding, `%` radii,
incremental restyle and re-windowing, settings, scroll containers, popups,
every new native, forms, toasts, menus, RTL, responsive natives, strings,
commands, template keys, the stepped pass); `tests/review_fixes.rs` the
round-1 review (nested controls in rows and slot cells, subtree rebuilds in
Forms, scroll clamping, hover cards, inert disabled controls, template key
collisions, per-node direction, `%` font sizes, snapshots mid-pass).

`src/generated/{catalog,themes}.rs` are COPIES written by
`bun run --filter @exponential-at/ui generate` (the package's
`generate.test.ts` gates them); never edit them.

## Numbers

Release build, x86-64 Linux desktop, `FixedMeasure` (`cargo run --release -p exponential-ui --example bench`):

| pass | time |
|---|---|
| 205-node card grid, `FixedMeasure`, warm width change (390 ↔ 900) | ~0.13 ms |
| 2,000-row windowed list (215 live nodes), one scroll step (re-window, ~40 nodes restyled, 2 upcalls) | ~1.0 ms |
| the same list, a hover (one node restyled, no rebuild, no upcall) | ~0.05 ms |
| `bench-list.json`: 100,000 `ListRow`s, 390×800, neutral: first paint (data + reduce + bind + layout) | ~70 ms |
| the same list: one viewport scroll step (mean of 100) | ~3.2 ms |
| the same list: `scrollToIndex(50000, start)` + layout | ~5.4 ms |
| the same list: items alive after the jump | 30 |
| through the Kotlin/JNA binding on the JVM incl. `nodes()` (205 nodes) | ~0.4 ms (layout alone ~0.16 ms) |

The Android emulator and iPhone figures are recorded by the painter runs
(VAPP-88/89) that build the device libraries.
