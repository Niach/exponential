# Changelog: `@exponential-at/ui-solid`

All notable changes to this package. The SDK packages share one version and
release together from a `ui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## 0.1.0 (unreleased)

First release (FEED-128).

- `ExponentialSurface` + `createSurface`: the SolidJS renderer, painting the same DOM as the React renderer (`.xui-surface`, `data-xui-id`, the `xui-<Component>-<part>` classes) with the SAME compiled stylesheet (`@exponential-at/ui/web`), on Kobalte for overlays, menus, selects, tabs, accordions and the form controls.
- Runtime themes, light/dark/system mode, locale and time zone, string overrides, density and contrast; host-owned inputs, extension registration and the host plugin API; `HostSurface` + `hostPlugin` for the host API.
- Runs the full conformance suite in headless Chromium and the deterministic web-parity check against the React renderer (same DOM, same computed styles, same frames, zero differing pixels).
