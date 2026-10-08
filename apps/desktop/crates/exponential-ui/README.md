# `exponential-ui` — the Exponential UI core

The Rust engine behind every native Exponential UI painter (gpui, SwiftUI,
Compose), VAPP-86. It is the native half of [`@exponential-at/ui`]
(`packages/exponential-ui`): the same catalog, macro table, basic-catalog map
and built-in themes, EMBEDDED by the same generator, replaying the same
fixtures byte for byte. The mobile facade is the sibling crate
[`exponential-ui-ffi`](../exponential-ui-ffi) (UniFFI → Swift + Kotlin).

Standalone and publishable (VAPP-91): it depends on `taffy`, `serde`,
`serde_json`, `indexmap` and nothing from the Exponential app.
`cargo publish --dry-run` is clean.

## What a painter gets

```text
A2UI messages (JSON) ─▶ reducer ─▶ UiNode tree ─▶ layout tree ─▶ taffy ─▶ frames + visuals
                        (core + basic map,   (bindings resolved,   (batched measure,
                         macros, extensions)  parts, layers, lists)  ≤ 3 upcalls/pass)
```

1. **Reducer** (`reducer`, `macros`, `basic_map`, `validate`, `extension`,
   `expr`): a flat `updateComponents` list or the nested authoring form →
   ONE normalized `UiNode` tree in the core vocabulary. A2UI basic
   components are mapped through `basic-map.json`, macros expanded through
   `macros.json` (every expanded node carries `recipe {macro, part, props}`),
   extension catalogs validated (`define_extension`) and merged into the
   `CatalogView`, anything unknown becomes the `Unknown` placeholder plus an
   issue — never an error.
2. **Theme** (`theme`, `recipes`, `themes`, `geometry`): `load_theme`
   flattens an `extends` chain over the built-ins (`neutral` →
   `exponential` → `playful`, embedded RESOLVED), `resolve_recipe` answers a
   part's visuals for one mode and state set, `resolve_node_style` applies
   the precedence native recipe < node style < macro part recipe. Painters
   never read the theme (except fonts); they receive resolved `Visual`s.
3. **Layout tree** (`layout_tree`): the normalized tree becomes a flat
   pre-order list of `LNode`s. Bindings (`{path}`, client functions) resolve
   against the data model (`data`), template children instantiate per item
   (ids `<template>.<i>`), and natives with structure of their own get
   SYNTHETIC PART nodes (`<owner>.<part>`, recipe keyed on the owner, e.g.
   `Tabs/tab`, `Accordion/trigger`, `Input/field`, `Checkbox/box`) so a
   theme styles them and the painter paints chrome per part. Overlay natives
   (Dialog, Drawer, Popover, Tooltip, DropdownMenu) keep their trigger inline
   and put their content into a LAYER with its own root. Extension natives
   arrive as `component: "Extension"` leaves (`extension_kind`, `catalog_id`).
4. **Layout** (`surface`, `style`, `tracks`, `measure`, `overlay`, `list`):
   one `TaffyTree` per surface (main root + one root per open layer), styles
   resolved per pass (`@media (min-width)` against the SURFACE width,
   `:pressed` from the node's states, logical inline properties made
   physical from the root `direction`), restyled only where the flat map
   changed. Text measurement is batched (below). Layers are placed against
   the viewport (Dialog centred, Drawer on its edge) or an anchor frame with
   flip + shift (`place_overlay`, the TS rule verbatim). `List`s past 24
   rows are windowed: only the visible range plus overscan is laid out,
   row heights are memoized per item key, `scroll(list, offset)` moves the
   window in constant work.
5. **Output**: `LayoutOutput { frames, layers, lists, visual_changes, … }`
   (frames in SURFACE coordinates, pre-order = paint order = accessibility
   order; absolute nodes come last among siblings, no z-index), plus
   `nodes()` (static per structure version) and `visuals()`.

## The measure protocol (the VAPP-4 verdict)

Never one FFI call per taffy measure request. `Measure` is batched:

| phase | call | when |
|---|---|---|
| 1 | `measure_intrinsics(leaves)` → min-content width, max-content width, height at max-content | every leaf without a memo entry (new, changed props, new font scale) |
| 2 | taffy against the Rust-side memo | a definite width clamps between min and max content; a width ≥ max-content (or no wrap opportunity: min == max) needs no height call |
| 3 | `measure_heights(leaves, [(index, width)])` | only leaves whose decided width is narrower than their max-content width and unknown in the memo; the tree is re-run; a third round only when a height changed a width decision |

`LayoutOutput::upcalls` counts the crossings (≤ 3 by construction;
`measure_rounds` the taffy runs). Every `LeafRequest` carries the resolved
`TextStyle` (font size, weight, line height, family) and the `ControlBox`
(recipe padding/border/gap, fixed or minimum sizes) so Compose can pick the
font before its first pass and so a host measures the BORDER box of a
control; taffy adds no padding to a measured leaf (`style::BoxKind::Leaf`).
Memo entries survive a rebuild by node id (`archive`), so a data-model change
or a scroll never re-measures unchanged text. In-process painters (gpui)
answer per request through `DirectMeasure`. `FixedMeasure` is the
geometry-test measure (8 px per character, 20 px lines, control boxes,
explicit sizes per id). The `shaping` feature adds `ShapingMeasure`: text
shaped in-process on the system fonts with cosmic-text (no upcalls for
text at all; controls fall back to the host).

## Surface API

```rust
let mut s = Surface::new("s1", SurfaceOptions { catalog_id: CORE_CATALOG_ID.into(), ..Default::default() });
s.apply(&json!({"version": "v0.9", "updateComponents": {"surfaceId": "s1", "components": [...]}}))?;
s.apply(&json!({"version": "v0.9", "updateDataModel": {"surfaceId": "s1", "path": "/rows", "value": [...]}}))?;
s.set_viewport(390.0, 0.0, None);               // height ≤ 0 = as tall as the content; Some(max) bounds a card
let out = s.layout(&mut measure);                // frames, layers, lists, visual_changes, upcalls
let nodes = s.nodes();                           // once per out.structure_version
let events = s.event(index, "press", None);      // → Action | OpenUrl | DataChanged | Input | Relayout
s.set_states("btn", vec!["hover".into()]);       // hover / pressed / focus / disabled / checked / open / selected
s.scroll("list", 1200.0); s.set_open("dlg", true); s.set_theme(Some(builtin_theme("playful").unwrap())); s.set_mode(Mode::Dark);
```

Local UI state the core owns (never sent to the producer unless the prop is
bound): Tabs selection, Accordion open set, Carousel page, overlay `open`,
Toggle `pressed`, list scroll offsets. A bound prop writes through to the
data model and yields `OutEvent::DataChanged`. Host-owned inputs stay
host-owned: the core only forwards `OutEvent::Input` (the host adds its
revision) and writes the value through when bound.

## Fixtures (`packages/exponential-ui/fixtures`, replayed in `tests/`)

| file | test |
|---|---|
| `catalog-components/macros/basic-map/extension.json`, `kitchen-sink*.json` | `tests/catalog_fixtures.rs` (the TS test names) |
| `theme-resolved/recipes/extends/invalid.json`, `control-geometry.json` | `tests/theme_fixtures.rs` (+ the three source theme files load to the same result) |
| `layout-geometry.json` (900/390, LTR/RTL, EXACT frames), `overlay-geometry.json` | `tests/layout_fixtures.rs` (+ layers flipping at the four edges, a 10,000-row list, the A2UI message flow, extension leaves, a theme switch) |
| the generated constants | `tests/generated_drift.rs` |

`src/generated/{catalog,themes}.rs` are COPIES written by
`bun run --filter @exponential-at/ui generate` (the package's
`generate.test.ts` gates them); never edit them.

## Numbers (release, M-series Mac, 205 layout nodes)

| pass | time |
|---|---|
| Rust only, `FixedMeasure`, warm width change | ~0.56 ms |
| through the Swift binding incl. `nodes()` marshalling | ~1.5 ms |
| through the Kotlin/JNA binding on the JVM incl. `nodes()` | ~0.8 ms |

The Android emulator and iPhone figures are recorded by the painter runs
(VAPP-88/89) that build the device libraries.
