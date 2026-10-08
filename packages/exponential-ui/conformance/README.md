# Exponential UI conformance

A renderer is **Exponential UI conformant** when its runner replays every
suite of [`manifest.json`](manifest.json) (GENERATED from the fixtures,
drift-gated) and the report passes:

```bash
bun run --filter @exponential-at/ui conformance:check path/to/report.json
# CONFORMANT  my-renderer (flutter 0.3.0): 1008 cases passed in 15 suites
```

Every suite present, every case run (the manifest's `cases` count), nothing
failed or skipped. The fixtures under `../fixtures/` are the contract; the
TS reference (`../src/`) generated their `expected` values and the Rust core
replays them byte for byte.

## The suites

| suite | fixture | a runner does, per case |
|---|---|---|
| `catalog` | `catalog-components.json` | reduce the nested node (no issues), PAINT it: no crash, no `Unknown` |
| `macros` | `catalog-macros.json` | expand: the tree equals `expanded` |
| `basic-map` | `catalog-basic-map.json` | reduce the A2UI basic surface: tree + issues equal |
| `extension` | `catalog-extension.json` | register the extension, reduce: tree + issues equal |
| `theme-resolved` | `theme-resolved.json` | load each built-in theme: equal |
| `theme-recipes` | `theme-recipes.json` | resolve the part recipe per theme × mode × state: equal |
| `theme-extends` | `theme-extends.json` | load over the parents: chain + probes equal |
| `theme-invalid` | `theme-invalid.json` | validate: the same issues, never a crash |
| `control-geometry` | `control-geometry.json` | measure YOUR painted control: every fixed key within 0.5 (with `minHeight`, height is only a floor) |
| `layout` | `layout-geometry.json` | lay out with the fixed measure: frames exact (a CSS renderer: within 1 px) |
| `overlay` | `overlay-geometry.json` | place the overlay: side, flip, position within `tolerancePx` |
| `replay` | `kitchen-sink.json` | paint the sink per built-in theme × mode: the expanded tree, no `Unknown`, paint/a11y order = pre-order |
| `host-transport` | `host-transport.json` | JSONL / SSE chunks through ONE decoder, the MCP carrier: messages + issues equal |
| `host-policy` | `host-policy.json` | the function gate, combine, URL policy, media request, source URIs, catalog negotiation |
| `host-router` | `host-router.json` | package validation; message flows → the ops, in order |

`manifest.json` says how each count is derived (`unit`). A renderer built on
the Rust core (the facade, `exponential-ui-ffi`) gets the pure suites through
its bindings; one on the TS package gets them from `@exponential-at/ui`;
anything else implements `catalog/host.json` and the reducer itself.

## The report

[`report.schema.json`](report.schema.json):
`{renderer, platform, version, conformanceVersion, suites: {<id>: {cases,
passed, failed: [names], skipped?}}}`. Our runners write it to
`$EXPONENTIAL_UI_CONFORMANCE_REPORT` (default `<repo>/.conformance/<name>.json`):

| renderer | runner | command |
|---|---|---|
| `exponential-ui` (Rust core) | `apps/desktop/crates/exponential-ui/tests/conformance.rs` | `cargo test -p exponential-ui --test conformance` |
| `@exponential-at/ui-react` | `packages/exponential-ui-react/browser/conformance.test.ts` | `bun run --filter @exponential-at/ui-react test:conformance` (headless Chromium) |
| `exponential-ui-gpui` | `apps/desktop/crates/exponential-ui-gpui/tests/conformance.rs` | `cargo test -p exponential-ui-gpui --test conformance` |
| `ExponentialUI` (SwiftUI) | `packages/exponential-ui-swift/Tests/ExponentialUITests/ConformanceTests.swift` | `swift test` |
| `at.exponential:ui-compose` | `packages/exponential-ui-compose/ui-compose/src/test/…/ConformanceTest.kt` | `./gradlew :ui-compose:testDebugUnitTest` |

CI (`.github/workflows/exponential-ui.yml`) runs all four renderers plus the
core on every SDK pull request and fails unless every report is conformant.

## Listing

Once two platforms pass, the A2UI ecosystem's renderers page gets an entry
(a pull request to google/A2UI, a person's call: it is outward-facing):

> **Exponential UI** (React, SwiftUI, Jetpack Compose, gpui): native renderers
> of the A2UI v0.9 basic catalog plus a 61-component core catalog, runtime
> themes, extension catalogs and a host API (transports, functions, bindings,
> policy). Apache-2.0. https://ui.exponential.at
