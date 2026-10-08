# `exponential-ui-ffi` — the mobile facade

The UniFFI facade over [`exponential-ui`](../exponential-ui) for the SwiftUI
(VAPP-88) and Compose (VAPP-89) painters. It ships INSIDE the Swift and
Kotlin packages (an xcframework / an Android `.so` with the generated
bindings), so an embedder never builds Rust.

## The contract is coarse

- `Surface(surfaceId, catalogId, themeId?, mode)` once; A2UI messages in as
  JSON (`apply`), or `setNested` / `setComponents` / `setData`.
- `nodes()` once (every slot; removed ones as tombstones so
  `nodes()[i].index == i`), then patch with `FfiLayout.delta`: fetch
  `added` + `changed` through `nodesAt(indices)`, drop `removed`;
  `renumbered` = fetch everything. Indices are STABLE slots; paint order is
  the order of `frames`, not the index order.
- `visuals()` / `visual(index)` for the resolved looks (per-side borders,
  corner radii, gradients, transforms, transitions as ms + cubic bezier,
  text decoration/transform/style, letter spacing, visibility, pointer
  events, cursor, chart colours).
- `layout(measurer)` returns one flat frame list, the overlay `layers`
  (class `overlay|toast`, `modal`, `dismissible`, frames in surface
  coordinates, `placement` for anchored ones), the windowed `lists`, the
  scroll containers (`scrolls`: clamped offsets + content sizes; frames are
  UNSCROLLED), the open `toasts` (the host times them →
  `dismissToast(id)`), `direction` and `breakpoint`.
- `Measurer` (foreign trait, implemented by the host): `measureIntrinsics`
  (min/max content width, height, the first `baseline`) and
  `measureHeights`, each a BATCH; at most three crossings per pass
  (`FfiLayout.upcalls`). **Upcalls run without the surface locked**: a
  measurer may call `nodes()` / `visual()`; passes are serialized and a
  `layout` re-entered from inside its own measurer returns the previous
  result with `reentrant = true` instead of deadlocking; a measurer that
  THROWS ends that pass (the error reaches the caller) and the next
  `layout` starts clean. Snapshots taken mid-pass (`nodes()`) never rebuild
  under it: a tree change lands on the next pass. `layoutFixed(sizesJson,
  wrap)` is the geometry-test measure for suites.
- Settings: `setSettings(FfiSettings)` / `settings()` or one at a time:
  `setLocale`, `setStringsJson`, `setModeSetting(light|dark|system,
  systemDark)`, `setDensity`, `setContrast(normal|high|system,
  systemHigh)`, `setFontScale`, `setInsets(top, right, bottom, left)`,
  `setPointer(hover, reducedMotion)`; `effectiveThemeJson()` (density +
  contrast applied), `stringsJson()`.
- `event(index, name, payloadJson)` → `[FfiEvent {kind, json}]` with
  `action | openUrl | functionCall | dataChanged | input | focus | announce |
  copy | pickFiles | relayout | hoverTimer` (`functionCall` = `{componentId,
  name, args}`, a host function for the registry + `decideFunction` gate); `setOpen`, `scroll`, `scrollTo`, `setStates`
  (`hover`, `pressed`, `focus`, `focus-visible`, …), `setPressed`,
  `submitForm`, `dismissToast`, `commandJson` (`focus`, `announce`,
  `scrollIntoView`), `takeEvents` (events raised outside a call: a hover
  opening a tooltip, a hover-close timer, a live region), `hoverTimeout(owner)`
  (with `FfiSettings.hoverCloseMs > 0` — natives: 150 — leaving a hover
  card's trigger and content raises `hoverTimer {owner, delay_ms}`; call
  this when it fires: it closes unless the trigger or content is hovered
  again; with 0 it closes at once and the host delays the un-hover),
  `setTheme*`, `setMode`,
  `registerExtension`.
- `Theme` (VAPP-88): a RESOLVED theme as an object (`Theme.builtin(id)`,
  `Theme.load(json, parents?)`), shared by surfaces (`Surface.withTheme`,
  `setTheme`, `theme()`, `mode()`) and queried by painters per part
  without re-parsing JSON: `resolvePart(owner, part, ownerPropsJson,
  states, mode)` → `{visual, styleJson}` (the owner's recipe props are
  derived like the core does), `color(name, mode)`, `spacing`, `radius`,
  `control`, `typeSize`, `lineHeight`, `opacity`, `fontFamily(kind)`,
  `fontsJson`, `controlGeometry`. `FfiNode` carries `partStates` (the
  core's `selected` / `open` / `checked`) and `macroName`.
- Free functions for suites and hosts: `reduceSurfaceJson`,
  `reduceNestedJson`, `extensionErrors`, `loadThemeJson`, `themeIssuesJson`,
  `builtinThemeJson`, `resolveRecipeJson`, `controlGeometryJson`,
  `placeOverlay`, `jsonEqual` / `jsonDiff`, `benchTreeJson`; round 1: the
  BIND pass (`bindTreeJson`, `runActionJson`, `resolveDynamicJson`) so a
  native never re-implements it, `tokenizeCodeJson`, `chartExtentJson`,
  `niceTicksJson`, `weekStart`, `textDirection`, `stringTableJson`,
  `formatString`, `resolveConditionsJson`, `componentA11yJson`,
  `validateStyleJson`.

## Build

```bash
bash apps/desktop/crates/exponential-ui-ffi/generate-bindings.sh  # bindings/swift + bindings/kotlin from the host library (.dylib/.so)
bash apps/desktop/crates/exponential-ui-ffi/build-ios.sh          # xcframework + bindings/swift
bash apps/desktop/crates/exponential-ui-ffi/build-android.sh      # jniLibs (.so) + bindings/kotlin
bash apps/desktop/crates/exponential-ui-ffi/run-binding-tests.sh [swift|kotlin|all]
```

Device builds use the workspace's `mobile` profile (release + LTO + one
codegen unit + stripped); the xcframework also carries the host's macOS
slice so the Swift package tests and macOS embedders link it, and
`build-ios.sh` copies the artefact and the Swift binding into
`packages/exponential-ui-swift` (the painter package). The generated binding sources under `bindings/`
are COMMITTED (the contract, reviewable without a toolchain); `out/` holds
the artefacts and is ignored. The suites under `tests/swift` and
`tests/kotlin` replay every shared fixture through the bindings against the
host dylib and drive a surface with a native `Measurer`; `tests/facade.rs`
does the same from Rust.

Kotlin needs JNA ≥ 5.12 (the generated code uses `com.sun.jna.internal.Cleaner`);
`run-binding-tests.sh` takes `JNA_JAR=…` and compiles with the
`kotlin-compiler-embeddable` jar from the gradle cache when no `kotlinc` is
installed (`KOTLINC=…` overrides).
