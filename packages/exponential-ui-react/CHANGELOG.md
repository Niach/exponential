# Changelog: `@exponential-at/ui-react`

All notable changes to this package. The SDK packages share one version and
release together from a `ui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## 0.1.0 (unreleased)

First release.

- `ExponentialSurface` + `useSurface`: the reference renderer, painting with real CSS on shadcn/Radix at the frames the Rust core computes (geometry-locked to taffy).
- Runtime themes, light/dark/system mode, locale and time zone, built-in string overrides, and density and contrast settings.
- Host-owned inputs, overlays on Radix portals, extension registration and the host plugin API.
- Runs the full conformance suite in headless Chromium.
