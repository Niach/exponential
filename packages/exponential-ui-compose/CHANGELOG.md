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
- Video and AudioPlayer play their `src` through the media policy with Media3 ExoPlayer (`androidx.media3:media3-exoplayer` + `media3-ui` 1.11.1): poster and duration until the press, `autoplay` muted, a header-carrying or `data:` request fetched within `media.limits` into a cache file; a denied src loads nothing.
- Media redirects: each hop is re-policed (a hop off a rule's prefix loses its headers) and an https → http hop is refused.
- Video / AudioPlayer: http(s) streams with its headers after a policed redirect probe, pinned to the resolved origin (no byte cap, so authed media over 20 MB plays); only `data:` goes to a file, uncapped.
- Video / AudioPlayer: one open per player; a second press waits (spinner, control disabled), another src cancels it, a replaced player is released.
- `data:` media files are refcounted and deleted by the last player, at most 8 kept, the cache directory emptied on the first use per process.
- An `Image` without `alt` paints only the glyph (no English "image" label).
- Markdown inline parsing is linear on hostile input (unclosed destinations, bracket and emphasis floods); link labels nest at most 32 deep.
- Paint failures are keyed by component: a failed node paints empty, unreported, until ITS props change; it drops when it leaves the surface. The host dedupes per surface + component and forgets one when `updateComponents` names it.
- Markdown block images align to the top of their box.
- `SurfaceModel.snap` reaches `max` when the step does not divide the range.
- The example's `brand` theme loads with `ThemeHandle.loadOrDefault`; a unit test loads every shipped sample theme strictly.
- Robolectric unit, conformance and snapshot suites, plus instrumented tests (`src/androidTest`) that run on an emulator in CI.
