# genUI: round 8 contract

Round 8 adds four things: a `Canvas` primitive (vector drawings on Motion's
engine, with native text), Checkbox `indeterminate`, Progress `valueText` +
`detail`, and Stepper `collapse`. Each `##` section lists what moved, what
locks it and the per-renderer checklist. Phase 0 (contract + shared core) is
done; the painters are the platform lanes' work.

## 1. `Canvas`

A static vector drawing (charts, plots, diagrams, illustrations). Interactive
UI and text stay native; the drawing goes through eui-motion's display list,
replayed natively like every Motion. Webviews are out.

| area | change | source of truth | locked by |
|---|---|---|---|
| catalog | `Canvas` (`native`, `media`, lite, no children). Props below; events `press`, `move`, `leave` | `core.catalog.json` | `catalog-components.json`, `specimens.json`, `kitchen-sink.json` |
| engine | `canvas::Canvas::compile` → a one-frame Motion; `render(slots)` → the display list + `text` ops | `apps/desktop/crates/eui-motion/src/canvas.rs` | `fixtures/canvas.json` (`tests/canvas.rs`) |
| display list | new `Op::Text`, packed VERSION 2 | `display.rs`, eui-motion README › Packed frames | `canvas.json`, `motion-frames.json` (version word) |
| limits | `limits.json` › `motion.canvas`: `maxOps` 4000, `maxTexts` 500, `maxTextChars` 1000 | `limits.json` | `canvas.json` `refuse`, `tests/canvas.rs` |
| facades | WASM `CanvasPlayer` (constructor = validation, `slotIds()` = a JSON string, `sample(slotsJson)`); uniffi `CanvasPlayer` (`new`, `size`, `slot_ids`, `sample`) + `canvas_to_drawing` | `eui-motion-wasm`, `exponential-ui-ffi/src/motion.rs` | `src/canvas.test.ts`, `exponential-ui-ffi/tests/facade.rs` |
| TS core | `canvasFitMatrix`, `canvasToDrawing`, `canvasBoxStyle`, `canvasDeclaredSize`, `canvasA11y`, `canvasFit`; server `canvasPropsProblems` | `src/canvas.ts`, `src/motion/server.ts` | `src/canvas.test.ts` |
| Rust core | box = the Motion box; measure; `press`/`move`/`leave` fire with the painter's `{x, y}` only with `pointer: true` | `layout_tree/mod.rs`, `measure.rs`, `surface/events.rs` | `eui-core/tests/round8.rs` |
| a11y | `img` named by `label`, hidden without one; `pointer` adds no role or focus stop | `a11y.json` | `round1.test.ts` |
| recipes | `Canvas` parts `root`, prop `fit` | `recipes.json` | `theme.test.ts` |

Props:

| prop | type | default | meaning |
|---|---|---|---|
| `size` | `[w, h]` numbers, required | | the drawing's coordinate space, origin top-left |
| `shapes` | object array, required, bindable | | Motion's shape vocabulary (static) + `text` |
| `fit` | `motionFit` | `contain` | how the drawing fills the box |
| `label` | string | | accessible name (role img) |
| `pointer` | boolean | false | emit `press`/`move`/`leave`; false = inert like an image |
| `style` | style | | |

Shapes: `rect {position (centre), size, radius}`, `ellipse {position, size}`,
`star` / `polygon`, `path {d}`; `fill`, `stroke`, `gradient-fill`; `trim`,
`repeater`, `round-corners`; `group {shapes, transform}`; colours `$color.*`,
`#hex`, `[r, g, b, a]`, `{light, dark}`. Anything else (`merge`,
`gradient-stroke`, raw `lottie` items or keys) is refused
(`canvas-shape-type`, `canvas-lottie`); any `{keys}` is refused
(`canvas-animated`: "Canvas values are static; use Motion to animate").

`text {text, position [x, y] = [0, 0], size = 14, weight = regular |
medium | semibold | bold, align = start | center | end, baseline = alphabetic
| top | middle, font = sans | mono, color = $color.foreground}`. Line breaks
become spaces (one line, never wrapped). Inside groups it follows the group
transforms and opacity; modifiers never touch it. Unknown fields are ignored
(as the dialect ignores unknown keys).

Rendering:

- The display list is the Motion one (`fixtures/motion-frames.json`
  documents every op). Every `text` op comes AFTER every other op (after the
  frame's final `pop`), in source order.
- `Op::Text {transform, text, font, weight, size, align, baseline, color}`:
  the anchor is the op's local origin. Map `(0, 0)` through fit × transform;
  set one line at font px `size` (scaled by the transform) in the theme's
  `$type.family.sans` / `$type.family.mono` at the CSS weight (400/500/600/700).
  `align`: start = the line's left edge at x, center = its middle, end = its
  right edge (physical; drawings never mirror in rtl). `baseline`: top =
  baseline at y + ascent, middle = baseline at y + (ascent − descent) / 2,
  alphabetic = y. Ascent and descent = the font's own (hhea) ascender and
  descender magnitudes at that size (what CTFont / Paint.FontMetrics /
  `measureText().fontBoundingBoxAscent/Descent` / gpui font metrics report);
  no leading, no line height. The colour is straight RGBA with the groups'
  opacity already in alpha.
- Packed text: `7, t[6], font (0 sans | 1 mono), weight (400|500|600|700),
  size, align (0|1|2), baseline (0 top | 1 middle | 2 alphabetic), r, g, b, a,
  n, n UTF-16 code units`.
- Text is never outlined by the engine; the tiny-skia raster skips it. gpui:
  raster the shapes, then draw native text on top.
- Colours: exactly a Motion's slot contract. `slotIds()` lists every
  `$color.<token>` slot (text colours included) and `ld:<light>:<dark>` pairs;
  hosts fill them with `motionThemeSlots` (web) / `theme_slots` (gpui) / the
  natives' equivalent, plus `$mode: dark`. Recompile on `size`/`shapes`
  change, resample on theme/mode change. No playback: one frame.
- Fit: contain = uniform min, centred; cover = uniform max, centred, clipped by
  the box; fill = stretched from the top-left. Shapes are clipped to the
  composition rect (the frame's leading `clip`); text only by the box.
- Pointer: with `pointer: true`, `press {x, y}`, `move {x, y}` (at most once
  per frame while hovered or dragging) and `leave` (no payload), x/y in
  drawing units = the fit mapping inverted, two decimals; letterbox points map
  outside `0..size`, never clamped.

Layout (every measurer): the Motion box. Intrinsic = `size`; unstyled =
`width: w`, `maxWidth: 100%`, `aspectRatio: w/h`; a style width or height
wins and the aspect fills the other edge; both = no aspect; no baseline
(`layout.json` $comment).

Renderers:

- [x] Engine, WASM, uniffi, Rust core, TS core (phase 0).
- [ ] Web (React, Solid, element): `web/primitives/motion-paint.ts` accepts
  VERSION 2 and paints `text` ops (Canvas2D `fillText`, the theme font, the
  baseline rule above); a Canvas native over `CanvasPlayer`.
- [ ] SwiftUI, Compose: a Canvas leaf over the uniffi `CanvasPlayer`; the
  packed parser accepts op 7; the measurers answer `size`.
- [ ] gpui: a Canvas leaf (raster + native text), `canvas::to_drawing`.

## 2. Checkbox `indeterminate`

| area | change | source of truth | locked by |
|---|---|---|---|
| prop | `indeterminate` boolean, default false, bindable | `core.catalog.json` | `catalog-components.json` |
| look | the checked box with a dash: builtin icon `Checkbox.check.indeterminate` = `ui-minus` (instead of `Checkbox.check` = `ui-check`); recipes when-prop `indeterminate` (neutral: the checked fill) | `core.catalog.json` `builtinIcons`, `recipes.json`, `neutral.theme.json` | `theme-recipes.json` |
| a11y | checked state MIXED (aria-checked=mixed, SwiftUI value "mixed", Compose `ToggleableState.Indeterminate`, AccessKit `Toggled::Mixed`), mixed wins over `checked` | `a11y.json` | |
| press | mixed → `checked: true`, `indeterminate: false`, both written (bound paths; unbound = local state); plain → toggles, `indeterminate` untouched; `change` = `{checked, indeterminate: false}` | `src/checkbox.ts`, `eui-core surface/events.rs` | `interactions-round1.json` `checkbox` (Rust `interactions_fixture.rs`, TS `src/canvas.test.ts`) |
| core | the `box` part's props carry `indeterminate` | `layout_tree/fields.rs` | |

## 3. Progress `valueText` + `detail`

`valueText` (string, bindable): trailing caption text at the end of the label
row; the row (`header`) shows when `label` or `valueText` is set. `detail`
(string, bindable): a caption line under the track, also the bar's
accessible description. Parts `header` (row, baseline-aligned, gap sm),
`label` (grows), `value` (no shrink, `marginInlineStart: auto`; painters set
tabular numerals), `detail`. Neutral recipes colour `value` and `detail`
mutedForeground. NOTE: `label` moved under `header` for every Progress with a
label. Locked by `catalog-macros.json`, `bind-time.json`
(`Progress/bound:valueText+detail`), `specimens.json`.

## 4. Stepper `collapse` + the `@container` condition

`collapse` (`stepperCollapse`: never | auto | always, default never) through
macro `alternatives` (literal values only):

- `never`: unchanged.
- `always`: a `list` root holding one `line` Text per step (`$each`, `$if`
  current), `$string.stepCompact` = "Step {step} of {total} · {label}" (step =
  current + 1); none shows when `current` is past the last step.
- `auto`: a column root holding `full` (the never-Stepper, styled
  `"@container (max-width: $breakpoint.sm)": {display: none}`) and `compact`
  (that list, `display: none` unless the same condition holds).

The new style condition `@container (min-width | max-width: N)` (N = px or
`$breakpoint.<name>`; `style.json` grammar) matches the width of the node's
PARENT content box after layout, not the surface; in source order with the
`@media` blocks; before the parent has a width it never matches. Rust core:
`conditions.rs` + `surface/pass.rs refit_containers` (lay out, read each
parent width, restyle, lay out again; at most twice per pass), so SwiftUI,
Compose and gpui get it for free. TS: `resolveConditions` with
`containerWidth`. Web: CSS `@container (width < Npx)` with the parent as an
`inline-size` container (`web/box-css.ts`, the web lane). Locked by
`style-conditions.json` (the container case), `catalog-macros.json`,
`bind-time.json` (`Stepper/collapse-always:bound-current`),
`eui-core/tests/round8.rs`.

## Prompt

+180 tokens: the measured 9,733, under the 10,000-token ceiling (no named exception).
