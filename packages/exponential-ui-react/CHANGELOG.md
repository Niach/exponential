# Changelog: `@exponential_app/eui-react`

All notable changes to this package. The SDK packages share one version and
release together from a `eui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## Unreleased

- Style key `fontVariantNumeric: tabular-nums`: CSS `font-variant-numeric` through the shared node sheet.
- Select `button` popup keeps its starting cursor (#EXP-1629): `selectOpensWithoutCursor` is `pill` only.
- Select `button` popup matches web's picker (#EXP-1629): recipe and layout values.
- Button `size: "icon-xs"` + `tone: "muted"` (#EXP-1629): `icon-xs` is icon-only (no label part, the label is the `aria-label`), its size, radius, glyph and muted colour from the recipes. Snapshots re-recorded (every Button gains `data-r-tone="default"`; + the three new catalog cases).
- Select option `hint` (#EXP-1629): the `itemHint` part before the check.
- Select `button` trigger glyph (#EXP-1629): the picked option's `icon` + `iconColor` before the value.
- Select `button` trigger + Text `truncate` (#EXP-1778): the button is the field trigger with no label part (the label = its `aria-label`) and the `Select.dropdown` glyph; its popup at least `selectButtonPopupMinWidth`, `selectButtonPopupOffset` under it; a `middle` Text cuts its line with `watchMiddleTruncate` (the element's font on a canvas, again on resize).
- Select `row` disabled (#EXP-1629): the exponential theme's dimmed value + chevron, from the recipes.
- Menu item `detail`, `identifier`, `preview` (#EXP-1629): the `identifier` part between a row's icon and its label, the `detail` part after the label (a SubTrigger's too); a row's `preview` = the Embed native in the `Menu/preview` box portalled beside the row (`natives/menu-preview.tsx`: the TS reference's timing, a keyboard focus = the highlight, Radix's outside guards let presses into it), its events the Menu's `action`; a menu without previews tracks nothing. `menuRows` + `menuPreview` replay here.
- Select option `iconColor` (#EXP-1629): the `itemIcon` and the `triggerIcon` `valueIcon` take `propColor(iconColor)` as their colour.
- Composer matches the app: the send glyph is `ui-submit` (`CircleArrowUp`, in `FALLBACK_ICONS`); a flat field stretches across the row (`align-self: stretch`, grown by its `min-height`, never a `height`) with its text at the recipe's padding-top. Snapshots: every `Composer/*` case (the glyph; `Composer/layout=flat` = `min-height` instead of `height`).
- Menu item `iconColor` (#EXP-1629): the item's glyph takes `propColor(iconColor)` as its colour; a `checkbox` entry draws its `icon` (the core's row: check, glyph, label).
- Skeleton `rows` (#EXP-1629): no hairline between rows (base CSS); `batch-ui.test.tsx` replays `fixtures/skeleton-rows.json`.
- Select `multiple` (#EXP-1629): a picked option's item carries `data-r-join-top` / `data-r-join-bottom` (`selectItemJoins` over the shown options), so the theme draws a run of adjacent picks as one block.
- Popover `matchAnchorWidth`: the content's `width` = `var(--radix-popover-trigger-width)` (Radix's anchor width: the `anchor` node's virtual ref, else the trigger; floating-ui re-measures it on a resize). The browser overlay suite asserts the panel width on the fixture's `matchAnchorWidth` cases. Snapshots: + `Popover/matchAnchorWidth=true|false`.
- Select `pill` popup EXACTLY where `placeInsetOverlay` puts it (#EXP-1629): Radix's popper (no middleware, its wrapper rounded to device px) still places it, then the content moves by a `translate` (`insetPopupNudge`) onto the core's box: the inset, the clamp, then whole px from the pill, half away from zero (was up to half a px off). Re-applied whenever Radix moves the wrapper and as the pill, the popup or the window moves or resizes. The browser overlay suite replays the `overlay-geometry.json` `inset` cases with no tolerance, plain and searchable (harness `?view=inset`).
- Composer slot `above` (#EXP-1762): `.xui-composer-above` is the root's FIRST child (a stretched column over the field); a flat root with one (`[data-above]`) wraps it onto its own full-width line (base-css).
- Composer `layout: "flat"` (#EXP-1762): the root is one row (`[data-r-layout="flat"]`): the leading slot in its own `.xui-composer-bar` before the field, the field flexes and centres on the disc, the controls bar after it. Snapshots: `data-r-layout` on every Composer part, + `Composer/layout=flat|stacked`.
- `ColorPicker` 28 px cells, `inline`, child trigger (#EXP-1775): macro data only; snapshots gain `ColorPicker/inline=*` and `ColorPicker/child`; the three new `pickerPick` cases replay here.
- Select `triggerIcon` + the own-colour marks (#EXP-1764): a `row` trigger with `triggerIcon` draws the chosen option's `icon` (the `valueIcon` part) before its value; `brand-claude` paints in its own orange (its node's fill).
- `IconPicker` + `ColorPicker` (#EXP-1760): a pick (a cell, a swatch, the No icon row) closes the picker's Popover (`ClosePickerContext`), as the core does; `pickerPick` replays here.
- Select `pill` trigger, `loading`, option `mono`, `searchPlaceholder`, the `searchIcon` part (#EXP-1765): the pill is the field trigger content-sized with no label part (the label = its `aria-label`, no description row) and the `Select.dropdown` glyph in the `chevron` part; its popup `selectRowPopupWidth` wide, `selectPillPopupOffset` under it and `selectPillPopupInset` outside its aligned edge (`selectPopupGeometry` `alignOffset`); opened by the pointer it shows no cursor until a key (the keyboard keeps it); `loading` fills the list with the Skeleton's `selectLoadingRows` rows (`<id>.skeleton`) instead of the options and `empty`; a `mono` option's item is `data-mono` (the mono type) with its `trailing` a muted sans `.xui-Select-trailing` run, `aria-hidden` (the option is named by its label); the value/placeholder parts carry the trigger's `disabled`; a pill search shows the `Select.clear` button ("Clear search", a 0.75 × `searchIcon` glyph centred in a 1.25 × box) only while it holds text, a press empties it and keeps focus; a `loading` Select still opens; `Select/empty` honours the recipe `textAlign` (the part is a block).
- Select option `description` (#EXP-1629): an option's glyph sits in the `itemIcon` part (its recipe size), a described option draws `itemText` holding `itemLabel` and the muted `itemDescription`. `popupMinWidth` lets the popup fit its options (the core's layer minimum).
- Icon `size: xl` (#EXP-1629): 28 px from the recipe (`$control.iconXl`).
- Input `variant: "box"` + `size` (#EXP-1629): no label part, the field's `aria-label` is the label (as `bare` / `code`). Snapshots re-recorded (every Input gains `data-r-size="default"`).
- Select option `trailing` + event `open` (#EXP-1629): the popup row reads `label trailing` (Radix's `ItemText` keeps the label alone), the trigger the label; `open` fires as the popup opens.
- Textarea `variant` + Input `bare` / `code` (#EXP-1629): no label part, the field's `aria-label` is the label. Snapshots re-recorded (every Textarea gains `data-r-variant="field"`).
- Select `description` (#EXP-1629): under a field trigger; in a row the `labels` column (label + hint) and the `control` column (value + chevron); the trigger's `aria-describedby` names it.
- Composer `leading` / `trailing` slots and image paste (#EDE-65 R), React's: the slots sit in the controls row (`.xui-composer-leading` before the attach control, `.xui-composer-trailing` before send); a paste with image files fires `attach` {files} and calls `host.onUpload(images, {nodeId, name: <the Composer's id>})`, a paste with other files only is prevented, text pastes as before.
- Select (#EXP-1629, the IDE's multi picker): a `row` trigger's value and chevron sit in the `button` part, which lends its pointer hover to them; a `multiple` popup opens with the cursor on its first chosen option. Snapshot `Select/triggerVariant=row` re-recorded (the `button` span).

## 0.21.0 (2026-11-09)

- Select `triggerVariant: row` (#EXP-1629): the trigger is the `label`, a spacer, the `value` (at most `selectRowValueMaxWidth`, truncated) and the `Select.chevron` glyph as the `chevron` part (the field trigger names both parts too); `align` + `popupWidth` on the popup (row: hung off the VALUE, `start`, 262 px, `selectRowPopupOffset` under it, `selectRowPopupMargin` from the window's edges); every Select opens at the TOP with the cursor on the first enabled option (the search field focused when searchable), no longer scrolled to the chosen one; the list's bottom `fade` (`searchListFade`) while options sit below; the search glyph in `mutedForeground`; item `hover` = the cursor (Radix's highlighted item too), `selected` = chosen only. Snapshots re-recorded.
- Recipes (#EXP-1629): `backgroundClip: padding-box` paints a part's fill inside its border, `$shadow.popover` resolves like every shadow, and an Avatar's picture paints its `image` recipe fill (the exponential theme's `secondary`) under its ring.
- `primitives/kbd` `Kbd`: the `accent` fill (the catalog's `Kbd` recipe; was `muted`), through the shared `KBD_CLASSES`.
- Input `variant: "row"` (#EXP-1629): a Group row (label inside, the value end-aligned at 70%); the exponential theme's row is the IDE's (14 px rem). Snapshots re-recorded (every Input field gains `data-r-variant="field"`).
- Embed (#EDE-65): a nested genUI surface painted in place, sandboxed: its own data model, its own surface id (`embedSurfaceId`, so inner ids never meet the host's), the host's theme, mode, strings, locale and presentation hooks only (`embedHost`); every inner event fires the Embed's `action` with `{name, context}`, an inner host function is denied, a bad `surface` paints the `embedInvalid` error Alert, a deep change of the surface remounts the card.
- Composer `menuKeys` (#EDE-65): while it is on, Up, Down, Enter, Tab and Escape without a modifier fire `key` {key} (prevented and stopped); an IME composition's keys stay the IME's (its Enter never submits).
- KeyRecorder field capture (#EXP-1629): a press while idle starts recording and focuses the field; while recording every key (Tab, Escape included) and mouse buttons 3-5 go to `key` with the canonical name and held flags, blur stops it, and a press does nothing.

## 0.20.0 (2026-11-08)

- Chart `valueFormat` (ticks, tooltip, value labels, summary) and `rows` + `categoryField` + series `field` (#FEED-205).
- The icon list follows the app (#EXP-1733): `nav-threads` is now `nav-chats`, `thread-results` is now `chat-results`.
- Popover, Menu (press and context) and Tooltip close once their anchor scrolls, moves or leaves the tree (#EXP-1739, `watchAnchor`).
- `Window` (VAPP-290) paints from the core macro (no per-renderer code); snapshot recorded.

## 0.19.1 (2026-11-07)

- Released with the SDK at 0.19.1: the Swift and Compose hosts check an app's calls against its upgraded package, so a format-5 app's `records_upsert` is no longer refused silently (VAPP-272).

## 0.19.0 (2026-11-07)

- Round 8: the `Canvas` native (`CanvasView`: role img named by `label`, the Motion box, `pointer` events), Checkbox `indeterminate` (`checked="indeterminate"`, the `ui-minus` glyph, `aria-checked=mixed`, the press rule of `interactions-round1.json` `checkbox`), the primitive Checkbox's dash. Snapshots re-recorded (Progress header, Checkbox recipe prop).

## 0.18.0 (2026-11-06)

- A focused text field adopts an action's `set` of its bound value (#VAPP-288): `ctx.assignData` / `ctx.writeEpoch`, `useWriteEpoch`, `useHostOwnedValue({epoch})`; echoes keep the typing rule.
- Tree (#VAPP-285): a Tree row keeps `role=treeitem` and takes the WAI-ARIA tree keys (`treeKeyTarget`).
- A foldable Row (#VAPP-284): a pressable Box with `on.toggle` and `aria-expanded` takes ArrowRight / ArrowLeft (`foldKey`, rtl-aware); a press target inside another fires only its own press.

## 0.17.0 (2026-11-06)

- DatePicker `inline` (the month grid in place), `marks` event dots, `today`, the `month` event; days carry the `isToday` / `outside` recipe props (VAPP-282).
- Tabs `variant: strip` (#VAPP-279): the strip DOM (items, folded tabs out of flow, the `+N` button), the fit on every render and resize, the pointer / long-press drag with a live preview, Ctrl/Cmd+Shift+Arrow; replays `tabsReorder`.
- `host.markdownOptions` (`breaks`, `density`, `selectable`, `streaming`): hard breaks, the chat rhythm (`data-r-density` on the Markdown root), selection off, and streamed text whose finished blocks are memoized (each top-level block re-renders only when its block changes).
- Markdown host hooks (`catalog/host.json` `markdown`): a `textLinks` chip paints the Chip look inline, `imageSize` / `onImagePress` size and press block images, `attachment` paints a one-link paragraph as the Video / AudioPlayer native or the FileUpload file row. `hostPlugin(host)` forwards the policy's hooks.
- `SettingsRow` (#VAPP-280, round 7): one line of a settings page as a catalog macro: the label (+ a muted description) on the start column, the control (its children) on the end column, 16 px in, 12 px top and bottom, at least 44 px tall; below `stackBelow` (default `sm`; `never` keeps it inline) the control goes under the label and stretches. A group named by the label. Every renderer paints it through the core's expansion; prompt budget 9,292 → 9,359 tokens (a named exception).
- Sidebar `rail` (#VAPP-277): paints the icon rail from the expanded tree; Tooltip resolves the logical sides `start`/`end` against the surface direction, and a Tooltip inside a focusable host (a pressable Row) adds no tab stop: the host's keyboard focus opens it.

## 0.16.3 (2026-11-06)

- Markdown `imageMaxHeight` is a host render option (`markdown.options`, a number or `null` = fit the width at the image's aspect, capped at the viewport; default 160), and Compose Markdown text follows the surface's font scale and folds at whole lines (`Markdown.foldHeight`).
- Slider `icon`, `endLabel`, `showValue` (#VAPP-286): a leading glyph, short muted text after the track (bindable) and `showValue: false` (no value above the track); the core lays the volume row out as a `row` part (gap sm: icon iconSm, the growing track, the end label at least buttonIcon wide). New parts `row`, `icon`, `end`; prompt budget 9,216 -> 9,237 (a named exception). React paints the row in the same DOM order.
- `host.popupHost` (VAPP-281): anchored Popovers / Menus render into the host's element outside the surface (theme scope copied, in-flow content, events route back).
- Popover `align` + `anchor`, Menu `side` + `align` + `anchor` (VAPP-276): Radix's `align`; an `anchor`ed overlay is placed against that node (`PopoverPrimitive.Anchor virtualRef`, a virtual DropdownMenu trigger), the node gets the trigger's aria + toggle, Escape returns focus to it.
- Markdown parses the same on all five renderers (`markdown-render.json`, every golden case passes; VAPP-258), and a number field seeded with a value selects it on focus so Save runs once (VAPP-208).

## 0.16.2 (2026-11-06)

- The icon list follows the app: `power` and the eDE icons (`ede-window`, `ede-lock`, `ede-power`, `ede-suspend`, …) are new.

## 0.16.1 (2026-11-05)

- Released with the SDK at 0.16.1: a top or bottom Drawer spans the window on desktop, iOS and Android (VAPP-253).

## 0.16.0 (2026-11-05)

- Released with the SDK at 0.16.0: the records store moves behind a `RecordStore` handle (VAPP-212; **breaking** for the Swift and Compose `RecordsQuery.value(rows…)` and the FFI `records_value_json`), RecordForm date-time pickers and an owner's Author field (VAPP-245), and text that binds in Dialog, Alert and Chip (VAPP-206).

## 0.15.4 (2026-11-05)

- The open libraries are the Exponential SDK: genUI, Motion and Apps name the skills and docs (`exponential-ui` skill = `exponential-genui`; docs under /docs/sdk/); package names and tags stay. The amber warning colour is `semantic.yellow` (EXP-1695).

## 0.15.3 (2026-11-05)

- The icon list follows the app: `thread-stories` is gone, `thread-results` and `nav-needs-you` are new (FEED-191).

## 0.15.2 (2026-11-04)

- A `Button` in a `Dialog`, `Drawer` or `Popover` `trigger` slot opens the overlay; Markdown text asks `textLinks` (`#IDENT` links).

## 0.15.1 (2026-11-04)

- No changes of its own; released with the SDK (EXP-1691's fixes landed in the core).

## 0.15.0 (2026-11-04)

- No changes of its own; released with the SDK (format 4 and in-app links landed in the core).

## 0.14.0 (2026-11-04)

- `Row.href` / `Button.href` open the URL; the record and calendar macros render (EXP-1674, VAPP-217, VAPP-232).

## 0.13.0 (2026-11-03)

- Switch gains `size` (`default` | `compact`): the compact switch is a 28×16 track under a 12px thumb, as on the other renderers (#1853).

## 0.12.0 (2026-11-03)

- No changes of its own; released with the SDK (U1 screens, R8 client migrations and R7 components() landed in the core).

## 0.11.0 (2026-11-03)

- No changes of its own; released with the SDK (R6 transitions and R8 record migrations landed in the core).

## 0.10.0 (2026-11-03)

- No changes of its own; released with the SDK (refs + expand, history and files landed in the core).

## 0.9.0 (2026-11-03)

- No changes of its own; released with the SDK (computed fields and the sorted install issues landed in the core).

## 0.8.0 (2026-11-03)

- VAPP-200: `<ExponentialSurface readOnly>` (and `HostSurface`): every writing control paints disabled (`readOnlyTree` over the root and the lifted templates). Conformance suite `read-only`.

## 0.7.1 (2026-11-03)

- No changes of its own; released with the SDK (#1742's specimen look parity and #1744's neutral example names).

## 0.7.0 (2026-11-03)

- VAPP-160: `VoiceWave` (native and primitive) stops animating while it is off screen or in a hidden container (its frame clock observes the wave's SVG). No visual change.

## 0.6.0 (2026-11-02)

- EXP-1624: a ZoomableImage whose picture has not loaded (pending or failed) keeps its size and draws a placeholder (the new `image` chrome glyph over the alt, at the picture's radius) and no zoom pill, instead of the browser's broken-image glyph.

## 0.5.0 (2026-11-02)

- No changes of its own; released with the SDK (the records query and expressions landed in the core).

## 0.4.0 (2026-11-02)

- EXP-1586: the `ZoomableImage` native (pinch / ⌘-wheel / double-click / + − 0 1 zoom, drag and scroll pan with rubber band and mouse momentum, THE zoom pill with its presets Menu, `zoom` and `tap` events, a bindable `scale`) and its `ZoomableImage` primitive, both on the shared zoom controller.
- EXP-1493: the Carousel `counter` pill (and the `CarouselCounter` primitive) sits at the track's top-END corner (mirrored under RTL), off the page's own heading.

## 0.3.0 (2026-10-31)

- Round 6 (EXP-1495): the Carousel `counter` pill and `thumbnails` strip (a thumbnail press fires `change {page}`); the `Outline` and `Mosaic` macros paint through Box/Text/Image; the primitives `Outline`, `Mosaic`, `CarouselCounter`, `CarouselThumbnails`.
- VAPP-180 (with the SDK): a package's `exposes` and `upstreams`, validated by the core.

## 0.2.1 (2026-10-30)

- Version-only bump with the SDK (eui-gpui 0.2.1: per-cluster font fallback, YO-30); unchanged from 0.2.0.

## 0.2.0 (2026-10-30)

- Renamed (EXP-1438): `@exponential_app/ui-react` is now `@exponential_app/eui-react`; 0.1.x under the old name is deprecated on npm.
- VAPP-162: round 5 paints: `QrCode` (the core's matrix, dark on a light plate), `QrScanner` (viewfinder chrome; the camera through the new `host.qrScanner.start(feed, formats, onValue) → stop` hook, `scanned {value}` once per distinct value, the placeholder without one), `VoiceWave` `variant: bars`, `Segmented` `orientation: vertical` (the rail) + the `count` badge, `Button` `hold` (holdStart/holdEnd/holdCancel, `holdingLabel`), TraceItem `done`/`skipped`, and the `ui-serif` family emitted as the CSS keyword; fallback glyphs for the round-5 concepts.
- NEW-77: `VoiceWave` paints (one SVG of three paths from the core, the recipe colours, rAF while it moves, reduced motion rests) with the child rule; the `VoiceWave` primitive in `@exponential_app/eui-react/primitives`.
- **Breaking:** `@exponential_app/eui-react/primitives` no longer exports the Exponential app's own components: `glass-rows` (`GlassGroup`, `GlassRow`, `ListRow`, …), `typeahead`, `composer`, `trace`, `pill` and `segmented-bar` moved to the app's `@exp/ui`, and `SegmentedControl` lost its `embedded` arm (the glass-group row). React's primitive set is now the same module set as Solid's; the compiled `styles.css` drops their classes (−10.6 KB).

## 0.1.1 (2026-10-23)

- Renamed from `@exponential-at/ui-react` to `@exponential_app/eui-react` (the npm scope is the `exponential_app` org); the peer `@exponential_app/eui` follows. No code change.

## 0.1.0 (2026-10-23)

First release.

- VAPP-152: the node sheet is painted in `<style>` batches (`nodeSheetBatches`): a streamed `updateComponents` appends one with its new nodes' rules and leaves the others' elements and text alone, so Chromium restyles the new card instead of the whole surface. `share` comes from `@exponential_app/eui/web` (one copy for React and Solid).

- VAPP-141, VAPP-130, VAPP-137, VAPP-139, VAPP-140: a change re-renders only the nodes it reaches. The surface context keeps its identity when only the data, the breakpoint, `data-xq` or the mode change (`SurfaceContextValue.data`/`breakpoint`/`xq`/`mode` are live getters, `subscribe` announces a change, `deriveContext` derives one); each node re-renders when a value its bindings read is new. A re-reduced tree shares its unchanged nodes, the two stylesheets are rewritten only when their text changes, a template whose keys all change mounts as one block, a windowed list reads its extents once per measurement. On the benchmark Mac: a streamed message 13.6 → 2.4 ms, a mode switch 32 → 7.5 ms, a resize 28 → 1.2 ms, `scrollToIndex` on 100,000 rows 28 → 3.8 ms, 10,000 template items 328 / 406 → 148 / 133 ms.
- VAPP-146: a Select option whose value is `""` is an ordinary option (it threw); `fixtures/interactions-round1.json` `select`.
- An Avatar without a picture paints its initials with the first frame.

- The primitives' class strings come from `@exponential_app/eui/web/primitives` (FEED-131), the one source the Solid twins share; the exports keep their names.
- New primitives: `table`, `command` (no cmdk) and `toast` (no sonner, on the toast stack contract).

- `ExponentialSurface` + `useSurface`: the reference renderer, painting with real CSS on shadcn/Radix at the frames the Rust core computes (geometry-locked to taffy).
- Runtime themes, light/dark/system mode, locale and time zone, built-in string overrides, and density and contrast settings.
- Host-owned inputs, overlays on Radix portals, extension registration and the host plugin API.
- Runs the full conformance suite in headless Chromium.
- Trace lines, the Text shimmer (reduced motion: plain) and CodeBlock's destructive/sm looks; an `ansi` block copies without its escapes.
- The stylesheet, bind pass and pure helpers come from `@exponential_app/eui/web` (re-exported under the same names), shared with every web renderer.
