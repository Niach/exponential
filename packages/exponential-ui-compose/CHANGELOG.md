# Changelog: `at.exponential:ui-compose` + `ui-compose-primitives`

All notable changes to this package. The SDK packages share one version and
release together from a `ui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## 0.1.0 (unreleased)

First release.

- `:ui-compose`: `SurfaceModel` + `ExponentialSurface` paint an A2UI surface at the frames the Rust core computes on Android 8+. The AAR bundles the UniFFI facade (arm64-v8a, armeabi-v7a, x86_64) and its Kotlin binding.
- `:ui-compose-primitives`: generic Compose primitives themed by `PrimitiveTokens`.
- Host-owned text fields set the Input `type`'s keyboard, autocorrection and autofill hints (email, password, phone).
- Material sliders keep the author's range. `step` 0 is continuous; a step that divides the range is passed as M3 `steps`.
- An extension the core refuses fails `SurfaceModel` creation; it is no longer skipped silently.
- Robolectric unit, conformance and snapshot suites, plus instrumented tests (`src/androidTest`) that run on an emulator in CI.
