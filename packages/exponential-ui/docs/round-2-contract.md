# Exponential UI — round 2 contract

> **Round 4 (VAPP-103):** this is the round's historical record. Names it
> uses that round 3 folded (ToggleGroup, TabBar, ButtonGroup, DropdownMenu,
> ContextMenu, Sheet, HoverCard, Pill, EntityChip, Band, RowList and the
> ListRow family) are REMOVED from the catalog; `round-3-contract.md` §4 maps
> each to what replaces it.

Round 2 adds the authoring features round 1 left out, settles the open
contract questions in TS and Rust together, and turns the 53 gpui-vs-web
divergence causes into decisions. This file is the CONTRACT half: what
changed in `@exponential-at/ui` and what each renderer implements to match.
Everything here is locked by a fixture or a test (`src/round2.test.ts`).

Read §0, the sections you own, then §9 (your checklist) and §10.

## 0. What moved

| area | change | source of truth | locked by |
|---|---|---|---|
| Resizable | new native: panels + handles, bound `sizes`, limits, keys | `core.catalog.json`, `src/resizable.ts` | `fixtures/resizable.json` |
| `position: sticky` | pinned inside the nearest scroller | `catalog/style.json` | `style.test.ts`, `round2.test.ts` |
| `backdropBlur` | `$blur.<name>` only; new token group `blur` | `style.json`, `tokens.json`, themes | `theme-resolved.json`, `round2.test.ts` |
| `animation` | 8 keyframe sets, motion-token timing, reduced motion | `style.json` `animations`, `src/animation.ts` | `fixtures/animations.json` |
| `direction` | on ANY node, inherited; textAlign start/end per node | `style.json`, `src/direction.ts` | `fixtures/text-direction.json` |
| Formatter | host-supplied number/currency/percent/date/relative time; the English fallback takes the surface zone as a UTC offset | `src/format.ts`, `locale.json` `englishFallback` | `fixtures/format.json` |
| host API | `surface`: settings (+ `timeZone`), Formatter methods, commands (+ `scrollToIndex`); version 2 | `catalog/host.json` | `host.test.ts` |
| display of non-strings | a bound number in a text prop shows `412` | `src/format.ts displayString` | `format.json` `display` |
| built-in strings | 8 ids for the labels renderers still hard-coded | `catalog/strings.json` | `round2.test.ts` |
| templates | template nodes LIFTED out of the tree into `templates`; self-instantiating ones reported, kept in place | `src/reducer.ts` | `template-items.json`, `kitchen-sink.expanded.json`, `catalog-basic-map.json` |
| keys | missing/empty/duplicate keys → `#<index>`; suffixes accumulate, `.`/`~` escaped | `src/list.ts` | `template-items.json` |
| lists | one-axis window, sections + sticky headers, `scrollToIndex` | `src/list.ts`, `a11y.json` | `fixtures/virtual-list.json` |
| parts | every laid-out part is a recipes part, dumped as `<id>.<part>[.<i>]` | `catalog/recipes.json` | `theme.test.ts`, the harness |
| layout numbers | field/media intrinsic sizes, window, Resizable, TreeGuides | `catalog/layout.json` | generated constants |
| Video / Image / Chart / overlays | explicit sizes instead of browser defaults | `core.catalog.json`, `layout.json`, §7 | `conformance-known.json` |
| conformance | manifest v2 (24 suites), layout-box dump, real Nunito + Fira Code, 53 decisions | `src/conformance.ts`, `conformance/` | `manifest.json`, `compare.test.ts` |
| prompt | teaches sticky, backdropBlur, animation, Resizable, sections | `src/prompt.ts` | `prompt-budget.json` (6475 / 6500) |

Counts: 83 components (47 natives, 36 macros), 31 functions (17 core),
65 built-in strings, 16 token groups.

## 1. Resizable

```json
{ "id": "split", "component": "Resizable", "children": ["nav", "main"],
  "direction": { "base": "vertical", "md": "horizontal" },
  "sizes": { "path": "/ui/split" }, "panels": [{ "min": 20 }, { "min": 30, "collapsible": true }],
  "handle": true, "style": { "height": 480 } }
```

| prop | type | meaning |
|---|---|---|
| `direction` | horizontal \| vertical, responsive | panels side by side or stacked |
| `sizes` | `[number]`, bindable, two-way | percent per child (sum 100) |
| `panels` | `[panel {min?, max?, collapsible?}]` | limits by index; `min` default 10 (`layout.json` `panelMin`), max 100 |
| `handle` | boolean | draw a grip on each handle |
| `style` | style | the group box (give it a height) |

Event `change {sizes}` after every drag end or key. A bound `sizes` is
written back (two-way, like every bindable native).

**Numbers** (`src/resizable.ts`, all percent, rounded to 1e-6):

| function | rule |
|---|---|
| `normalizeSizes(sizes, count, panels)` | valid given sizes kept, missing ones share the rest of 100 (equal split when none given), scaled to 100, clamped to [min, max] (a collapsible panel may stay 0), the difference handed to panels with room in index order; a `min` above 100/count drops to it |
| `resizePanels(sizes, handle, delta, panels)` | from the sizes at the START of the drag; only panels `handle` and `handle + 1` change; clamped by both panels' limits; a collapsible panel dragged below half its min collapses to 0; a collapsed one dragged past half its min reopens at max(min, delta), clamped like any drag |
| `keyboardResize(…, key, orientation, direction)` | arrows move the handle on screen by `resizeStep` (10): horizontal Left/Right (rtl: ArrowRight shrinks the first panel), vertical Up/Down; Home/End = first panel to its min/max; Enter toggles collapse of the first panel if collapsible, else the second |
| `panelExtents(sizes, container, handle)` | px = (container − handles × `$control.hairline`) × size / 100 |
| `dragDelta(px, container, count, orientation, direction)` | pointer px → percent (rtl horizontal flips) |

**Layout**: a flex row (column) of panels and handles; every handle is
`$control.hairline` thick in layout with an 8 px hit area
(`resizeHandleHit`) centred on it; panels stretch across. **Recipes**:
`Resizable/root|panel|handle|grip`, `when: {direction, handle}`; neutral:
handle `$color.border` → `$color.ring` on hover/focus/pressed, grip a 6×24
pill (24×6 vertical). **A11y**: role group; each handle a focusable
`separator` (orientation of the line, valuenow = the panel before it,
valuemin/max = its limits, controls = that panel), named `$string.resize`.

## 2. Style

| key | values | rule |
|---|---|---|
| `position` | + `sticky` | relative in layout, then pinned by its `top`/`bottom`/`left`/`right`/`inset*` inside the nearest scrolling ancestor (overflow scroll/auto, else the surface scroller) while its parent is in view |
| `backdropBlur` | `$blur.sm\|md\|lg\|xl` only | blurs what is BEHIND the box through its translucent background. No platform blur → paint the background alone (gpui today), so authors pair it with a translucent `backgroundColor`. CSS: `backdrop-filter` + `-webkit-` |
| `animation` | pulse spin fade-in slide-in-up/down/left/right shimmer | `style.json` `animations` |
| `animationDuration` | `$motion.<name>` only | replaces the set's duration token (factor kept) |
| `direction` | ltr \| rtl, ANY node | inherited; flex order, logical insets/paddings, mirrored glyphs, textAlign start/end and the bidi paragraph direction follow the node's resolved direction |

Recipes may set `backdropBlur` and `animation` too. Tokens: `blur`
(neutral: 4 / 12 / 16 / 24 px; the others inherit).

**Animations** (`src/animation.ts`):

| name | keyframes | duration | easing | runs | reduced motion |
|---|---|---|---|---|---|
| pulse | opacity 1 → 0.5 → 1 | `$motion.slow` × 7 | `$ease.standard` | ∞ | opacity 1 |
| spin | rotate 0 → 360° | `$motion.slow` × 4 | linear | ∞ | 0° |
| fade-in | opacity 0 → 1 | `$motion.standard` | `$ease.decelerate` | 1 | end state |
| slide-in-up/down/left/right | opacity 0 → 1, translate 8 px → 0 (up = from below, left = from the right) | `$motion.standard` | `$ease.decelerate` | 1 | end state |
| shimmer | a band (background colour, alpha 0 → .5 → 0) sweeps −1 → 1 box widths, clipped to the box | `$motion.slow` × 5 | linear | ∞ | no band |

Channels compose OUTSIDE the node's own `transform`, about the box centre;
opacity MULTIPLIES the node's own (`paintedOpacity`: own 0.5 + pulse →
0.5 … 0.25; a finished fade-in keeps the author's 0.6). Channels are
physical: `direction` never mirrors them (rtl too: `slide-in-left` moves
left, the shimmer sweeps left → right). Easing applies per keyframe segment
(CSS semantics). An animation starts when the node enters the tree (first
paint, `visible` turning true, a template item appearing).

CSS: `@keyframes xui-<name>` (`keyframesCss`) + `animationCss(name,
animationTiming(name, theme))`, using the individual `translate`/`rotate`/
`scale` properties. Opacity keyframes move `--xui-a-opacity`, and
`styleToCss` writes `opacity: calc(<own> * var(--xui-a-opacity, 1))`;
register both custom properties once per document with
`ANIMATION_PROPERTIES_CSS` (unregistered, they jump instead of
interpolating).

**Text alignment** (`src/direction.ts`): `nodeTextAlign` = style
`textAlign`, else a Text's `align`, else `start`; start/end resolve against
the NODE's direction (`physicalTextAlign`). The Rust core's `Visual` carries
the resolved physical align; gpui and the natives shape text with the
node's direction as the bidi paragraph direction.

## 3. Formatting

ONE `Formatter` per surface, from `SurfaceSettings.locale` and the new
`timeZone` (IANA; default the platform's, UTC in fixtures). The zone is the
HOST's: it builds the formatter (or, for the Rust fallback, the UTC offset)
in it. The Rust core has no zone database and no zone setting: the zone
reaches it only through the formatter (FFI: `HostFormatter`, or `HostZone`
for the fallback; `FfiSettings.timeZone` is kept for the host):

```ts
interface Formatter {
  readonly locale: string
  number(value: number, o?: { decimals?: number; grouping?: boolean }): string   // 0..3 digits by default
  currency(value: number, code: string, o?: { decimals?: number; grouping?: boolean }): string  // the code's minor digits
  percent(value: number, o?: { decimals?: number }): string                       // 0.256 → 26%
  date(value: unknown, o?: { format?: string; style?: `short`|`medium`|`long`|`full`; time?: boolean }): string
  relativeTime(value: unknown, now: number): string                               // yesterday, in 2 hours
  plural(value: number): `zero`|`one`|`two`|`few`|`many`|`other`
}
```

| platform | formatter |
|---|---|
| TS (React) | `intlFormatter(locale, timeZone)` (Intl) |
| Rust core | a `Formatter` trait; default `EnglishFormatter` over `LOCALE_JSON` `englishFallback`, at a host-supplied UTC offset per instant (gpui: the local zone) |
| FFI (Swift, Kotlin) | a foreign `Formatter` callback interface the host passes in `SurfaceSettings` (Foundation / java.text·android.icu), else the core's English one |

Rules: date values are `yyyy-mm-dd` (a calendar day, no zone), an ISO
date-time (no offset = UTC) or epoch ms; anything else formats to ``.
Instants show in the surface zone; a calendar day never shifts. The English
fallback has no zone database: it takes `ZoneOffset = (utcMs) => minutes`
(TS `englishFormatter(zone)`, `fixedOffset`, `intlZoneOffset`; Rust: the
host passes the platform's offset). Its patterns, joiners (`, ` / ` at `)
and the unknown-currency template are data (`englishFallback`
`datePatterns`, `timePattern`, `dateTimeJoin`, `unknownCurrency`): read
them, never copy them. A numeric string is `[ws][+-](digits[.digits]|.digits)
[e[+-]digits][ws]`; hex, `Infinity` and separators are not numbers.
`format` = a TR35 subset (`y yy yyyy M MM MMM MMMM d dd E EEE EEEE h hh H HH
m mm s ss a`, quoted literals, `''`), ASCII digits, locale names; `style` =
the locale preset (`time: true` appends the short time). Relative time
picks the unit by |Δ| (< 1 min seconds, < 1 h minutes, < 1 day hours, < 7
days days, < 30 days weeks, < 365 days months of 30 days, else years),
truncated toward zero, numeric:auto phrases (now, yesterday, last week).
Rounding = half away from zero on the shortest round-trip decimal.

**Bind table**: `bindFunctionNames` now includes `formatNumber`,
`formatCurrency`, `formatDate`, `pluralize` (A2UI basic) and the core
`formatPercent{value, decimals}`, `formatRelativeTime{value, now?}`; all run
through the surface Formatter (`ResolveOptions.formatter`, `now`). A node
using `formatRelativeTime` without `now` re-binds at least once a minute.
`pluralize`: value 0 with a `zero` arm → it, else the CLDR category, else
`other`.

**Where renderers format**: Table `number`/`currency`/`percent`/`date`/
`relativeTime` cells (`column.currency`, `column.decimals`), NumberField
display, chart tick labels and value labels, DatePicker/DateRangePicker/
TimePicker trigger text, calendar month and weekday names (`date(…, {format:
"MMMM yyyy"})`, `"EEE"`), the format functions.

**Display of non-strings** (`displayString`): a `string`/`markdown` prop
bound to a number shows ECMAScript `Number.prototype.toString` (`412`,
`1.5`, `0.30000000000000004`, `1e+21`, `-0` → `0`), booleans `true`/`false`,
nothing for null/objects/arrays/NaN. No locale (use `formatNumber`).

**Strings**: `invalidValue` (a failed check without a message), `message`
(the Composer field's name), `codeBlock` (an untitled CodeBlock region),
`dialog` (an untitled Dialog/Drawer), `table` (an uncaptioned Table),
`carousel` / `slide` (role descriptions), `resize` (a Resizable handle).
Replace every hard-coded English label with them: React `natives/shared.tsx`
`Invalid value`, `inputs.tsx` Composer `aria-label`, `code.tsx` region
label, `overlays.tsx` title-less `DialogPrimitive.Title {kind}`, media
`aria-roledescription`; gpui `paint.rs` Table/FileUpload component-name
labels, `paint/date.rs MONTH_NAMES` (→ Formatter); Compose
`Semantics.kt`/`NodeInfo.kt` `Loading` (`$string.loading`),
`SurfaceMeasurer.kt`/`TextFields.kt` `Message`, `MediaLeaves.kt` `Page N`
(`pageOf`); SwiftUI `NodeInfo.swift`/`NodeView.swift` `Loading`,
`ControlLeaves.swift`/`SurfaceMeasurer.swift` `Message`, `MediaLeaves.swift`
`Page N`.

**Conformance** replays `format.json` through the English fallback
(exact). A platform Formatter (Foundation, android.icu) may run too, after
mapping U+202F and U+00A0 to a space on both sides: newer ICU puts U+202F
before AM/PM. `zoned` cases carry `timeZone` (ICU) and `offsetMinutes`
(fallback).

## 4. Templates, keys, rows

**Lifted templates.** A node a `template.component` names is the TEMPLATE:
the reducer lifts it out of every children list and slot (any depth, root
excepted) into `ReduceResult.templates[id]`, validated and expanded like the
root, in discovery order; it never also renders in place. The flat path
builds referenced ids eagerly; an id nobody defines is an issue
(`template: no component with this id`). Nested templates are lifted too.

**Cycles.** A template that would instantiate itself is an issue
(`template: cycle through this id`) and is NOT lifted: it stays where it was
and its owner renders no items. That covers a template naming its own owner,
an ancestor of its owner, the root (issue on the root id), and loops through
other templates (each one on the loop is reported).
`kitchen-sink.expanded.json` gains `templates: {"list-item": …}` and loses
the in-place `list-item`.

**Keys** (`templateItemKeys`): the value at `template.key` (a pointer
relative to the item; objects as JSON) — `#<index>` when missing, null, ``
or a DUPLICATE of an earlier key, more `#`s until unique; no key = the
index. Table rows: `tableRowKeys(rows, rowKey = "id")`, same rules.

**Instance ids**: every node of an item is painted as `<node id><suffix>`,
suffix = the enclosing item's suffix + `.<key>`; it ACCUMULATES through
nested templates (`issue.ops.1`). The key is escaped first
(`instanceSegment`: `~` → `~0`, `.` → `~1`), so `a.b` + `c` (`.a~1b.c`)
and `a` + `b.c` (`.a.b~1c`) differ. The Rust `item_suffix` appends, never
replaces. Table part ids keep the raw row key.

**List sections**: `sectionBy` (a field) groups CONSECUTIVE template items
(data order kept); slot `section` is row-scoped (List `slotScope: row`) and
binds once per section with the literal scope `{value, count, index}`
(`sectionScope` / `bindSectionHeader`); `stickyHeaders` pins the current one.
A header is a heading (level 3) in the a11y tree.

**Rust, same pass**: layout reads props through `resolve_node_props` (a
literal Table `rows` holding `{path}` is DATA, `bind-time.json`
`Table/slot-cell:literal-rows`); `reconcile.rs` and gpui events/paint show
non-string text values with `display_string`, never `as_str`; the FFI
exposes `bindRowSlotJson(slot, rowsProp, rows, index, data, options) →
node | null` and `bindSectionHeaderJson(slot, section, index, data,
options)`.

## 5. Virtual lists

One axis, vertical or horizontal (`direction`). Offsets start at the list's
content start (the inline START of a horizontal list: its right edge in
rtl, whatever sign the platform's scroll position uses); item i starts at Σ(extentⱼ + gap) for j < i, where a
`divided` list adds `$control.hairline` to the gap (the divider centred in
it). Unmeasured items take the MEAN extent of the measured items once any
is measured (the content length stays stable while rows measure in), else
`$control.row` (`itemExtents(measured, row)`; `windows` cases with
`measured` + `row`).

| function | rule |
|---|---|
| `virtualWindow(extents, gap, scroll, viewport, overscan = 5, offsets?)` | every item intersecting [scroll, scroll + viewport) (an item ending exactly at `scroll` counts) ± `windowOverscan`; `before`/`after` spacers. Pass cached `itemOffsets` (rebuilt only when an extent changes): two binary searches per frame |
| `scrollOffsetForIndex(…, index, viewport, scroll, align, inset, offsets?)` | start / center / end / nearest (default: the smallest move; none when visible); `inset` = the pinned header's extent; clamped to the content |
| `scrollOffsetForItem(rows, rowExtents, gap, dataIndex, …, stickyHeaders)` | a sectioned list: the DATA index → its flat row (absent = no move), `inset` = its own section header's extent when sticky |
| `listSections` + `sectionRows` | the flat row list: a header before each section |
| `stickyHeader(rowOffsets, rowExtents, headerRows, scroll)` | the last header at or before `scroll` pins at min(scroll, nextHeaderStart − extent); rendered even outside the window |

Windowing starts past `windowThreshold` (50). A list without a bounded size
windows against its nearest scrolling ancestor (overflow auto/scroll AND
taller content) or the host viewport. `scrollToIndex {id, index, align?}`
(a host command, `a11y.json`) takes the DATA index (before a local sort),
renders the item first, subtracts the pinned header. Items keep their
position in the whole list for assistive tech (setsize/posinset).

**Bench** (`fixtures/bench-list.json`): 100,000 `ListRow`s on a 390×800
surface, neutral theme; report `firstPaintMs`, `scrollStepMs` (100 viewport
steps), `scrollToIndexMs` (index 50000, start), `renderedItems` in each
renderer's README with the machine.

## 6. Parts (conformance coverage)

`catalog/recipes.json` `native.<C>.parts` lists EVERY part a painter lays
out. New: Switch/body, Radio/items, Slider/header, Table/body,
CodeBlock/body + code, FileUpload/title + browse + fileIcon, NumberField/input,
ChipInput/input, Tooltip/anchor, Accordion/count, List/section, Resizable/*.

| rule | |
|---|---|
| id | `<node id>.<part>` (`code.header`) |
| per item | `.<index>` (Radio `item.0`, Tabs `tab.1`, Accordion `trigger.1`, FileUpload `file.0`, ChipInput `chip.0`, CodeBlock `line.3`, List `divider.1` = before item 1, Table `headerCell.2`) |
| Table rows | `.<row key>` (`row.alice`, `checkbox.alice`, `cell.alice.0`, `checkbox.header`) |
| web | `data-xui-id` + `data-xui-part` on the part's element |
| paint parts (never dumped) | Switch/thumb, Slider/thumb + range, Ring track/fill, focus rings, arrows, decorations |

## 7. Explicit sizes (no browser defaults)

`catalog/layout.json` (generated as `layoutConstantNames`/`Values`,
`LAYOUT_JSON`): `windowThreshold` 50, `windowOverscan` 5, `resizeStep` 10,
`panelMin` 10, `resizeHandleHit` 8, `fieldIntrinsicWidth` 160,
`mediaIntrinsicWidth` 320, `mediaAspectRatio` 1.7777778,
`treeGuideColumn` 16.

| component | contract |
|---|---|
| overlays (Dialog, Drawer, Sheet, AlertDialog, Popover, HoverCard, Tooltip, DropdownMenu with a trigger slot) | LAYOUT-TRANSPARENT: the frame is the trigger's; the trigger is the flex item (a Link stretches in a stretch row, a Button keeps its control height); nothing without a trigger |
| DropdownMenu default trigger | outline button: `icon` (when set) + `label`, no chevron |
| Video | `aspectRatio` prop (default 16:9): height = width / ratio; max-content width 320, min-content 0 |
| Image | `aspectRatio`, else 16:9 without a height; the box never waits for the picture; `Image/fallback` fills it, glyph `$control.iconLg` centred |
| Chart | `height` = the WHOLE box (title, legend, axes inside); min-content width 0 (shrinks in a row) |
| AudioPlayer | title line (`AudioPlayer/track`) + `$spacing.xs` + controls row (`AudioPlayer/controls` = `$control.row`) |
| text fields (Input, Textarea, NumberField, ChipInput) | max-content width 160 |
| picker triggers (Select, DatePicker, DateRangePicker, TimePicker) | content-sized (text + glyph + padding + border), no 160 floor |
| Badge, Pill | 2 × paddingHorizontal + content; an absent `$if` part adds no gap |
| ToggleGroup | content-sized (align-self start) unless `fill` |
| TreeGuides | depth × 16 wide, stretched to its row; `TreeGuides/line` `width` = the stroke |
| Slider | header + `$spacing.xs` + a track row as tall as the thumb; the 6 px rail centred |
| Radio | `Radio/root` gap; an option row = max(dot, label line) |
| Table | header (cell + hairline) + rows (cell + hairline) + caption (line + 2 × paddingVertical) |
| CodeBlock | border + header (recipe padding + copy button + hairline) + lines × lineHeight; padding only from recipes. The header's `title` part = `title`, else the `language` id (`ts`; none for `plain`), so a header never stands empty |
| FileUpload | the drop zone holds icon, title, hint and the Browse button (`$control.buttonSm`, outline) |
| Text, Markdown | an empty text = 0 lines; a paragraph = its runs, no extra line |
| Switch | `[label + description][switch]`: label first, the track at the end; the row toggles |
| Checkbox, Radio | control first, then the label column; never overlapping |
| Accordion | `count` = its own muted part after the title |
| Carousel | only the active page is placed; the others are aria-hidden + inert, may stay mounted, carry `data-xui-inactive`. 2+ pages: `Carousel/root` gap + a centred `controls` row = `previous` + `indicator` (the dots) + `next`, spaced by `Carousel/controls` gap; the buttons' size from their recipes (28 × 28), no CSS literals |
| text in fields | starts at border + `paddingHorizontal`; no painter padding |

### Painter asks (VAPP-100)

What the core hands every painter so none re-derives it:

| item | contract |
|---|---|
| `FfiTextStyle.letterSpacing`, `textTransform`, `fontStyle` | resolved like the other text fields (letter spacing × font scale); measure and paint with them |
| `FfiLeaf.ownerComponent` | the native or MACRO owning a part leaf (`Tabs`, `Stepper`); null for a plain node |
| `FfiLeaf.text` | a number or boolean in the text prop = its display string (§3) |
| `Surface.effectiveTheme()` | the `Theme` the core resolves against: extends, density and contrast applied |
| `setHovered(ids)` / `setHover(id, on)`, `FfiNode.hovered` + `interactionStates` | the host's pointer hover; recipes' `hover` and `:hover` resolve through it |
| `FfiNode.hoverStyled` (Rust `Surface::hover_styled`) | the node restyles under the pointer: a `:hover` style block, or a recipe rule on `state: hover` its props match, in the effective theme. A painter tracks a mouse over these nodes (and pressables, triggers, fields) and reports the hover; re-read after a theme switch |
| `markDirty(index)` after a load | a Markdown is measured again; an Image box keeps its ratio's height |
| Stepper | `number` and `label` carry the step `status` (`$recipe`): the current number takes the marker's ink (`primaryForeground`), an upcoming one and an upcoming label are muted |

## 8. Conformance

- **Manifest v2** (`CONFORMANCE_VERSION` 2, 24 suites, 1642 cases): new
  suites `bind`, `style-conditions`, `code-tokens` (round 1's, now counted)
  and `format`, `template-items`, `text-direction`, `resizable`,
  `virtual-list`, `animations`. Runners read the version from the manifest.
- **Dump** (`conformance/dom.ts`): frames are LAYOUT boxes (`LAYOUT_ONLY_CSS`
  turns transforms and animations off while measuring); `[data-xui-inactive]`
  subtrees are skipped.
- **Fonts**: Nunito (7 static instances of Google Fonts' variable font) and
  Fira Code 6.2 (4 weights) under `conformance/fonts/` with their OFL
  texts; `fonts.json` maps them; not shipped in the npm package. Licence row:
  see the report.
- **Kitchen sink**: a `panels` Resizable is the last section (the kitchen
  sink uses every visible component).
- **Decisions**: `fixtures/conformance-known.json` `causes` (53 origins of
  the 2026-10-08 run) + `rules` (decisions with no origin today). The gpui
  ratchet reads only `cases`; its writer must keep `causes` and `rules`.

| group | nodes | right | fix |
|---|---|---|---|
| overlay trigger rows | resp-drawer, menu, hover-card(-trigger), alert-dialog, dialog, drawer, sheet, popover, tooltip | layout-transparent (§7) | React (wrappers fill/stretch, menu chevron); gpui (Link trigger stretch) |
| media sizes | audio, page-1, video | §7 | React (audio row, video ratio); gpui (Image ratio, video ratio) |
| chart heights | chart, chart-line, chart-donut | gpui (`height` = whole box) | React |
| control heights | code, table, nf-files | gpui | React |
| control heights | form-plan, form-volume | web | gpui |
| picker row widths | form-due, form-sub | web | gpui |
| badge and pill widths | hdr-badge (+ hdr-title/sub), nav-inbox.count (+ label), resp-nav-inbox.count (+ label), pill-1 (+ pill-2..4, footer-spacer) | web | gpui |
| empty and number text | list-item.meta.*, list-item.body.title(.*) | web | core + gpui (`display_string`, 0-line empty text); contract (lifted template) |
| sparkline shrink | sparkline (+ spark-label, progress-bound.*) | web | gpui (Chart min-content 0) |
| toggle group stretch | toggles | web | gpui |
| tree guides | tree-guides | gpui | React |
| card row | card-row.body.title/subtitle | web | gpui (a sibling part is 30 px wider) |
| markdown lines | main-md (+ nav-card) | web | gpui |
| transforms | tab-bar.item.1.icon | gpui (paint-only) | the dump (done) |

Rules without an origin today: Switch order (React), Checkbox overlap
(React), field text inset 14 vs 23 (gpui), Accordion count (gpui + React),
image fallbacks (both), carousel pages (React marks, dump skips), rtl bidi
of trailing punctuation (gpui), resp-states line breaks (survivor: UAX #14
both sides, inside the ±1 line tolerance), part coverage (React).

**Ratchet procedure** once React and gpui land: `bun run --filter
@exponential-at/ui conformance -- --write-baseline` (intended web changes:
lifted template, Resizable, layout-box dump, Nunito/Fira Code, the React
fixes), then `EXP_UI_WRITE_FIXTURES=1 cargo test -p exponential-ui-gpui
--test conformance` (keeps `causes`/`rules`), target zero origins; a
survivor gets `fix: ["survivor"]` + `reason`.

**Shots**: `specimens.json` now gives the round-1 containers real content
(Sidebar nav, AppBar action, AlertDialog/HoverCard triggers, ContextMenu
target, Form fields, ScrollArea rows, Resizable panels). View-catalog ids
to add (web + android here, ios + desktop on the Mac): `exponential-ui-`
`scroll-area sidebar app-bar code-block kbd label breadcrumb tab-bar
stepper alert-dialog hover-card context-menu toast sparkline form
number-field rating chip-input date-range-picker time-picker file-upload
resizable`.

## 9. Per-renderer checklists

### React (`@exponential-at/ui-react`)
1. Resizable native (flex + hairline handles, pointer capture from the drag START sizes, `keyboardResize`, `normalizeSizes`, two-way `sizes`, separator a11y, recipes incl. grip).
2. `position: sticky` (CSS), `backdropBlur` (`styleToCss` already maps it), `animation` (`ANIMATION_PROPERTIES_CSS` + `keyframesCss` once per document, replacing the own `--xui-band` rule; `styleToCss(…, theme)` multiplies opacity; drop it under reduced motion), `direction` on inner boxes (the CSS cascade does it; mirror glyphs by the node's direction).
3. Formatter: build `intlFormatter(locale, timeZone)` per surface, pass it to the bind pass (`formatter`), use it for Table cells (new `currency`/`percent`/`relativeTime` types), NumberField, chart ticks, pickers, calendar names.
4. Strings: replace every hard-coded label with the eight ids of §3.
5. Templates: read `templates` from the reduce result (drop `findNode` in-place lookup); keys already `#<index>`.
6. List: `sectionBy` + `section` slot (`bindSectionHeader`), `stickyHeaders`, horizontal windowing with cached offsets, gap/divider in the offsets, `scrollToIndex` command (`scrollOffsetForItem` when sectioned), setsize/posinset; bench → README.
7. Parts: `data-xui-id="<id>.<part>[.<i>]"` + `data-xui-part` on every part of §6; `data-xui-inactive` on inactive carousel pages.
8. §7 fixes on the web side: overlays layout-transparent (wrapper `display: contents` or trigger-sized), DropdownMenu without chevron, Chart frame = `height`, AudioPlayer controls row, Video `aspect-ratio`, Table/CodeBlock spacing from recipes only, FileUpload Browse button, TreeGuides columns, Switch label first, Checkbox no overlap.
9. Regenerate the web baseline after 1–8 (§8).

### Rust core (`exponential-ui`) + FFI
1. Load the new catalog: Resizable, List `slotScope: row` + `section` slot, `panel` def, cellType, Video `aspectRatio`, 31 functions; `generated_drift.rs` counts (83 components); `layout.json` (`LAYOUT_JSON`).
2. `theme.rs`: token group `blur` (field, `NUMBER_GROUPS`, completeness, overlay); recipe keys `backdropBlur` (token-only `blur`) and `animation` (enum); replay `theme-resolved.json`.
3. `style_check.rs`: `direction` no longer root-only (spec-driven), `sticky`, the two token keys; `animations` table (`style.json`) → `animation.rs` port (timing, `cubic_bezier`, frames) replaying `animations.json` within 1e-3.
4. `format.rs`: the `Formatter` trait + `EnglishFormatter` from `englishFallback` (patterns, joiners, unknown currency READ from it; a UTC-offset-per-instant zone; the numeric-string grammar); replay `format.json` `calls` + `zoned`; the six format functions in the bind table through the surface formatter (the host's, in its zone; `ZonedEnglishFormatter` at the host's offset); `display_string`; replay `format.json`.
5. `reducer.rs`: lift templates into `templates` (nested + flat, nested templates, the missing-id issue, cycles kept in place); replay `template-items.json` `reduce`, `catalog-basic-map.json`, `kitchen-sink.expanded.json`.
6. `list.rs`: keys (`#<index>`), `table_row_keys`, `item_extents` (unmeasured = the measured mean, else `$control.row`; `ListWindow` the same), `item_suffix` accumulation with the `~0`/`~1` escape, `virtual_window` + `scroll_offset_for_index` over cached offsets (binary search), `scroll_offset_for_item`, sections + sticky; replay `template-items.json`, `virtual-list.json`.
7. `resizable.rs`: the five functions (reopen at max(min, delta)); replay `resizable.json` within 1e-6.
   `animation.rs`: `keyframes_css` writes `--xui-a-opacity`, `ANIMATION_PROPERTIES_CSS`, `painted_opacity`; replay `animations.json` `css`, `properties`, `opacity`.
8. `direction`: per-node direction + physical align into `Visual`; replay `text-direction.json`.
9. `data.rs`/`reconcile.rs`: layout via `resolve_node_props`; `display_string` instead of `as_str` for text values.
10. `host/contract.rs`: host.json version 2 + the `surface` section.
11. FFI: `bindRowSlotJson`, `bindSectionHeaderJson`, `scrollToIndex` command, a foreign `Formatter` + `FfiSettings.timeZone` (the host's; the core's fallback gets the host's UTC offset through `HostZone`); regenerate bindings (`generate-bindings.sh`); bench 100k (`bench.rs`) → README.

### gpui (`exponential-ui-gpui`)
1. Resizable (drag, keys, AccessKit separator), sticky (paint offset inside the scroller), backdropBlur (fallback: background only, documented), animations from the core frames (reduced motion = rest).
2. §7 gpui-side fixes: Image/Video ratio, Chart min-content 0, picker triggers without the 160 floor, Radio gap, Slider row height, Badge/Pill no gap for absent parts, ToggleGroup content-sized, Link trigger stretch, Markdown paragraph lines, empty text = 0 lines, number text via `display_string`, Accordion count part, field text inset, bidi paragraph direction = node direction, the CardRow part.
3. Formatter: core `EnglishFormatter` at the local UTC offset (or a host one) for Table cells, NumberField, chart ticks, `paint/date.rs` month names; strings for the Table/FileUpload labels; painted opacity = own × frame.
4. List: horizontal windowing, sections + sticky headers, `scrollToIndex`, unbounded-height windows against the scroller; bench headless → README.
5. Ratchet: keep `causes`/`rules` when writing `conformance-known.json`; lower the counts.

### SwiftUI (Mac run)
1. Resizable (`DragGesture` from the start sizes, `accessibilityAdjustableAction` on handles), sticky (`pinnedViews` / offset), backdropBlur (`.background(.ultraThinMaterial)` by radius: sm/md thin, lg/xl regular; else background only), animations (`TimelineView` with the core frames; `accessibilityReduceMotion` = rest).
2. Formatter: a Foundation formatter over the FFI callback interface (`NumberFormatter`, `Date.FormatStyle`, `RelativeDateTimeFormatter` with the core's unit choice), locale + timeZone from settings.
3. Templates from `templates`; keys/suffixes from the core; List sections (`Section` headers, pinned), `ScrollViewReader.scrollTo` for `scrollToIndex`.
4. Strings (`Loading`, `Message`, `Page N`, §3), parts, §7 sizes, Switch label first; captures for the 22 specimen ids (§8).

### Compose
1. Resizable (`draggable` from the start sizes, semantics `setProgress`), sticky (`stickyHeader` in LazyColumn, offset elsewhere), backdropBlur (API 31+ `RenderEffect` on a background copy, else background only), animations (`rememberInfiniteTransition` from the core frames; animator scale 0 = rest).
2. Formatter over the FFI callback (`android.icu.text.NumberFormat`, `DateFormat`, `RelativeDateTimeFormatter`), locale + timeZone from settings.
3. Templates from `templates`; LazyColumn/LazyRow keys = the core keys; `stickyHeader` sections; `LazyListState.scrollToItem`.
4. Strings (`Loading`, `Message`, `Page N`), parts, §7 sizes, Switch label first; JVM bench → README; web + android captures for the 22 specimen ids.

## 10. Breaking changes

- `direction` is valid on any node (was root-only).
- `ReduceResult.templates`; a template node listed as a child no longer renders in place; the flat path validates template components.
- `bindFunctionNames` + 6 format functions; `functions.names` + `formatPercent`, `formatRelativeTime`; `functions.core` 17.
- List has a row-scoped `section` slot (`rowSlotComponents` = Table, List).
- `cellType` + `currency`, `percent`, `relativeTime`; `column` + `currency`, `decimals`.
- Chart `height` = the whole box (round 1 painters differed).
- Image without `aspectRatio` or a height is 16:9 (round 1 had no rule; the web took the picture's own ratio once loaded): a portrait or square image given only a width is now cropped or letterboxed (`fit`). Set `aspectRatio`.
- `keyframesCss` animates `--xui-a-opacity`, not `opacity`; register `ANIMATION_PROPERTIES_CSS`.
- Instance suffixes escape `~` and `.` in keys.
- A self-instantiating template is no longer lifted (issue + kept in place).
- Format functions read numeric strings by the §3 grammar (`0x10`, `Infinity` → ``).
- `catalog/host.json` version 2 (`surface`).
- Token group `blur` (every root theme must define it); recipe keys + `backdropBlur`, `animation`.
- Conformance version 2; `conformance-known.json` gains `causes`/`rules`.
- Strings + 8 ids; `styleToCss(style, fonts, theme?)`.

## 11. Deferred

- Per-renderer implementation of everything above (the renderer lanes).
- Backdrop blur on gpui: no backdrop sampling in the pinned gpui; the background-only fallback stands until upstream adds one (smallest step: a gpui `BackdropFilter` element upstream).
- A localized string table per locale (strings stay host overrides).
- Calendar-aware relative days (`yesterday` across midnight in the surface zone): today 24 h buckets; smallest step: a `calendarDays` option on `relativeTime`.
- Cascading resize (pushing past the neighbour into the next panel): only adjacent panels change.
