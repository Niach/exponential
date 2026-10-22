# Exponential UI conformance

A renderer is **Exponential UI conformant** when its runner replays every
suite of [`manifest.json`](manifest.json) (GENERATED from the fixtures,
drift-gated) and the report passes:

```bash
bun run --filter @exponential-at/ui conformance:check path/to/report.json
# CONFORMANT  my-renderer (flutter 0.3.0): <n> cases passed in <m> suites
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
| `bind` | `bind-time.json` | expand, run the bind pass per dataset: bound tree, `set` presses, per-row slot cells |
| `style-conditions` | `style-conditions.json` | flatten the style per context |
| `code-tokens` | `code-tokens.json` | tokenize: the lines of tokens |
| `format` | `format.json` | format calls through the English fallback (`zoned`: at `offsetMinutes`), exact; display values. A platform Formatter may run too (U+202F, U+00A0 → space) |
| `template-items` | `template-items.json` | item and row keys, template instances + escaped suffixes, reduce with templates lifted (cycles reported, kept in place) |
| `text-direction` | `text-direction.json` | every node's direction and physical text alignment |
| `resizable` | `resizable.json` | panel sizes (normalize, drag, keys, extents, px → percent) within 1e-6 |
| `virtual-list` | `virtual-list.json` | the window, scrollToIndex (sectioned: data index → row), sections, the sticky header |
| `animations` | `animations.json` | timing + frames per theme within 1e-3; painted opacity = own × frame |

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
| `@exponential-at/ui-react` | `packages/exponential-ui-react/browser/suites.test.ts` | `bun run --filter @exponential-at/ui-react test:conformance` (headless Chromium) |
| `@exponential-at/ui-solid` | `packages/exponential-ui-solid/browser/suites.test.ts` | `bun run --filter @exponential-at/ui-solid test:conformance` (headless Chromium) |
| `@exponential-at/ui-element` | `packages/exponential-ui-element/browser/suites.test.ts` | `bun run --filter @exponential-at/ui-element test:conformance` (headless Chromium) |
| `exponential-ui-gpui` | `apps/desktop/crates/exponential-ui-gpui/tests/suites.rs` | `cargo test -p exponential-ui-gpui --test suites` |
| `ExponentialUI` (SwiftUI) | `packages/exponential-ui-swift/Tests/ExponentialUITests/ConformanceTests.swift` | `swift test` |
| `at.exponential:ui-compose` | `packages/exponential-ui-compose/ui-compose/src/test/…/ConformanceTest.kt` | `./gradlew :ui-compose:testDebugUnitTest` |

CI (`.github/workflows/exponential-ui.yml`) runs all six renderers plus the
core on every SDK pull request and fails unless every report is conformant;
the two non-React web renderers (Solid, the web component) must also pass
the web-parity gate against React (below).

Version 2 (round 2, `docs/round-2-contract.md` §8) added the last nine
suites: round 1's bind/style/code fixtures every runner already replayed,
and the round-2 contract fixtures.

## With the real-font harness (VAPP-98, round 1)

Round 1 (#VAPP-98) adds the other half to this directory: `run.ts` dumps the
cases of `fixtures/conformance-cases.json` from the web and desktop renderers
with the conformance fonts and compares them with
`fixtures/conformance-baseline.json` (geometry with real text, plus a reported
pixel step). The two halves do not overlap: this manifest locks the CONTRACT
every renderer replays case by case (no fonts involved), the harness locks
what real text does to it. Once both are on master, the harness's comparison
becomes suite `real-font` here (cases = its matrix, a renderer passes when its
dump has no divergence beyond `conformance-known.json`), so
`conformance:check` stays the one verdict. Runner files are named `suites`
(`browser/suites.test.ts`, gpui `tests/suites.rs`) beside the harness's own
`conformance` ones.

### The matrix

Every renderer runs the FULL cross product of `conformance-cases.json`
(`allCases` in `dump.ts`; the Rust, Swift and Kotlin runners build the same
keys `<fixture>/<theme>/<mode>/<width>/<direction>`), so adding a fixture, a
theme, a mode or a width there widens every runner at once:

| axis | values |
|---|---|
| fixtures | `kitchen-sink` (every component), `responsive` (the kitchen sink's responsive section, `layout-geometry-round1.json`), `dashboard` (`dashboard.json` + `dashboard.data.json`: an agent project dashboard, VAPP-103) |
| themes | the built-ins: `exponential`, `neutral`, `playful`, `orange` |
| modes | `dark`, `light` (a mode changes colours only by contract; the light cases lock that no theme mode or recipe moves a box) |
| widths | 390, 600, 900, 1280 |
| directions | `ltr`, `rtl` |

The web dump of the whole matrix takes well under a minute; the Swift and
Compose ratchets a few seconds each, so the matrix is not sampled.

### Recording

| what | where | command |
|---|---|---|
| the web baseline | any machine with Chromium | `bun run --filter @exponential-at/ui conformance -- --write-baseline --skip-desktop` (whole matrix only) |
| the SwiftUI ratchet | macOS | `EXPONENTIAL_UI_WRITE_FIXTURES=1 swift test --filter RealFont` (`packages/exponential-ui-swift`) |
| the Compose ratchet | ubuntu (CI verifies there) | `.github/workflows/record-compose-fixtures.yml`, artifact `compose-fixtures` (`EXP_UI_WRITE_FIXTURES=1 ./gradlew :ui-compose:testDebugUnitTest --tests '*RealFontConformanceTest'`) |
| the gpui ratchet | Linux only (`tests/conformance.rs` builds there) | the same workflow's `gpui` job, artifact `gpui-ratchet` (`EXP_UI_WRITE_FIXTURES=1 cargo test -p exponential-ui-gpui --test conformance`) |

A new case without a budget fails every ratchet, so a matrix change re-records
all four in the same change.

Round 2: the dump measures LAYOUT boxes (transforms and animations off,
`LAYOUT_ONLY_CSS`), skips inactive carousel pages (`data-xui-inactive`),
and every painted part carries `data-xui-id="<node id>.<part>[.<index or
row key>]"` + `data-xui-part` (catalog/recipes.json lists the parts), so the
web and gpui dumps cover the same nodes. Nunito and Fira Code are real
faces (`fonts/`, SIL OFL 1.1). `conformance-known.json` carries the
divergence decisions (`causes`, `rules`) beside the ratchet's counts.

## Shared browser runner

Every WEB renderer runs the manifest through ONE runner,
[`browser-suites.ts`](browser-suites.ts) `runBrowserSuites(options)`: the
pure suites through the core and the shared web layer
(`@exponential-at/ui/web`), the painted ones in headless Chromium against the
renderer's harness ([`browser-page.ts`](browser-page.ts): `bunHarness`,
`openPage`, `EXPONENTIAL_UI_CHROME` = an explicit Chromium executable). A
renderer plugs in with:

- **a harness**: a `serve.ts` that honours `PORT` and answers `/` with a page
  speaking the React harness's protocol (`?view=` kitchen-sink, geometry,
  overlay, catalog, conditions, tree, controls, specimen, mount, bench,
  round2; `theme`/`mode`/`rtl`/`width`/`echo`; the `window.__xui*` probes);
- **its logic**: `templateItems`, `itemInstance`, `runNodeAction` (the rest
  is shared).

The function returns `{report, failures}` (no vitest in the core); the
renderer's `browser/suites.test.ts` writes the report and asserts
`checkReport`, as React's does in about 30 lines.

## Web parity

A separate, web-only gate (not a manifest suite: the natives keep the
manifest's list): [`parity.ts`](parity.ts) `runParity({reference,
candidate})` loads two web renderers side by side in one headless Chromium
(software raster, `--font-render-hinting=none`, the conformance fonts,
device scale 1, reduced motion, animations and transitions off) and compares
every case EXACTLY:

| check | what |
|---|---|
| dom | the `.xui-surface` subtrees (tag, attributes by name, text); generated ids (`useId`, Radix, Kobalte `cl-N`) in id attributes → `#id-N` in first-seen order |
| style | the full computed style of every `[data-xui-id]` element (and every overlay part) |
| frame | their `getBoundingClientRect`, 0 px tolerance |
| pixels | full-page screenshots, pixelmatch threshold 0 (AA excluded): 0 differing pixels; native `<video>`/`<audio>` boxes masked (the browser's own spinner) |

Cases (390): the matrix through `?view=tree` (192), every specimen at 900 px
(75), the control specimens with `data-xs` forced to hover, focus, pressed,
checked, open, disabled, invalid on every part (112), the overlay cases open
(11). The report ([`parity-report.schema.json`](parity-report.schema.json))
keeps the FIRST difference of each kind per case; differing screenshots land
in `.conformance/parity/`.

```bash
bun run --filter @exponential-at/ui conformance:parity              # React vs React, the self-check (~3 min)
bun run --filter @exponential-at/ui conformance:parity -- --candidate packages/exponential-ui-solid/harness/serve.ts
bun run --filter @exponential-at/ui conformance:parity -- --candidate packages/exponential-ui-element/harness/serve.ts
bun run --filter @exponential-at/ui parity:check .conformance/exponential-ui-parity-react-react.json
bun run --filter @exponential-at/ui-solid test:parity               # Solid's gate calls runParity
bun run --filter @exponential-at/ui-element test:parity             # the web component's gate, the same call
```

## Listing

Once two platforms pass, the A2UI ecosystem's renderers page gets an entry
(a pull request to google/A2UI, a person's call: it is outward-facing):

> **Exponential UI** (React, SwiftUI, Jetpack Compose, gpui): native renderers
> of the A2UI v0.9 basic catalog plus an 81-component core catalog, runtime
> themes, extension catalogs and a host API (transports, functions, bindings,
> policy). Apache-2.0. https://ui.exponential.at
