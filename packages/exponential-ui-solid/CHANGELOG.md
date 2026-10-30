# Changelog: `@exponential_app/ui-solid`

All notable changes to this package. The SDK packages share one version and
release together from a `ui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## Unreleased

- VAPP-162: round 5 paints like the React renderer (parity-gated): `QrCode` (the core's matrix, dark on a light plate), `QrScanner` (viewfinder chrome; the camera through the `host.qrScanner.start(feed, formats, onValue) → stop` hook, `scanned {value}` once per distinct value, the placeholder without one), `VoiceWave` `variant: bars`, `Segmented` `orientation: vertical` (the rail) + the `count` badge, `Button` `hold` (holdStart/holdEnd/holdCancel, `holdingLabel`, replaying `interactions-round1.json buttonHold`); fallback glyphs for the round-5 concepts.
- NEW-77: `VoiceWave` paints like the React renderer (parity-gated), with the child rule; the `VoiceWave` primitive in `@exponential_app/ui-solid/primitives`.

## 0.1.1 (2026-10-23)

- Renamed from `@exponential-at/ui-solid` to `@exponential_app/ui-solid` (the npm scope is the `exponential_app` org); the peer `@exponential_app/ui` follows. No code change.

## 0.1.0 (2026-10-23)

First release (FEED-128).

- VAPP-146: picking the Select option that holds the current value fires nothing when that value is `""` (it fired a change); `fixtures/interactions-round1.json` `select` replays against React.
- VAPP-147: the natives read the shared cached `nativeRecipeProps` (the package's own copy is gone).
- VAPP-152: a streamed `updateComponents` re-binds only the nodes it changed: the new tree shares its unchanged nodes with the last (`share`), each listed node follows its own id, and a binding reads the data model only where it evaluates one (a data write no longer re-binds every node). The node sheet is painted in `<style>` batches (`nodeSheetBatches`). Streaming `applyMs` 13.8 → 2.4 ms on the Linux bench machine.
- VAPP-148: `scrollToIndex` reuses the measured extents (one pass over the rows per measurement, React's VAPP-139); list-100k `scrollToIndexMs` 71.8 → 8.9 ms on the Linux bench machine.

- VAPP-106: the package's entries and its stylesheet are measured by `packages/exponential-ui/release/size.ts` (`release/sizes.json` is the record, `release/size-budget.json` the budget CI fails on); its bundles carry neither the described catalog nor a fixture.

- `@exponential_app/ui-solid/primitives` (FEED-131): the React primitive set on Solid with the same DOM and classes, every module but the six app-specific ones (glass-rows, typeahead, composer, trace, pill, segmented-bar). It includes `table`, `command`, `toast` and the calendar (react-day-picker v9's DOM). Gated against React by the web-parity check.
- `@exponential_app/ui-solid/styles.css`: the compiled primitives stylesheet, byte-identical to `@exponential_app/ui-react/styles.css`.
- The Radix machinery (layers, focus scope, popper, presence, menu, roving focus, listbox) is one internal layer the natives and the primitives share. `@kobalte/core` is no longer a dependency.

- `ExponentialSurface` + `createSurface`: the SolidJS renderer, painting the same DOM as the React renderer (`.xui-surface`, `data-xui-id`, the `xui-<Component>-<part>` classes) with the SAME compiled stylesheet (`@exponential_app/ui/web`), with Radix's DOM for overlays, menus, selects, tabs, accordions and the form controls.
- Runtime themes, light/dark/system mode, locale and time zone, string overrides, density and contrast; host-owned inputs, extension registration and the host plugin API; `HostSurface` + `hostPlugin` for the host API.
- Runs the full conformance suite in headless Chromium and the deterministic web-parity check against the React renderer (same DOM, same computed styles, same frames, zero differing pixels).
