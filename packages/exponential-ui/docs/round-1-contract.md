# Exponential UI — round 1 contract (renderer hardening)

> **Round 4 (VAPP-103):** this is the round's historical record. Names it
> uses that round 3 folded (ToggleGroup, TabBar, ButtonGroup, DropdownMenu,
> ContextMenu, Sheet, HoverCard, Pill, EntityChip, Band, RowList and the
> ListRow family) are REMOVED from the catalog; `round-3-contract.md` §4 maps
> each to what replaces it.

Round 1 makes one A2UI surface render faithfully, responsively and
accessibly on web, desktop, iOS and Android. This file is the CONTRACT half:
what changed in `@exponential-at/ui` (catalog, macros, style, tokens,
themes, the TS reference, fixtures, generated constants) and what each
renderer (React, the Rust core, gpui, SwiftUI, Compose) must implement to
match. Everything below is locked by a fixture or a test in this package;
the section numbers are the ones the catalog `$comment`s cite.

Reading order for a renderer lane: §0 (what moved), then the section for
the area you own, then §9 (your checklist), §10 (breaking changes).

## 0. What moved, at a glance

| area | change | source of truth | locked by |
|---|---|---|---|
| bound macro inputs (audit bug A) | the expander EMITS function calls for bound inputs; renderers evaluate at bind time | `catalog/macros.json` `$comment`, `src/expr.ts`, `src/macros.ts`, `src/dynamic.ts` | `fixtures/catalog-macros.json` (`bound:*` cases), `fixtures/bind-time.json` |
| two-way binding for macros | `$set` → `functionCall: {call: "set"}` on the part's action | `catalog/macros.json`, `src/macros.ts`, `src/dynamic.ts runAction` | `bind-time.json` `presses` |
| core functions | `percent add sub eq lt clamp cond fallback concat coalesce text map len fill` + action `set`; the BIND table adds the basic `and or not required` (`bindFunctionNames`) | `core.catalog.json` `functions.core`, `src/expr.ts CORE_FUNCTIONS`, `src/dynamic.ts BIND_FUNCTIONS` | `expr.test.ts`, `bind-time.json`, `round1-fixes.test.ts` |
| style conditions | max-width, min/max-height, orientation, hover, prefers-reduced-motion, `$breakpoint.*`; states `:hover` `:focus-visible` `:pressed` | `catalog/style.json` `conditions`, `src/style.ts resolveConditions` | `fixtures/style-conditions.json` |
| style keys | 23 new keys (transition, transform, gradient, per-side widths, per-corner radii, text styling, overflowX/Y, visibility, pointerEvents, userSelect, cursor, …) | `catalog/style.json` | `style.test.ts`, schema |
| responsive props | `{base, sm?, md?, lg?, xl?}` on `responsive` props | `core.catalog.json`, `src/macros.ts` | `catalog-macros.json` (`responsive:*`), kitchen sink |
| catalog | 21 new components, Table macro → native, new props everywhere | `core.catalog.json`, `macros.json`, `recipes.json` | `catalog-components.json`, `catalog-macros.json` |
| built-in strings | `$string.<id>`, host-overridable table | `catalog/strings.json`, `src/strings.ts` | `bind-time.json`, `round1.test.ts` |
| locale | week start, likely region (CLDR), language aliases, RTL, mirrored glyphs | `catalog/locale.json`, `src/locale.ts` | `round1.test.ts`, `round1-fixes.test.ts` |
| CodeBlock tokenizer | one scanner, 15 languages | `catalog/code.json`, `src/code.ts` | `fixtures/code-tokens.json` |
| chart numbers | extents, nice ticks, colour order, donut hole | `src/chart.ts` | `round1.test.ts` |
| accessibility | machine-readable `roles`; role + notes + keyboard per component; macro parts carry role/states/name (`$a11y` → `accessibility`); rules, host commands | `catalog/a11y.json`, `catalog/macros.json`, `src/a11y.ts`, `src/macros.ts` | `round1.test.ts`, `round1-fixes.test.ts`, `catalog-macros.json` |
| bind pass | per-ROW Table slot cells, DATA props verbatim, bound accessible names | `src/dynamic.ts` | `bind-time.json` (`extra`, `rowSlots`) |
| theming | breakpoint/ease/density tokens, chart6–8, shadow xl, contrast overlays, `system` mode, recipe motion | `catalog/tokens.json`, `themes/*`, `src/theme.ts` | `theme-*.json`, `control-geometry.json` |
| prompt | teaches responsive rules, states, `visible`, keys; common props described once | `src/prompt.ts` | `prompt-budget.json` (6451 / 6500 tokens) |

## 1. Bound macro inputs and two-way binding

### The bug
Macros expand in the reducer before any data exists. `expr.ts` used to
compute `$percent`, string interpolation, `$cond`, `$eq`, `$add` on the
`{path}` OBJECT: Progress `value: {path}` drew `width: "0%"`, Pagination
`page: {path}` printed `[object Object] / 5`, Collapsible `open: {path}` was
always open (a binding object is truthy) and never wrote back, a bound Pill
`label` passed only by luck.

### The design (chosen: emit, never re-expand)
Re-expanding on every data change would make expansion a function of data
and force every renderer to re-run the macro table per frame. Instead, when
ANY input of a template expression is DYNAMIC (an A2UI data binding
`{path}` or a function call `{call, args}`), the expander emits an A2UI
function call over the CORE functions; the renderer evaluates it at bind
time exactly like an authored call. Expansion stays a pure function of the
surface, so the TS reference and the Rust core stay line-by-line mirrors.

| template form | literal inputs | a dynamic input emits |
|---|---|---|
| `"{props.x}"` (whole string) as a MEMBER of props/style/context/`$recipe` or an array item | the value | the dynamic value itself (two-way binding survives) |
| `"{props.x\|d}"` as a member | value, else `d` | the dynamic value itself (the fallback is dropped) |
| `"{props.x\|d}"` as an OPERAND (inside any `$`-object, `$cond`/`$map` results included) | value, else `d` | `fallback{value, default: d}` |
| `"a {props.x} b"` | the joined string | `concat{values: ["a ", x, " b"]}` (empty pieces dropped) |
| `"{!props.x}"` | boolean | `not{value}` |
| `"{len(props.x)}"` | length | `len{value}` |
| `"{range(props.x)}"` (for `$each`) | `[0..n-1]` | nothing (no items) |
| `$map {from, cases, default}` | the case | `map{value, cases, default}` |
| `$cond [c, then, else]` | then/else | `cond{if, then, else}` |
| `$eq` `$lt` `$add` `$sub` `$percent` | value | `eq{a,b}` `lt{a,b}` `add{a,b}` `sub{a,b}` `percent{value,max}` |
| `$text` | String(v) | `text{value}` |
| `$coalesce [a, b, …]` | first truthy | literals that are falsy are skipped; the first truthy literal BEFORE any dynamic wins; else `coalesce{values: [dyn…, first truthy literal]}` |
| `$clamp [v, min, max]` | the clamped number | `clamp{value, min, max}` |
| `$fill [template, {name: v}]` | ALWAYS `fill{template, params}` (the template is a `$string.<id>` only the surface's table resolves) | `fill{template, params}` |
| `$not` | boolean | `not{value}` |
| `$if` / `$any` on a part | decides at expansion | the part IS emitted with `visible` = the dynamic condition (`and{values}` for several `$if`s, `or{values}` for several `$any`s; one alone is itself) |
| `$each` over a dynamic array | one part per item | nothing (use List with a template) |

Template KEYS interpolate too (`"@media (max-width: $breakpoint.{props.collapseBelow|md})"`).

Truthiness everywhere (expander and renderer) = `truthy`: `undefined`,
`null`, `false` and `""` are false; `0` is TRUE.

### Core functions (`core.catalog.json` `functions.core`)
`percent{value, max}` → `"N%"` clamped 0..100, two decimals, `0%` when max
≤ 0 · `add{a,b}` / `sub{a,b}` (non-numbers count 0) · `eq{a,b}` strict,
EXCEPT a number equals a string spelling it (`5` = `"5"`: form and URL state
arrive as strings; `0` ≠ `""`, `1` ≠ `true`) · `lt{a,b}` numeric ·
`clamp{value, min?, max?}` (a number; a missing bound does not clamp; min
wins when max < min) · `cond{if, then, else}` · `fallback{value, default}`
(value unless undefined/null; `0`, `""` and `false` are values) ·
`concat{values}` (undefined/null as "") · `coalesce{values}` (first truthy)
· `text{value}` (String, missing stays missing) · `map{value, cases,
default}` (`cases[String(value)]`) · `len{value}` (array or string length,
else 0) · `fill{template, params}` (`{name}` placeholders filled, unknown
ones stay; the template arrives already resolved through the string table) ·
`set{path, value}` (ACTION only).

**The bind table is MORE than the core names.** The expander also emits the
basic catalog's `not` (`{!x}`, `$not`), `and` (several bound `$if`s) and
`or` (several bound `$any`s), with the same truthiness; `required` completes
the reference table. A renderer implements ALL of `bindFunctionNames`
(generated; = `src/dynamic.ts BIND_FUNCTIONS`): a table of only
`coreFunctionNames` resolves Collapsible's `set` value `not{…}` to nothing.
Arguments resolve at ANY depth (arrays and objects of bindings/calls).

### Two-way binding: `$set` and the combined action
A template part may say `"$set": {"press": {"prop": "open", "value": "{!props.open}"}}`.
When the author's `props.open` is a DataBinding `{path: P}`, the part's
`press` action gets `functionCall: {call: "set", args: {path: P, value: <evaluated value>}}`
next to any routed author event (key order `event`, then `functionCall`). A
literal prop adds nothing — the host keeps owning that state through the
routed event, as before. An Action carries ONE function: when the author's
routed handler is itself a `functionCall` (e.g. `openUrl`), it wins, NO `set` is
added and the reducer reports `on.<event>: a function action replaces the
two-way set of props.<prop>; …` once per macro (never lost silently; locked
by `bind-time.json` `extra` `TabBar/set:author-function`). Used by: Collapsible (open), Pagination (page),
ButtonGroup (selected), TabBar (value), Rating (value), AlertDialog
cancel/confirm (open → false).

**Action semantics** (`src/dynamic.ts runAction`): resolve the function's
args AND the event context against the data model AS IT IS at the press,
then apply `set` (write `value` at `path`, relative paths against the
node's data scope like a binding; missing objects are created), then
dispatch the event. So a Collapsible's routed event carries the NEW `open`
(`!old`), and the write does not double-negate it.

**Natives with bindable state** (Dialog/Drawer/Popover/Toast `open`,
Tabs/ToggleGroup/Select/Radio/Input/… `value`, Checkbox/Switch `checked`,
Toggle `pressed`, Carousel `page`, Table `sort`/`selected`, ChipInput
`values`, DateRangePicker `start`/`end`, NumberField/TimePicker `value`):
when the prop is a binding, the renderer writes the user's change to that
path itself AND fires `change` (this is the two-way binding the macros now
reach through `set`).

### The bind pass (reference: `src/dynamic.ts`)
After expansion, per data model: `bindTree(node, data, {scope?, strings?,
extensions?})`:

- **Props resolve along the component's SCHEMA** (`resolveNodeProps`): a
  binding or call anywhere resolves; arrays and SHAPED objects recurse; a
  DATA position — a shape-less `object` or an array of them, today only
  Table `rows` (generated `dataProps`) — is the author's data: a LITERAL
  there is copied VERBATIM (a row `{path: "/etc/hosts"}`, a `call` column or
  a `"$string.cancel"` cell stay data), a dynamic value AT the position
  resolves and its result is never descended. Values a binding returns are
  never descended or `$string`-resolved anywhere. Everything else
  (`resolveDynamic`) resolves at any depth: bindings by JSON pointer
  (absolute `/a/b`, or relative to the scope), calls by the BIND table,
  `$string.<id>` through the surface's string table.
- STYLE values, recipe props and **`accessibility`** (A2UI's
  `label`/`description` are DynamicStrings; the `$a11y` states too) resolve
  the same way; an empty result is dropped.
- A node whose `visible` resolves falsy is dropped (with its subtree, no
  layout); `visible` is removed. Actions stay untouched (press time).
- A `template` and the slots of a **ROW-SCOPED** component (catalog
  `slotScope: "row"`, generated `rowSlotComponents`: Table) stay UNBOUND.
  The painter binds each slot cell once per row with `bindRowSlot(slot,
  rowsProp, rows, index, data)`: `rowsProp` = the UNBOUND `rows` value — a
  binding gives the scope `{base: "<its pointer>/<index>"}` (relative paths
  read the row in the model, a relative `set` writes into it); a literal or
  a call gives `{item: rows[index]}` (relative paths read inside the row,
  absolute ones the surface data, a relative `set` writes nothing). `index`
  is the row's index in `rows` as given, BEFORE any local sort. A cell whose
  `visible` resolves falsy for a row renders empty for that row.

After expansion a STYLE value may be dynamic (Progress fill `width:
percent{…}`); resolve it, then apply it like an authored value. Authors
cannot write dynamic style values (`validateStyle` refuses).

`fixtures/bind-time.json` — `cases`: 26 (every bindable macro prop bound to
`/<prop>`), each × THREE data models (a sample, its falsy twin, and `{}` =
the path MISSING, so every operand's `fallback{value, default}` takes its
default; `0` is TRUTHY, so the number twin exercises arithmetic, not
truthiness) → the bound tree and every `set` press outcome. `extra`: 5
hand-written cases (bound-rows and literal-rows Table slot cells with
`rowSlots` = every row's bound cells, DATA rows verbatim, a bound accessible
name, an author function meeting `$set` with the reducer's `issues`, a
numeric prop bound to string data). The audit cases are pinned in
`round1.test.ts` ("the audit's bugs"), the review fixes in
`round1-fixes.test.ts`.

## 2. Responsive authoring and the style whitelist

### Conditions (`catalog/style.json` `conditions`, `src/style.ts`)
One level deep, ONE regex grammar (`STYLE_MEDIA_PATTERN`):

- `@media (min-width: N)` / `(max-width: N)` / `(min-height: N)` /
  `(max-height: N)` — `N` = `<number>px` or `$breakpoint.<name>`; sizes are
  the SURFACE box (not the window); `min-*` = `size >= N`, `max-*` =
  `size < N` (strict, so `min-width: md` and `max-width: md` never overlap —
  React: emit `@container xui (width < Npx)`, not `max-width`).
- `@media (orientation: portrait|landscape)` — portrait = height ≥ width;
  landscape when the height is unknown.
- `@media (hover: hover|none)` — a hover-capable pointer.
- `@media (prefers-reduced-motion: reduce|no-preference)` — the platform
  setting.
- NOT supported: `prefers-color-scheme` (mode belongs to the theme),
  ranges, `em`, `screen`, nesting.
- States `:hover`, `:focus-visible`, `:pressed` against the node's own
  interaction state (`:focus-visible` = keyboard focus only).

Flattening (`resolveConditions`, the reference): base keys, then every
MATCHING `@media` block in SOURCE order (later wins; authors write
narrow-first), then `:hover`, `:focus-visible`, `:pressed` in that order
(pressed wins). A `$breakpoint` token resolves through the theme
(`theme.tokens.breakpoint`) BEFORE matching; an unknown token never matches
(and `validateStyle` reports it). `resolveStyleValues` also rewrites the
token inside media KEYS (`resolveConditionKey`). Locked by
`fixtures/style-conditions.json` (7 cases × contexts).

### Breakpoint tokens
`tokens.json` `breakpoint: [sm, md, lg, xl]`, px, per theme (built-ins:
640 / 768 / 1024 / 1280, Tailwind's). `activeBreakpoint(width, bp)` = the
LAST name whose px ≤ width, `null` below `sm` (= `base`).

### Responsive props
A prop with `responsive: true` also takes `{base, sm?, md?, lg?, xl?}`;
the value at a breakpoint = the nearest key at or below it (`base` under
the first) — `responsiveAt`. Validation checks each value and refuses
other keys. Today: Stack `direction gap align justify wrap`, Grid
`columns gap`, Drawer `side`.

- On a MACRO the expander evaluates each part's style with the `base`
  props, then once per breakpoint the value names (ascending, each
  inheriting the last), and every key that changed lands in an
  `"@media (min-width: $breakpoint.<bp>)"` block of that part. Props,
  conditions and recipe props see `base` (a responsive value cannot add or
  remove parts). A key cannot be UNSET at a breakpoint.
- On a NATIVE the renderer picks `responsiveAt(value, activeBreakpoint(surfaceWidth, theme.tokens.breakpoint))`
  at layout time (Drawer: a bottom sheet on phones, a side panel from md),
  and queries the recipe with that RESOLVED value (`when: {side: "right"}`
  never sees the object).
- An author `style` on a macro still merges over the root's, condition
  blocks merged key by key, so a Box style override remains a second way.

### Whitelist additions (`catalog/style.json`; schema + `STYLE_KEY_NAMES` ×4)

| key | values | notes |
|---|---|---|
| `overflowX`, `overflowY` | visible hidden clip scroll auto | layout |
| `insetBlockStart`, `insetBlockEnd` | length | layout (logical top/bottom) |
| `borderTopWidth` … `borderLeftWidth` | number / `$border.*` | LAYOUT EFFECT like `borderWidth` (`STYLE_LAYOUT_EFFECT_KEYS`); taffy border rect per side |
| `borderStyle` | solid dashed dotted | default solid; a width without a style is solid |
| `borderTopLeftRadius` … `borderBottomLeftRadius` | length | per corner, over `borderRadius` |
| `backgroundGradient` | `{angle, stops: [{color, offset 0..1}]}` (≥ 2 stops) | linear, CSS angle (0 = up, 90 = right), painted OVER `backgroundColor` (the colour shows only through transparent stops) |
| `fontWeight` | 300 400 500 600 700 800 | was 400–700 |
| `letterSpacing` | length | px |
| `textAlign` | left right center start end justify | start/end flip in RTL |
| `textDecoration` | none underline line-through | |
| `textTransform` | none uppercase lowercase capitalize | |
| `fontStyle` | normal italic | |
| `transition` | `$motion.<name>` ONLY | the duration opacity/colour/transform/size changes animate with; nothing animates without it; reduced motion = 0 ms |
| `transitionEasing` | `$ease.<name>` ONLY | cubic-bezier `[x1, y1, x2, y2]` |
| `transform` | `translate(Xpx, Ypx)`, `scale(N)`, `rotate(Ndeg)`, space-separated | PAINT-ONLY: never changes layout, hit-testing follows paint where the platform does |
| `visibility` | visible hidden | hidden keeps its box, paints nothing, leaves the a11y tree |
| `pointerEvents` | auto none | none = presses pass through |
| `userSelect` | auto none text | |
| `cursor` | auto default pointer text not-allowed grab grabbing move col-resize row-resize | a hint pointer platforms honour, touch ignores |

Still excluded (decided): `zIndex` (paint order = tree order, absolute
children LAST among siblings, overlays stack by LAYER), per-side border
COLOURS (gpui paints one border colour per box), `calc()`, named grid
lines, `gridAutoFlow`, `justifyItems`, `order`, multi-value shorthands.

CSS mapping lives in `src/css.ts` (`styleToCss`: `transition` →
`all <ms>ms cubic-bezier(…)`, `backgroundGradient` → `background-image:
linear-gradient(…)`, any non-zero width without colour → `transparent`,
without style → `solid`).

## 3. Catalog

82 components (46 natives, 36 macros), 81 visible.

### New components

| component | kind | why that kind / key behaviour |
|---|---|---|
| Form | native | owns field collection, submit gating and error focus |
| NumberField | native | steppers, clamping, locale formatting |
| ChipInput | native | token editing, keyboard |
| DateRangePicker | native | one calendar, two-step selection |
| TimePicker | native | list or typed `HH:mm` |
| FileUpload | native | drop zone, file picker, host upload callback |
| CodeBlock | native | tokenizer, gutter, copy |
| ContextMenu | native | right-click / long-press, shares DropdownMenu's menu |
| Toast | native | toast LAYER, timers, live region |
| Table | native (was a macro) | typed cells, sort, selection, windowing, slot cells |
| ScrollArea | macro | Box with overflowX/Y |
| Sidebar | macro | two columns, the column hidden below `collapseBelow` (a `max-width` block) or when `collapsed` (bound → `visible`). NOTHING replaces it on narrow screens (one slot cannot render twice without duplicating ids and state): the AUTHOR gives phones another way in — a TabBar, or a Sheet behind an AppBar menu button (`$string.menu`) holding the same destinations; the catalog description (= the prompt) says so |
| AppBar | macro | title/subtitle/back + `leading`/`trailing` slots spliced in place (`"$slot:name"`) |
| TabBar | macro | bottom destinations, `$set` value |
| Breadcrumb | macro | links + current, `select` event |
| Stepper | macro | `status` = done/current/upcoming per step (a `$recipe` discriminator) |
| Rating | macro | `range(max)` stars, `$set` value |
| Kbd, Label | macros | text styling |
| AlertDialog | macro over Dialog | `dismissible: false`, a template-built `footer` SLOT part (cancel/confirm), `$set` open → false |
| HoverCard | macro over Popover | `openOn: hover` |
| Sparkline | macro over Chart | `kind: sparkline`, no axes/grid/legend |

### Changed components
- **Table** (BREAKING): `columns: [{key, label, type?, slot?, width?, align?, sortable?}]`,
  `rows: [object]` (bindable), `rowKey="id"`, `sort {key, direction}`
  (bindable: bound = the host orders `rows`; unbound = the renderer sorts a
  local copy: numbers numerically, strings by the locale collator, missing
  last), `selectable none|single|multiple` (multiple = a checkbox column with
  a select-all header), `selected [keys]` (bindable), `caption`,
  `emptyText`, `striped` (the painter queries `row` with `striped: true` on
  ODD rows), `density`, `stickyHeader`. Cell `type`: text, number (locale
  number), date (locale date), boolean (a tick), badge (the value in a
  Badge look), slot (the Table slot named by `slot` renders once per row
  with the ROW as its data scope: relative bindings resolve into the row —
  §1 "The bind pass", `bindRowSlot`). `slots: ["*"]` = any slot name;
  `slotScope: "row"`. `rows` is DATA: literal rows are never interpreted.
  Windowed past `WINDOW_THRESHOLD` = 50 rows.
- **Chart**: kinds `bar stackedBar line area pie donut sparkline`; `min`,
  `max`, `xLabel`, `yLabel`, `showAxes`, `showGrid`, `showLegend`,
  `showValues`; categories optional (sparkline); the width is the box's (no
  fixed viewBox). Numbers every painter shares (`src/chart.ts`): extent
  (`min` defaults to 0 or the lowest negative, `max` to the largest value
  but at least 0 — for stackedBar the largest category sum of the POSITIVE
  values up and the lowest sum of the NEGATIVE ones down; sparkline spans
  its own min..max; computed in loops, never by spreading, so a 150k-point
  series is fine), the accessible name `$string.chartSummary` /
  `sparklineSummary` filled from `chartSummaryParams` /
  `sparklineSummaryParams`,
  `niceTicks(min, max, 5)` (Heckbert, steps 1/2/5×10ⁿ, rounded to the step's
  decimals) for axis ticks and grid lines, colours = the series tone, else
  `$color.chart1..8` in order (wrapping), donut hole = 0.6 × radius, the
  legend only with 2+ series or slices, the tooltip part on hover/focus.
- **Image**: `fallback` icon (on error or no src; default glyph
  `builtinIcons["Image.fallback"]`), `loading lazy|eager`, `focalX/Y` 0..1
  (the point kept when cropped, 0.5 default).
- **Drawer**: `dismissible`, `dragToDismiss`, responsive `side`.
- **Textarea**: `autosize` from `rows` to `maxRows`.
- **Markdown**: `lines` truncation, `style`. **Text**: `live off|polite|assertive`.
- **Popover**: `openOn press|hover` (hover also opens on keyboard focus).
- **Select** = the combobox: `searchable` adds a filter field and fires
  `search {query}` (debounce 150 ms); `options` is bindable so the host
  refills it (async source); `source` names a host list; `emptyText`.
- **Button**: `submit` submits the enclosing Form.
- **DatePicker**: `firstDayOfWeek` (else the locale's, §4).
- **DropdownMenu** + ContextMenu `menuItem.kind`: `item | checkbox | separator | label | submenu`
  (`checked` bindable, `items` one level deep, `shortcut`).

### Event payloads (what the renderer merges into the author's event context)

The payload is merged OVER the author's resolved `context`: on a clashing key
the component's payload WINS (React `hardening.test.tsx`, Rust
`Surface::fire`). The payload is also still sent as its own `payload` field.

| component | event | context |
|---|---|---|
| value controls (Input, Textarea, Select, Radio, ToggleGroup, Tabs, Slider, NumberField, TimePicker, DatePicker, Composer) | `change` | `{value}` (NumberField: a number) |
| Checkbox, Switch / Toggle | `change` | `{checked}` / `{pressed}` |
| Dialog, Drawer, Popover, Toast (and the macros over them) | `change` | `{open}` |
| DateRangePicker | `change` | `{start, end}` |
| ChipInput | `change` / `add` / `remove` | `{values}` / `{value}` / `{value}` |
| Select | `search` | `{query}` |
| DropdownMenu, ContextMenu | `select` | `{value}` (+ `{checked}` for a checkbox item, its NEW state) |
| Table | `sort` / `select` / `rowPress` | `{sort: {key, direction}}` / `{selected: [keys]}` / `{key, row}` |
| Form | `submit` / `invalid` / `change` | `{values: {<field name>: value}}` / `{errors: [{name, message}]}` / `{name, value}` |
| FileUpload | `upload` / `remove` | `{files: [{name, size, type}]}` / `{name}`; the BYTES go to the host's `onUpload(files, {nodeId, name})` callback, never through an event |
| Toast | `dismiss` / `action` | `{}` |
| Input, NumberField / Composer | `submit` | `{value}` |
| macros | as routed by `$on` + `$context` | e.g. Pagination `{page}`, Breadcrumb `{value}`, Rating `{value}` |

### Form
Collects every field beneath it that has a `name` (Input, Textarea,
NumberField, Checkbox, Radio, Switch, Select, ChipInput, DatePicker,
DateRangePicker, TimePicker, FileUpload, Slider) — nested Forms are not
collected by the outer one. Submit (a `submit` Button, Enter in a
single-line field) runs EVERY field's `checks` regardless of `validateOn`;
any failure: no `submit`, `invalid {errors}` fires, EVERY failing field
shows ALL its failed messages under it (id `<field id>.error`, linked by
aria-describedby / the platform equivalent), `summary: true` also lists
them above the fields, focus moves to the first invalid field and
`invalidFields` is announced. `busy` refuses submits and shows the submit
Button loading; `disabled` makes every field inert.

### Toast
A native in the TOAST layer (above dialogs), stacked bottom-centre
(phones) / bottom-end (wide), newest nearest the edge, max 3 visible. The
host queues by rendering one Toast per notice with `open` bound; the
renderer runs `duration` (0 = sticky; paused while hovered or focused),
writes `open: false`, fires `dismiss` + `change`. `error` toasts are
`assertive`, others `polite`; a toast never takes focus.

### CodeBlock tokenizer (`catalog/code.json`, `src/code.ts`)
One forward scan, first matching rule wins: line kinds → line comments →
block comments → strings (`\` escapes; stop at a line break unless the
quote is `multiline`; `keyStrings` makes a string before `:` a property) →
markup tags (html) → numbers (`[0-9][0-9A-Za-z_.]*`) → identifiers
(keyword | property before `:` with `keyIdents` | function before `(` |
property after `.` | type (typeNames or Capitalized) | plain) → spaces →
operator runs → one punctuation char → one plain char. Then split into
LINES and merge adjacent same-kind tokens; a line's tokens concatenate to
exactly its text. Kinds = the `codeToken` enum; the recipe is
`CodeBlock/token` with `when: {kind}`. `fixtures/code-tokens.json` (17
cases, all 15 languages + edge cases).

### Built-in glyphs (`core.catalog.json` `builtinIcons`)
`<Component>.<part>[.<variant>]` → an icons.json name, e.g.
`CodeBlock.copy → ui-copy`, `FileUpload.icon → upload`,
`Table.sortIcon.asc → ui-chevron-up`, `Toast.icon.error → ui-error`.
Generated as `builtinIconSlots` / `builtinIconNames` (×4). Never hardcode
a glyph for an owned part.

## 4. Data, locale and strings

- **`visible`** (every component, reserved key like `style`): `true |
  false | {path} | {call}`; falsy once resolved = not rendered, no layout,
  not in the a11y tree. The expander also sets it on parts whose `$if` was
  bound. On a macro the author's `visible` lands on the expanded root.
- **Template keys**: `children: {componentId, path, key?}` → normalized
  `template: {component, path, key?}`; `key` = a pointer relative to each
  item whose value identifies it, so reordering keeps the item's component
  state (focus, scroll, local input); the index when absent.
- **`accessibility`** is now copied by `reduceSurface` too (it was only in
  `reduceNested`).
- **SurfaceSettings** (`src/types.ts`): `locale` (BCP 47, default `en-US`),
  `strings` (overrides), `mode` (`light | dark | system`), `density`,
  `contrast` (`normal | high | system`), `theme`.
- **Locale contract** (`catalog/locale.json`, `src/locale.ts`): formatting
  is the platform's ICU (Intl / Foundation / java.text / icu4x) in the
  SURFACE locale for `formatNumber`, `formatCurrency`, `formatDate`,
  `pluralize`, NumberField, Table number/date cells, calendar month and
  weekday names. What must not differ is data: `parseLocale` first maps
  deprecated language codes (`languageAliases`: `iw`→`he`, `ji`→`yi`,
  `in`→`id`, `jw`→`jv`, `mo`→`ro`, `tl`→`fil`; Java/Android still emit
  them); `weekStart(locale)` (CLDR firstDay by region; a bare language takes
  `likelyRegion` = CLDR likelySubtags for every rtl language and ~90 common
  ones, e.g. `ur`→PK = Sunday, `ps`→AF = Saturday, `dv`→MV = Friday;
  default Monday), `textDirection(locale)` (`rtl` languages; the surface
  root gets `direction: rtl` unless the author set one).
- **RTL glyphs** (`locale.json` `rtlMirroredIcons`, `mirrorsInRtl`): under
  rtl a painter MIRRORS exactly these directional glyphs (back, chevron
  left/right, arrow right, send, undo, external link — semantic and Lucide
  names): `scaleX(-1)` as the OUTERMOST transform of the glyph, after the
  node's own `transform` (CSS `transform: scaleX(-1) <own>`; SwiftUI
  `.scaleEffect(x: -1)` outside `.rotationEffect`; Compose `graphicsLayer {
  scaleX = -1 }` outside), so AppBar back, Pagination/Carousel/DatePicker
  prev/next, Breadcrumb separators, NavRow/ListRow/Collapsible chevrons and
  submenu indicators point the reading direction (a rotated Collapsible
  chevron still points down when open). Media transport glyphs, up/down
  chevrons and everything else are never mirrored. Flex rows already
  mirror their ORDER (`direction: rtl`), so prev sits on the right AND
  points right.
- **Built-in strings** (`catalog/strings.json`, 52 ids): the copy no author
  writes (search placeholder, month buttons, copy, previous/next, close,
  confirm/cancel, `{count} selected`, `pageOf`, `ratingStar` "{value} of
  {max}", `ratingValue`, `stepDone`/`stepCurrent`/`stepUpcoming`,
  `chartSummary`, `sparklineSummary`, `pagination`, `breadcrumb`, …). A
  sentence is ONE id with placeholders, never concatenated fragments (the
  fragment `of` is gone); a macro fills one through `$fill` → `fill`. The host overrides per surface
  (`stringTable(overrides)`); `$string.<id>` in a prop default, a macro
  template or an authored value resolves at bind time; `{name}` placeholders
  fill via `formatString`. `validateProps` reports an unknown id.
  Generated as `stringIds` / `stringDefaults` (×4).

## 5. Theming

- **Tokens added** (`catalog/tokens.json`, append-only): `color.chart6..8`;
  `control.appBar`, `tabBar`, `toast`; `shadow.xl`; `breakpoint.{sm,md,lg,xl}`
  (px); `ease.{standard,decelerate,accelerate}` (`[x1, y1, x2, y2]`);
  `density.{compact,comfortable}` (multipliers 0.5..2).
- **Elevation** = the shadow ladder `none < sm < md < lg < xl` (flat, card,
  popover, dialog, toast). Inside a layer paint order is tree order; layers
  (base < overlay < toast) stack — there is no zIndex.
- **Modes**: `resolveMode(setting, systemPrefersDark)`; `system` follows the
  platform live.
- **High contrast**: a theme may carry `contrast: {light?, dark?}` partial
  colour/shadow overlays; `applyContrast(theme)` merges them over the modes
  when the platform asks (or `contrast: high`). Neutral ships overlays
  (foreground, mutedForeground, border, input, ring, card/popover
  foregrounds); the others inherit them.
- **Density**: `applyDensity(theme, density)` scales `control` and
  `spacing` by `$density.<name>` and rounds to whole px, once per surface;
  radii, type and borders do not scale. Every resolver and fixture stays
  density-agnostic.
- **Recipes**: keys add `borderTop/Right/Bottom/LeftWidth`, `borderStyle`,
  `letterSpacing`, `textDecoration`, `textTransform`, `fontStyle`,
  `transition` (`$motion.*`), `transitionEasing` (`$ease.*`), `transform`;
  states add `invalid` (a field whose checks failed) and `dragover`.
  Painter-supplied `when` discriminators: `CodeBlock/token.kind`,
  `Table/headerCell|cell.align`, `Table/row.striped`. Template-built slot
  parts (AlertDialog `footer`) count as parts.
- **Built-ins**: neutral has values for every new token and recipes for
  every new component (motion on Button, Toggle, Tabs, Switch, rows, Pill;
  the Collapsible chevron rotates 90° when open; FileUpload's dashed
  drop zone; Stepper statuses; Rating stars; the CodeBlock palette on
  chart6–8 + semantic hues); playful overrides several (overshoot easing,
  pill toasts, primary chips); exponential takes its easings from
  `packages/design-tokens` `motion.ease`, adds `shadow.xl`,
  `control.appBar/tabBar/toast` (scripts/generate-themes.ts) and glass
  recipes for the new fields, menus, toast, table, code block, drop zone,
  sidebar and tab bar (`themes/exponential.recipes.json`). The theme builder edits breakpoints and
  density; `diffTheme` keeps `contrast`.
- **Geometry**: `control-geometry.json` now also locks NumberField
  (field), ChipInput (field, `minHeight`), TimePicker and DateRangePicker
  (trigger).

## 6. Accessibility (`catalog/a11y.json`)

`roles` = the MACHINE-READABLE vocabulary (ARIA names plus `text` = static
text and `hidden` = out of the accessibility tree; generated `a11yRoles`).
Every component has a `role` (one of `roles`: its main accessible object),
`notes` (prose: composite parts, states, which strings name it) and `keys`
(82 entries; `round1.test.ts` gates it).

**Macro parts carry their semantics on the tree.** A template part's
`$a11y` (`catalog/macros.json`) is evaluated like props (bound inputs emit
calls) into the expanded node's `accessibility`: `{role, label,
description, level, expanded, current (page|step|false), selected, pressed,
checked, valueNow, valueMin, valueMax, hidden, autoFocus}`; the author's
`accessibility` on a macro wins over its root's. A painter maps them to the
platform (ARIA role/aria-*, SwiftUI traits/`accessibilityValue`, Compose
`semantics { role; stateDescription; heading(); selected }`, AccessKit) and
`autoFocus` = focus lands there when the enclosing dialog opens. Today:
Card/Group (group + title), Separator, ScrollArea (region), Band/Heading
(heading + level), RowList/Stepper (list; steps = listitem, current=step,
labelled `stepDone|Current|Upcoming`), Alert (alert|status), EmptyState
(status), Progress (progressbar + values), Meter, Collapsible trigger
(button + expanded), Pagination (navigation `pagination`, status label
`pageOf`), ButtonGroup (pressed), Breadcrumb (navigation `breadcrumb`,
current=page, separators hidden), TabBar (navigation; items button +
current=page), Rating (stars = buttons labelled `ratingStar`, the chosen one
pressed; read-only = img labelled `ratingValue`), AlertDialog (alertdialog,
cancel autoFocus), Sidebar (navigation + main), AppBar (banner, title
heading 1), NavRow (link + current), Pill (button + pressed when
pressable). Natives own theirs (`a11y.json`). Highlights: Tabs / Radio / ToggleGroup = roving tab stop + arrows
(wrap) + Home/End; Select = combobox + listbox, type-ahead, searchable keeps
focus in the field with aria-activedescendant; DropdownMenu / ContextMenu =
menu semantics with ArrowRight/ArrowLeft for submenus, Shift+F10 opens a
ContextMenu; Accordion headers; DatePicker grid = arrows ±1 day/week,
PageUp/PageDown month (Shift = year), Home/End by the LOCALE week start;
Tooltip shows on hover AND keyboard focus; Dialog focus trap + return;
AlertDialog focuses cancel; Toast never steals focus.

Rules (all components): keyboard reachability in tree order, focus ring
only for keyboard focus, labels/descriptions/errors linked (error id
`<node id>.error`), hidden nodes leave the a11y tree, icon-only controls
take `label`, `accessibility.label/description` override, reduced motion
zeroes transitions and stops shimmer.

Host → surface commands (`SurfaceCommand`): `focus {id}`, `announce {text,
live}`, `scrollIntoView {id}`. Live regions: `Text.live`, Toast, Form's
`invalidFields` announcement, CodeBlock's `copied`.

## 7. Prompt (`src/prompt.ts`)

New rules teach: `visible`, template `key`, responsive props (marked `^`)
with `{base, sm?, md?, lg?, xl?}`, the condition grammar with
`$breakpoint.*`, the three state blocks, `display: "none"`, Form and
Sidebar, leaving built-in copy unset. Compaction to stay under budget:
props that mean the same everywhere (`name`, `label`, `value`, `style`,
`checks`, …) are described ONCE (`COMMON_PROPS`), enum-first and `false`
defaults are implied, only shapes the listed components use are printed.
Full prompt 6451 tokens (budget 6500), lite 5205, terse 4752 for 81
components (was 5438 for 60).

## 8. Generated constants (Swift / Kotlin / Rust, `src/catalog.generated.ts`)

New rows: `styleLayoutEffectKeys`, `coreFunctionNames`,
`bindFunctionNames` (the whole bind table a renderer implements),
`stringIds`, `stringDefaults`, `codeLanguageNames`, `builtinIconSlots`,
`builtinIconNames`, `weekStartRegions` + `weekStartDays`,
`likelyRegionLanguages` + `likelyRegions`, `languageAliases` +
`languageAliasTargets`, `rtlLanguages`, `rtlMirroredIcons`, `a11yRoles`,
`rowSlotComponents`, `dataProps` (`Component.prop`). New scalars:
`styleTransformPattern`, `defaultLocale`, `weekStartDefault`.
`styleMediaPattern` changed (the new grammar), `styleStates` is
`[:hover, :focus-visible, :pressed]`, `functionNames` gained 15 names,
`tokenGroups` gained `breakpoint`, `ease`, `density`. The Rust file also
embeds `STRINGS_JSON`, `LOCALE_JSON`, `CODE_JSON`, `A11Y_JSON` beside the
existing catalog JSON, mirrored into
`apps/desktop/crates/exponential-ui/src/generated/catalog.rs`. Themes
generated outputs carry the new token groups and `contrast`.

## 9. Per-renderer checklists

### React (`@exponential-at/ui-react`, the reference renderer)
1. Media: map every condition (`min/max-width|height` → `@container xui (width >= N)` / `(width < N)` with `$breakpoint` resolved from the theme; orientation via the container's aspect: emit both `(orientation: …)` container queries; `hover`/`prefers-reduced-motion` → real `@media`). States `:hover` → `:hover` (and `[data-xs~=hover]`), `:focus-visible` → `:focus-visible`, `:pressed` → `:active, [data-xs~=pressed]`, emitted after the media rules in that order.
2. Bind pass: run `bindTree` semantics in the binding layer (props along the schema with DATA props verbatim, styles, recipe props, `accessibility`, `$string`), drop falsy `visible`, honour template `key` as the React key; Table slot cells per row (`bindRowSlot`); map `accessibility` (role, states, `autoFocus`) to ARIA; mirror `rtlMirroredIcons` under rtl.
3. Actions: `runAction` order; implement `set` against the surface data model.
4. Natives: Form, NumberField, ChipInput, DateRangePicker, TimePicker, FileUpload (host `onUpload`), CodeBlock (`tokenizeCode` from this package), ContextMenu, Toast (layer + queue), Table (native, windowing ≥ 50), Chart kinds + `niceTicks`; Image fallback/focal point (`object-position`), Drawer flags, Textarea autosize, Markdown lines, Text live, Popover openOn, Select search.
5. New style keys via `styleToCss`; motion: `transition` + `transitionEasing`; reduced motion → `transition-duration: 0ms`.
6. Theme: `resolveMode('system')` via `matchMedia`, `applyDensity`, `applyContrast` (`prefers-contrast: more`), new recipe keys/states (`invalid`, `dragover`).
7. Built-in strings + glyphs from the generated tables; `textDirection` for the root.

### Rust core (`apps/desktop/crates/exponential-ui`)
1. `expr.rs`: dynamic inputs emit calls exactly as `src/expr.ts` (member vs operand rule, `fallback`, `concat`, `$lt`, `$sub`, `$not`, `$text` over a value object, `range(...)`, key interpolation); `TemplateChild` gains `"$slot:name"` splices and `slots` values that are template nodes (AlertDialog) — `MacroTemplate.slots` becomes `IndexMap<String, SlotRef>` (string | node), plus `$set`.
2. `macros.rs`: `$if`/`$any` → `visible` (and/or), `$set` → `functionCall: set` (after `event`; an author FUNCTION wins and is reported), `$a11y` → `accessibility` (author label/description merged over the root's), responsive variants → media blocks, author `visible`/`accessibility` onto the root; `expr.rs`: `$clamp`, `$fill` (always emits), `$eq` numeric-string equality. Replay `catalog-macros.json` (189 cases incl. `bound:*`/`responsive:*`) and `bind-time.json` (`cases` × 3 datasets AND `extra` with `issues` + `rowSlots`).
3. `reducer.rs`: `visible` (validate: boolean or dynamic), template `key`, `accessibility` on the flat path, slot names checked (`*` = any).
4. `validate.rs`: responsive values, objects without shape, `$string.` ids.
5. `style_check.rs`: the new grammar (`STYLE_MEDIA_PATTERN`), states, unknown breakpoint tokens, `transform`/`gradient` value types, the new keys; `resolveConditions` port + `style-conditions.json` replay. Layout: per-side border widths into taffy's border rect, `overflowX/Y`, `insetBlockStart/End`, `visibility: hidden` keeps the box.
6. `data.rs`: the WHOLE bind table (`bindFunctionNames`: core + `clamp`/`fill` + and/or/not/required; `eq` numeric-string equality), bind pass dropping falsy `visible`, props along the schema (`dataProps` verbatim), `accessibility` resolved, row-scoped slots left unbound + `bind_row_slot` (scope `item` for literal rows), `$string` resolution through the surface table (never inside data); `reducer.rs`: `visible` on the BASIC path too, the `$set` collision issue.
7. `theme.rs`: token groups `breakpoint`/`ease` (tuples)/`density`, `contrast`, `apply_density`, `apply_contrast`, `resolve_mode`, recipe keys/states, `resolveConditionKey`. Replay `theme-*.json`, `control-geometry.json`.
8. New modules to port (pure): `code.rs` (tokenizer, replay `code-tokens.json`), `locale.rs` (+ `languageAliases`, `rtlMirroredIcons`), `chart.rs` (`chart_extent` with loops and split positive/negative stacks, `nice_ticks`, the summary params), strings table. The embedded JSON constants are already generated.
9. FFI: expose SurfaceSettings, `SurfaceCommand`, the action outcome (`set` writes) so the natives do not re-implement the bind pass.

### gpui painter (`exponential-ui-gpui`)
1. Paint the new keys: per-side border widths with ONE colour, `borderStyle` dashed/dotted (custom path), per-corner radii (`corner_radii`), `backgroundGradient` (gpui linear gradient, two stops; more stops: chain quads), `transform` at paint time only (no layout), `visibility`, `opacity` unchanged, `letterSpacing`/`textTransform`/`textDecoration`/`fontStyle`/weights 300–800 in text runs, `cursor` → `CursorStyle`, `pointerEvents: none` skips hit-testing.
2. Motion: animate colour/opacity/transform/size between resolved styles with `transition` ms + `transitionEasing` bezier; 0 ms when the OS asks for reduced motion.
3. Conditions: the core flattens with the surface size, hover capability (true on desktop), reduced motion, and the node's states (`hover`, `focus-visible` = focus from keyboard, `pressed`).
4. Natives: the round-1 list above, menus with submenus, Toast layer, Table with windowing, CodeBlock with the core tokenizer, Chart with `nice_ticks` and the chart palette, FileUpload drop (gpui file drop events) + host callback.
5. Keyboard per `a11y.json`; AccessKit roles from `accessibility.role`/states where available; mirror `rtlMirroredIcons` under rtl.

### SwiftUI (iOS)
1. Conditions: surface size from `GeometryReader`/`onGeometryChange`, `horizontalSizeClass` is NOT the rule (use px vs `$breakpoint`), hover = iPadOS pointer (`.onHover` available), reduced motion = `accessibilityReduceMotion`.
2. Keys: per-side widths via an overlay `Path` (one colour), dashed via `StrokeStyle(dash:)`, per-corner radii via `UnevenRoundedRectangle`, gradients via `LinearGradient` (angle → unit points), `transform` via `.offset/.scaleEffect/.rotationEffect` (paint-only by construction), `visibility: hidden` = `.opacity(0)` + `.accessibilityHidden`, `pointerEvents: none` = `.allowsHitTesting(false)`, `cursor` ignored (pointer: `.pointerStyle` on iPadOS 17+ optional).
3. Motion: `.animation(.timingCurve(x1, y1, x2, y2, duration:), value:)`.
4. Natives: Form, NumberField (Stepper + TextField with `.number` format in the surface locale), ChipInput, DateRangePicker (`MultiDatePicker` is not a range — custom grid), TimePicker (list/menu), FileUpload (`fileImporter` + drop), CodeBlock (core tokenizer via FFI or a Swift port), ContextMenu (`.contextMenu`), Toast (overlay layer), Table (LazyVStack windowing), Charts (Swift Charts with `niceTicks` values).
5. Locale: `Locale(identifier:)` for formatters; week start from `catalog/locale.json` data (do NOT trust `Calendar.current`), direction from `rtlLanguages`.
6. VoiceOver per `a11y.json` and each node's `accessibility` (`accessibilityAddTraits` from role/states, `.isHeader` + level, `accessibilityValue` for progress, `autoFocus` via `@AccessibilityFocusState`; adjustable actions for Slider/NumberField), `AccessibilityNotification.Announcement` for `announce`, `@AccessibilityFocusState` for `focus`; `.flipsForRightToLeftLayoutDirection(true)` only on `rtlMirroredIcons`.

### Compose (Android)
1. Conditions: `BoxWithConstraints` / `onSizeChanged` on the SURFACE, hover = `LocalInputModeManager` / mouse presence, reduced motion = `Settings.Global.ANIMATOR_DURATION_SCALE == 0`.
2. Keys: per-side widths + dashes via `drawBehind` (`PathEffect.dashPathEffect`), per-corner radii `RoundedCornerShape(topStart…)`, gradients `Brush.linearGradient` (angle → offsets in the size), `transform` via `graphicsLayer` (paint-only), `visibility: hidden` = `alpha 0` + `clearAndSetSemantics {}`, `pointerEvents: none` = no pointer input, `cursor` = `pointerHoverIcon`.
3. Motion: `animate*AsState(tween(ms, easing = CubicBezierEasing(x1, y1, x2, y2)))`.
4. Natives as for SwiftUI (Material 3 `DateRangePicker`, `TimePicker`-like list, `ExposedDropdownMenuBox` for the searchable Select, `LazyColumn` for Table windowing, `DropdownMenu` + long-press for ContextMenu, `SnackbarHost` semantics for the toast layer).
5. Locale: `java.text`/`android.icu` formatters in the surface locale; week start from the generated table; `LayoutDirection` from `rtlLanguages`.
6. TalkBack per `a11y.json` and each node's `accessibility` (`semantics { role; stateDescription; heading(); selected; liveRegion }`), `View.announceForAccessibility` for `announce`, `FocusRequester` for `focus` and `autoFocus`; mirror only `rtlMirroredIcons` under `LayoutDirection.Rtl`.

## 10. Breaking changes and migration

- **Table** is a NATIVE with object columns/rows. Old surfaces
  (`columns: ["Name"]`, `rows: [["Alice"]]`) fail validation; migrate to
  `columns: [{key: "name", label: "Name"}]`, `rows: [{name: "Alice"}]`.
- **Sparkline** is a core macro: an extension may no longer define one
  (the example extension's native is now `TrendLine`).
- **Style**: `":hover"` and `":focus-visible"` are valid (were refused);
  `@media (max-width: N)` means `< N`.
- **Functions**: `functionNames` = the basic 14, then the 15 core names
  (renderers must implement them; `set` is an action); `bindFunctionNames`
  is what a bind-time resolver needs (adds and/or/not/required). `eq` now
  equates a number with a string spelling it.
- **Bind pass**: Table slot cells are no longer bound against the surface
  (the pass leaves them for per-row binding); literal Table `rows` are never
  interpreted; `accessibility` resolves. `bind-time.json` datasets are 3 per
  case and the file gains `extra`.
- **Expanded tree**: macro parts carry `accessibility` (from `$a11y`);
  Pagination's prev/next use `lt` (disabled at the ends for any page beyond
  them) and write `clamp`ed pages.
- **a11y.json**: `role` is now machine-readable (prose moved to `notes`),
  plus `roles`.
- **strings.json**: `of` removed; 9 ids added. **locale.json**:
  `languageAliases`, `rtlMirroredIcons`, ~90 likely regions.
- **Actions** may carry `event` AND `functionCall` (only the expander emits
  both today).
- **UiNode** may carry `visible`; style values may be dynamic after
  expansion; `template.key`; `accessibility` from the flat path.
- **Recipes**: `recipeParts()` lists 82 components; neutral recipes for
  Table moved to the native's parts.
- **Rust**: `macros.json` now has `slots` values that are objects and
  `$set` keys — `MacroTemplate` must accept them before the crate can load
  the embedded table. Checked here: `cargo check -p exponential-ui --lib`
  compiles against the regenerated mirror; `tests/generated_drift.rs`
  still asserts 61 components (now 82) and `style_check.rs` the old media
  pattern — both are the Rust lane's to update.

## 11. Deferred (not in round 1)

- Per-side border COLOURS (gpui limit) and `zIndex` — kept out on purpose.
- A DnD reorder contract for List/Table rows, column resizing and pinning.
- Chart: stacked area, axis formatters per locale beyond `formatNumber`,
  log scales. Smallest next step: a `format` prop taking a basic-catalog
  format call.
- Form: async (server) validation round trip; smallest next step: an
  `errors` bindable prop on Form the host fills after `submit`.
- Combobox creatable entries (Select `allowCreate`).
- Locale-aware string pluralization of the built-in strings (only `{name}`
  placeholders today).
- Sidebar's own narrow-screen navigation (a built-in drawer): one slot
  cannot render in two places without duplicating ids and state. Smallest
  next step: a native `Sidebar` (or a `Drawer` mode that renders inline
  above a breakpoint) so ONE subtree moves between the column and a sheet.
- Multiple functions per Action (an author function AND the `$set` write):
  needs an Action shape beyond A2UI's single `functionCall`; reported instead.
