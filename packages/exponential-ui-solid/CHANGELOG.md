# Changelog: `@exponential-at/ui-solid`

All notable changes to this package. The SDK packages share one version and
release together from a `ui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## 0.1.0 (unreleased)

First release (FEED-128).

- VAPP-106: the package's entries and its stylesheet are measured by `packages/exponential-ui/release/size.ts` (`release/sizes.json` is the record, `release/size-budget.json` the budget CI fails on); its bundles carry neither the described catalog nor a fixture.

- `@exponential-at/ui-solid/primitives` (FEED-131): the React primitive set on Solid with the same DOM and classes, every module but the six app-specific ones (glass-rows, typeahead, composer, trace, pill, segmented-bar). It includes `table`, `command`, `toast` and the calendar (react-day-picker v9's DOM). Gated against React by the web-parity check.
- `@exponential-at/ui-solid/styles.css`: the compiled primitives stylesheet, byte-identical to `@exponential-at/ui-react/styles.css`.
- The Radix machinery (layers, focus scope, popper, presence, menu, roving focus, listbox) is one internal layer the natives and the primitives share. `@kobalte/core` is no longer a dependency.

- `ExponentialSurface` + `createSurface`: the SolidJS renderer, painting the same DOM as the React renderer (`.xui-surface`, `data-xui-id`, the `xui-<Component>-<part>` classes) with the SAME compiled stylesheet (`@exponential-at/ui/web`), with Radix's DOM for overlays, menus, selects, tabs, accordions and the form controls.
- Runtime themes, light/dark/system mode, locale and time zone, string overrides, density and contrast; host-owned inputs, extension registration and the host plugin API; `HostSurface` + `hostPlugin` for the host API.
- Runs the full conformance suite in headless Chromium and the deterministic web-parity check against the React renderer (same DOM, same computed styles, same frames, zero differing pixels).
