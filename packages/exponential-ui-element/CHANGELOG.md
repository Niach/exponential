# Changelog: `@exponential-at/ui-element`

All notable changes to this package. The SDK packages share one version and
release together from a `ui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## 0.1.0 (unreleased)

First release (FEED-130).

- VAPP-106: `.`, `./define` and `dist/standalone.js` are measured by `packages/exponential-ui/release/size.ts` (`release/sizes.json` the record, `release/size-budget.json` the budget CI fails on); `scripts/build-standalone.ts` exports `bundleStandalone(dir, { outDir })` for it, and the bundles carry neither the described catalog nor a fixture.

- `<exponential-surface>`: a custom element that paints a core A2UI surface in any page with no framework, by mounting the Solid renderer (`@exponential-at/ui-solid`) in its light DOM. The same DOM, the same compiled stylesheet and the same runtime themes as the React renderer; the element is `display: contents`, so `.xui-surface` is the box.
- Attributes `theme`, `mode`, `density`, `contrast`, `locale`, `time-zone`, `direction`, `width`, `height`, `surface-id`, and `src` + `post` + `transport` (JSONL, SSE or WebSocket through the host API). Properties for everything else: `surface`, `root`/`templates`/`data`, `host`, `plugin`, `extensions`, `strings`, `states`, `measure`, `viewportHeight`.
- `apply()`, `setData()`, `reset()` and the surface handle (`run`, `focus`, `announce`, `scrollIntoView`, `scrollToIndex`).
- Events: `xui-action`, `xui-input`, `xui-upload` (with `waitUntil`), `xui-function-call` (`respond`), `xui-open-url` (cancelable), `xui-paint-error`, `xui-theme-issues`, `xui-unknown`, `xui-status`, `xui-ready`.
- `defineElementExtension` for vanilla extension painters, `iconsFromSvg` for SVG markup icons, and `define()`, plus the `./define` and `./standalone` entries.
