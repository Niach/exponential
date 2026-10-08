# `exponential-ui-ffi` — the mobile facade

The UniFFI facade over [`exponential-ui`](../exponential-ui) for the SwiftUI
(VAPP-88) and Compose (VAPP-89) painters. It ships INSIDE the Swift and
Kotlin packages (an xcframework / an Android `.so` with the generated
bindings), so an embedder never builds Rust.

## The contract is coarse

- `Surface(surfaceId, catalogId, themeId?, mode)` once; A2UI messages in as
  JSON (`apply`), or `setNested` / `setComponents` / `setData`.
- `nodes()` once per `structureVersion`; `visuals()` / `visual(index)` for
  the resolved looks; `layout(measurer)` returns one flat frame list, the
  overlay `layers` (frames in surface coordinates, `placement` for anchored
  ones, `position` = `centered` or an edge) and the windowed `lists`.
- `Measurer` (foreign trait, implemented by the host): `measureIntrinsics`
  and `measureHeights`, each a BATCH; at most three crossings per pass
  (`FfiLayout.upcalls`). `layoutFixed(sizesJson, wrap)` is the geometry-test
  measure for suites.
- `event(index, name, payloadJson)` → `[FfiEvent {kind, json}]` with
  `action | openUrl | dataChanged | input | relayout`; `setOpen`,
  `scroll`, `setStates`, `setPressed`, `setTheme*`, `setMode`,
  `registerExtension`.
- Free functions for suites and hosts: `reduceSurfaceJson`,
  `reduceNestedJson`, `extensionErrors`, `loadThemeJson`, `themeIssuesJson`,
  `builtinThemeJson`, `resolveRecipeJson`, `controlGeometryJson`,
  `placeOverlay`, `jsonEqual` / `jsonDiff`, `benchTreeJson`.

## Build

```bash
bash apps/desktop/crates/exponential-ui-ffi/build-ios.sh       # xcframework + bindings/swift
bash apps/desktop/crates/exponential-ui-ffi/build-android.sh   # jniLibs (.so) + bindings/kotlin
bash apps/desktop/crates/exponential-ui-ffi/run-binding-tests.sh [swift|kotlin|all]
```

Device builds use the workspace's `mobile` profile (release + LTO + one
codegen unit + stripped). The generated binding sources under `bindings/`
are COMMITTED (the contract, reviewable without a toolchain); `out/` holds
the artefacts and is ignored. The suites under `tests/swift` and
`tests/kotlin` replay every shared fixture through the bindings against the
host dylib and drive a surface with a native `Measurer`; `tests/facade.rs`
does the same from Rust.

Kotlin needs JNA ≥ 5.12 (the generated code uses `com.sun.jna.internal.Cleaner`);
`run-binding-tests.sh` takes `JNA_JAR=…` and compiles with the
`kotlin-compiler-embeddable` jar from the gradle cache when no `kotlinc` is
installed (`KOTLINC=…` overrides).
