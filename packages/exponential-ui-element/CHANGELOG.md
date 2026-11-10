# Changelog: `@exponential_app/eui-element`

All notable changes to this package. The SDK packages share one version and
release together from a `eui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## Unreleased

- `IconPicker` + `ColorPicker` (#EXP-1760) through the Solid painter.
- Textarea `variant` + Input `bare` / `code` (#EXP-1629): through the React renderer.

## 0.21.0 (2026-11-09)

- Input `variant: "row"` (#EXP-1629): a Group row (label inside, the value end-aligned at 70%); the exponential theme's row is the IDE's (14 px rem). Through the Solid painter.
- Select `triggerVariant: row`, `align` and `popupWidth` (#EXP-1629) through the Solid painter.
- `KeyRecorder` (#EXP-1629) through the Solid painter.

## 0.20.0 (2026-11-08)

- `Window` (VAPP-290) through the Solid painter.

## 0.19.1 (2026-11-07)

- Released with the SDK at 0.19.1: the Swift and Compose hosts check an app's calls against its upgraded package, so a format-5 app's `records_upsert` is no longer refused silently (VAPP-272).

## 0.19.0 (2026-11-07)

- Round 8: Canvas and Checkbox `indeterminate` through the Solid painter.

## 0.18.0 (2026-11-06)

- Tree (#VAPP-285) through the Solid painter.
- A foldable Row (#VAPP-284) through the Solid painter: the fold keys and the nested fold button.
- `host.popupHost` (VAPP-281) through the Solid renderer.
- Popover `align` + `anchor`, Menu `side` + `align` + `anchor` through the Solid renderer (VAPP-276); the harness's overlay view builds the anchor-by-id cases.

## 0.17.0 (2026-11-06)

- Slider `icon`, `endLabel`, `showValue` (#VAPP-286): a leading glyph, short muted text after the track (bindable) and `showValue: false` (no value above the track); the core lays the volume row out as a `row` part (gap sm: icon iconSm, the growing track, the end label at least buttonIcon wide). New parts `row`, `icon`, `end`; prompt budget 9,216 -> 9,237 (a named exception). Painted through Solid.
- DatePicker `inline`, `marks`, `today` and the `month` event through the Solid renderer (VAPP-282).
- Tabs `variant: strip` (#VAPP-279) through the Solid renderer.
- `SettingsRow` (#VAPP-280, round 7): one line of a settings page as a catalog macro: the label (+ a muted description) on the start column, the control (its children) on the end column, 16 px in, 12 px top and bottom, at least 44 px tall; below `stackBelow` (default `sm`; `never` keeps it inline) the control goes under the label and stretches. A group named by the label. Every renderer paints it through the core's expansion; prompt budget 9,292 → 9,359 tokens (a named exception).
- Sidebar `rail` (#VAPP-277): paints the icon rail through Solid (logical tooltip sides, one tab stop per rail row).

## 0.16.3 (2026-11-06)

- Slider `icon`, `endLabel`, `showValue` (#VAPP-286): a leading glyph, short muted text after the track (bindable) and `showValue: false` (no value above the track); the core lays the volume row out as a `row` part (gap sm: icon iconSm, the growing track, the end label at least buttonIcon wide). New parts `row`, `icon`, `end`; prompt budget 9,216 -> 9,237 (a named exception). Painted through Solid.
- DatePicker `inline`, `marks`, `today` and the `month` event through the Solid renderer (VAPP-282).
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

- No changes of its own; released with the SDK.

## 0.15.1 (2026-11-04)

- No changes of its own; released with the SDK (EXP-1691's fixes landed in the core).

## 0.15.0 (2026-11-04)

- No changes of its own; released with the SDK (format 4 and in-app links landed in the core).

## 0.14.0 (2026-11-04)

- No changes of its own; released with the SDK (the U2 record macros, the U3 Calendar/Timeline macros and Row/Button href landed in the core).

## 0.13.0 (2026-11-03)

- No changes of its own; released with the SDK.

## 0.12.0 (2026-11-03)

- No changes of its own; released with the SDK (U1 screens, R8 client migrations and R7 components() landed in the core).

## 0.11.0 (2026-11-03)

- No changes of its own; released with the SDK (R6 transitions and R8 record migrations landed in the core).

## 0.10.0 (2026-11-03)

- No changes of its own; released with the SDK (refs + expand, history and files landed in the core).

## 0.9.0 (2026-11-03)

- No changes of its own; released with the SDK (computed fields and the sorted install issues landed in the core).

## 0.8.0 (2026-11-03)

- VAPP-200: the boolean `read-only` attribute / `readOnly` property: every writing control paints disabled. Conformance suite `read-only`.

## 0.7.1 (2026-11-03)

- No changes of its own; released with the SDK (#1742's specimen look parity and #1744's neutral example names).

## 0.7.0 (2026-11-03)

- No changes of its own; released with the SDK (eApps formatVersion 3 and per-kind record policies landed in the core).

## 0.6.0 (2026-11-02)

- No changes of its own; released with the SDK (eApps actions, package tests and optional types landed in the core; the ZoomableImage placeholder comes from the React and Solid renderers).

## 0.5.0 (2026-11-02)

- No changes of its own; released with the SDK (the records query and expressions landed in the core).

## 0.4.0 (2026-11-02)

- EXP-1586: the `ZoomableImage` native through the Solid painter (`zoom {scale, atFit}` and `tap` reach `xui-action`).
- EXP-1493: the Carousel `counter` pill sits at the track's top-END corner (mirrored under RTL), through the Solid painter.

## 0.3.0 (2026-10-31)

- Round 6 (EXP-1495), through the Solid painter: the Carousel `counter` pill and `thumbnails` strip (a thumbnail press reaches `xui-action` as `change {page}`), the `Outline` and `Mosaic` macros.
- VAPP-180 (with the SDK): a package's `exposes` and `upstreams`, validated by the core.

## 0.2.1 (2026-10-30)

- Version-only bump with the SDK (eui-gpui 0.2.1: per-cluster font fallback, YO-30); unchanged from 0.2.0.

## 0.2.0 (2026-10-30)

- Renamed (EXP-1438): `@exponential_app/ui-element` is now `@exponential_app/eui-element`; 0.1.x under the old name is deprecated on npm.
- VAPP-162: round 5 paints through the Solid painter (`QrCode`, `QrScanner`, `VoiceWave` bars, the vertical `Segmented` rail + counts, `Button` hold); the QrScanner's camera = `plugin.qrScanner.start(feed, formats, onValue) → stop`, a decoded value fires `scanned` (`xui-action`).
- NEW-77: `VoiceWave` paints through the Solid painter.

## 0.1.1 (2026-10-23)

- Renamed from `@exponential-at/ui-element` to `@exponential_app/eui-element` (the npm scope is the `exponential_app` org); the standalone bundle and the dependency on `@exponential_app/eui` follow. No code change.

## 0.1.0 (2026-10-23)

First release (FEED-130).

- VAPP-152 + VAPP-148: the Solid painter's streaming and `scrollToIndex` improvements apply unchanged behind the element.

- VAPP-107: `src` and `post` pass the page's URL policy (`plugin.urls`) before a transport is built (`decideTransport`): private, loopback and link-local hosts are denied unless `allowPrivate` opts them in (the page's own private origin is), `hosts` holds, `schemes: ["https"]` means https + wss only, `javascript:`/`file:`/`data:` never connect. A denied one builds no host and fetches nothing: `status` is `closed`, the new `statusDetail` says why. The standalone bundle carries the filter.
- VAPP-106: `.`, `./define` and `dist/standalone.js` are measured by `packages/exponential-ui/release/size.ts` (`release/sizes.json` the record, `release/size-budget.json` the budget CI fails on); `scripts/build-standalone.ts` exports `bundleStandalone(dir, { outDir })` for it, and the bundles carry neither the described catalog nor a fixture.

- `<exponential-surface>`: a custom element that paints a core A2UI surface in any page with no framework, by mounting the Solid renderer (`@exponential_app/eui-solid`) in its light DOM. The same DOM, the same compiled stylesheet and the same runtime themes as the React renderer; the element is `display: contents`, so `.xui-surface` is the box.
- Attributes `theme`, `mode`, `density`, `contrast`, `locale`, `time-zone`, `direction`, `width`, `height`, `surface-id`, and `src` + `post` + `transport` (JSONL, SSE or WebSocket through the host API). Properties for everything else: `surface`, `root`/`templates`/`data`, `host`, `plugin`, `extensions`, `strings`, `states`, `measure`, `viewportHeight`.
- `apply()`, `setData()`, `reset()` and the surface handle (`run`, `focus`, `announce`, `scrollIntoView`, `scrollToIndex`).
- Events: `xui-action`, `xui-input`, `xui-upload` (with `waitUntil`), `xui-function-call` (`respond`), `xui-open-url` (cancelable), `xui-paint-error`, `xui-theme-issues`, `xui-unknown`, `xui-status`, `xui-ready`.
- `defineElementExtension` for vanilla extension painters, `iconsFromSvg` for SVG markup icons, and `define()`, plus the `./define` and `./standalone` entries.
