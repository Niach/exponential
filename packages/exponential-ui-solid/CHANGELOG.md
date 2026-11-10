# Changelog: `@exponential_app/eui-solid`

All notable changes to this package. The SDK packages share one version and
release together from a `eui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## Unreleased

- Select `multiple` (#EXP-1629), React's: `joinTop` / `joinBottom` on a picked item next to another shown pick. A parity case (with and without search).
- Composer `layout: "flat"` (#EXP-1762), like React: the leading slot in its own bar before the field, one row.
- `ColorPicker` 28 px cells, `inline`, child trigger (#EXP-1775): macro data only (parity with React).
- Select `triggerIcon` + the own-colour marks (#EXP-1764), React's: the `valueIcon` part before a `row` trigger's value; `brand-claude` in its own orange.
- `IconPicker` + `ColorPicker` (#EXP-1760), React's: a pick closes the picker's Popover (`ClosePickerContext`); `pickerPick` replays here.
- Select `pill` trigger, `loading`, option `mono`, `searchPlaceholder`, the `searchIcon` part (#EXP-1765), React's (the mono `trailing` run aria-hidden; the pill search's clear button; `textAlign` on `empty`; `popperWrapper` gains `alignOffset`). Parity cases: the pill variants, its popup, pointer-open with no cursor, loading, mono + searchPlaceholder.
- Select option `description` (#EXP-1629), React's: the `itemIcon` glyph, `itemText` with `itemLabel` and the muted `itemDescription`. Parity cases for both popups.
- Icon `size: xl` (#EXP-1629): 28 px from the recipe (`$control.iconXl`), React's.
- Input `variant: "box"` + `size` (#EXP-1629): React's: no label part, `aria-label` on the field. Parity case (Input box sm, typing).
- Select option `trailing` + event `open` (#EXP-1629), React's: the popup row reads `label trailing`, `open` fires as the popup opens. Parity cases.
- Textarea `variant` + Input `bare` / `code` (#EXP-1629), React's: no label part, `aria-label` on the field. Parity case (Input code + Textarea row, typing).
- Select `description` (#EXP-1629), React's: the field hint, the row's `labels` + `control` columns, `aria-describedby`. Parity cases for both variants.
- Composer `leading` / `trailing` slots and image paste (#EDE-65 R): the slots sit in the controls row (leading at the start, trailing before send); a paste with image files fires `attach` {files} and calls `host.onUpload`, a paste with other files only is prevented. `composer-slots.test.tsx` replays `composerSlots` + `composerPaste`.
- Select (#EXP-1629, the IDE's multi picker), React's: the row's `button` part (its hover lent to the value, placeholder and chevron), a `multiple` popup's cursor on its first chosen option. Parity cases: multiple with a choice / with none.

## 0.21.0 (2026-11-09)

- Select `triggerVariant: row` (#EXP-1629), React's: the `label`, spacer, `value` and `chevron` parts, the popup hung off the value (start, 262 px, fitted into the window), `align` + `popupWidth`, every opening at the top with the cursor on the first enabled option (no longer scrolled to the chosen one), the list's bottom `fade`, item `hover` = the cursor, `selected` = chosen only. Parity cases for the row and the open popup.
- Recipes (#EXP-1629): `backgroundClip: padding-box`, `$shadow.popover` and an Avatar picture's `image` fill under its ring, through the shared theme CSS.
- `primitives/kbd` `Kbd`: the `accent` fill (the catalog's `Kbd` recipe; was `muted`), through the shared `KBD_CLASSES`.
- Input `variant: "row"` (#EXP-1629): a Group row (label inside, the value end-aligned at 70%); the exponential theme's row is the IDE's (14 px rem). Parity case for the row (typing, Enter).
- Embed (#EDE-65): a nested genUI surface painted in place, sandboxed: its own data model, its own surface id (`embedSurfaceId`, so inner ids never meet the host's), the host's theme, mode, strings, locale and presentation hooks only (`embedHost`); every inner event fires the Embed's `action` with `{name, context}`, an inner host function is denied, a bad `surface` paints the `embedInvalid` error Alert, a deep change of the surface remounts the card.
- Composer `menuKeys` (#EDE-65), React's: Up, Down, Enter, Tab and Escape without a modifier fire `key` {key} while it is on; an IME composition's keys stay the IME's.
- KeyRecorder field capture (#EXP-1629): a press while idle starts recording and focuses the field; while recording every key (Tab, Escape included) and mouse buttons 3-5 go to `key` with the canonical name and held flags, blur stops it, and a press does nothing.

## 0.20.0 (2026-11-08)

- Chart `valueFormat` (ticks, tooltip, value labels, summary) and `rows` + `categoryField` + series `field` (#FEED-205).
- The icon list follows the app (#EXP-1733): `nav-threads` is now `nav-chats`, `thread-results` is now `chat-results`.
- Popover, Menu (press and context) and Tooltip close once their anchor scrolls, moves or leaves the tree (#EXP-1739, `watchAnchor`).
- `Window` (VAPP-290) paints from the core macro (no per-renderer code).

## 0.19.1 (2026-11-07)

- Released with the SDK at 0.19.1: the Swift and Compose hosts check an app's calls against its upgraded package, so a format-5 app's `records_upsert` is no longer refused silently (VAPP-272).

## 0.19.0 (2026-11-07)

- Round 8: the `Canvas` native (DOM-parity with React) and Checkbox `indeterminate` (Radix's mixed DOM, the bubble input's `indeterminate`), the primitive Checkbox's dash.

## 0.18.0 (2026-11-06)

- A focused text field adopts an action's `set` of its bound value (#VAPP-288): `ctx.assignData` / `ctx.writeEpoch`, `createHostOwnedValue({epoch})`; echoes keep the typing rule.
- Tree (#VAPP-285): a Tree row keeps `role=treeitem` and takes the WAI-ARIA tree keys (`treeKeyTarget`).
- A foldable Row (#VAPP-284): a pressable Box with `on.toggle` and `aria-expanded` takes ArrowRight / ArrowLeft (`foldKey`, rtl-aware); a press target inside another fires only its own press.

## 0.17.0 (2026-11-06)

- DatePicker `inline` (the month grid in place), `marks` event dots, `today`, the `month` event; days carry the `isToday` / `outside` recipe props, parity-gated against React (VAPP-282).
- Tabs `variant: strip` (#VAPP-279), DOM parity with React; replays `tabsReorder`.
- `host.markdownOptions` (`breaks`, `density`, `selectable`, `streaming`), as React: the blocks render keyed by object, so a streamed text keeps its finished blocks' DOM.
- Markdown host hooks (`catalog/host.json` `markdown`): a `textLinks` chip paints the Chip look inline, `imageSize` / `onImagePress` size and press block images, `attachment` paints a one-link paragraph as the Video / AudioPlayer native or the FileUpload file row. `hostPlugin(host)` forwards the policy's hooks.
- `SettingsRow` (#VAPP-280, round 7): one line of a settings page as a catalog macro: the label (+ a muted description) on the start column, the control (its children) on the end column, 16 px in, 12 px top and bottom, at least 44 px tall; below `stackBelow` (default `sm`; `never` keeps it inline) the control goes under the label and stretches. A group named by the label. Every renderer paints it through the core's expansion; prompt budget 9,292 → 9,359 tokens (a named exception).
- Sidebar `rail` (#VAPP-277): paints the icon rail from the expanded tree; Tooltip resolves the logical sides `start`/`end` against the surface direction, and a Tooltip inside a focusable host (a pressable Row) adds no tab stop: the host's keyboard focus opens it.

## 0.16.3 (2026-11-06)

- Markdown `imageMaxHeight` is a host render option (`markdown.options`, a number or `null` = fit the width at the image's aspect, capped at the viewport; default 160), and Compose Markdown text follows the surface's font scale and folds at whole lines (`Markdown.foldHeight`).
- Slider `icon`, `endLabel`, `showValue` (#VAPP-286): a leading glyph, short muted text after the track (bindable) and `showValue: false` (no value above the track); the core lays the volume row out as a `row` part (gap sm: icon iconSm, the growing track, the end label at least buttonIcon wide). New parts `row`, `icon`, `end`; prompt budget 9,216 -> 9,237 (a named exception). Solid paints the row in the same DOM order.
- `host.popupHost` (VAPP-281), as React.
- Popover `align` + `anchor`, Menu `side` + `align` + `anchor` (VAPP-276): the popper's `align`; an `anchor`ed overlay is placed against that node, which gets the trigger's aria + toggle; Escape returns focus to it.
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

- VAPP-160: `VoiceWave` (native and primitive) stops animating while it is off screen or in a hidden container; the native resolves its box edge and part colours once per prop or theme change instead of every frame. No visual change.

## 0.6.0 (2026-11-02)

- EXP-1624: a ZoomableImage whose picture has not loaded (pending or failed) keeps its size and draws a placeholder (the new `image` chrome glyph over the alt, at the picture's radius) and no zoom pill, instead of the browser's broken-image glyph.

## 0.5.0 (2026-11-02)

- No changes of its own; released with the SDK (the records query and expressions landed in the core).

## 0.4.0 (2026-11-02)

- EXP-1586: the `ZoomableImage` native and primitive, DOM-identical to React (zoomable-image.parity cases); the Menu's press opening is `PressMenu`, shared with the percent button's presets Menu.
- EXP-1493: the Carousel `counter` pill (and the `CarouselCounter` primitive) sits at the track's top-END corner (mirrored under RTL), like React.

## 0.3.0 (2026-10-31)

- Round 6 (EXP-1495): the Carousel `counter` pill and `thumbnails` strip, DOM-identical to React (media.parity cases); the `Outline` and `Mosaic` macros; the primitive twins `Outline`, `Mosaic`, `CarouselCounter`, `CarouselThumbnails`.
- VAPP-180 (with the SDK): a package's `exposes` and `upstreams`, validated by the core.

## 0.2.1 (2026-10-30)

- Version-only bump with the SDK (eui-gpui 0.2.1: per-cluster font fallback, YO-30); unchanged from 0.2.0.

## 0.2.0 (2026-10-30)

- Renamed (EXP-1438): `@exponential_app/ui-solid` is now `@exponential_app/eui-solid`; 0.1.x under the old name is deprecated on npm.
- VAPP-162: round 5 paints like the React renderer (parity-gated): `QrCode` (the core's matrix, dark on a light plate), `QrScanner` (viewfinder chrome; the camera through the `host.qrScanner.start(feed, formats, onValue) → stop` hook, `scanned {value}` once per distinct value, the placeholder without one), `VoiceWave` `variant: bars`, `Segmented` `orientation: vertical` (the rail) + the `count` badge, `Button` `hold` (holdStart/holdEnd/holdCancel, `holdingLabel`, replaying `interactions-round1.json buttonHold`); fallback glyphs for the round-5 concepts.
- NEW-77: `VoiceWave` paints like the React renderer (parity-gated), with the child rule; the `VoiceWave` primitive in `@exponential_app/eui-solid/primitives`.

## 0.1.1 (2026-10-23)

- Renamed from `@exponential-at/ui-solid` to `@exponential_app/eui-solid` (the npm scope is the `exponential_app` org); the peer `@exponential_app/eui` follows. No code change.

## 0.1.0 (2026-10-23)

First release (FEED-128).

- VAPP-146: picking the Select option that holds the current value fires nothing when that value is `""` (it fired a change); `fixtures/interactions-round1.json` `select` replays against React.
- VAPP-147: the natives read the shared cached `nativeRecipeProps` (the package's own copy is gone).
- VAPP-152: a streamed `updateComponents` re-binds only the nodes it changed: the new tree shares its unchanged nodes with the last (`share`), each listed node follows its own id, and a binding reads the data model only where it evaluates one (a data write no longer re-binds every node). The node sheet is painted in `<style>` batches (`nodeSheetBatches`). Streaming `applyMs` 13.8 → 2.4 ms on the Linux bench machine.
- VAPP-148: `scrollToIndex` reuses the measured extents (one pass over the rows per measurement, React's VAPP-139); list-100k `scrollToIndexMs` 71.8 → 8.9 ms on the Linux bench machine.

- VAPP-106: the package's entries and its stylesheet are measured by `packages/exponential-ui/release/size.ts` (`release/sizes.json` is the record, `release/size-budget.json` the budget CI fails on); its bundles carry neither the described catalog nor a fixture.

- `@exponential_app/eui-solid/primitives` (FEED-131): the React primitive set on Solid with the same DOM and classes, every module but the six app-specific ones (glass-rows, typeahead, composer, trace, pill, segmented-bar). It includes `table`, `command`, `toast` and the calendar (react-day-picker v9's DOM). Gated against React by the web-parity check.
- `@exponential_app/eui-solid/styles.css`: the compiled primitives stylesheet, byte-identical to `@exponential_app/eui-react/styles.css`.
- The Radix machinery (layers, focus scope, popper, presence, menu, roving focus, listbox) is one internal layer the natives and the primitives share. `@kobalte/core` is no longer a dependency.

- `ExponentialSurface` + `createSurface`: the SolidJS renderer, painting the same DOM as the React renderer (`.xui-surface`, `data-xui-id`, the `xui-<Component>-<part>` classes) with the SAME compiled stylesheet (`@exponential_app/eui/web`), with Radix's DOM for overlays, menus, selects, tabs, accordions and the form controls.
- Runtime themes, light/dark/system mode, locale and time zone, string overrides, density and contrast; host-owned inputs, extension registration and the host plugin API; `HostSurface` + `hostPlugin` for the host API.
- Runs the full conformance suite in headless Chromium and the deterministic web-parity check against the React renderer (same DOM, same computed styles, same frames, zero differing pixels).
