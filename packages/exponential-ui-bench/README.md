# @exponential-at/ui-bench

The Exponential UI benchmark suite (VAPP-105): one set of scenario fixtures,
one runner per renderer, the results committed per machine, a ratchet in CI,
and the Numbers page on ui.exponential.at rendered from the results.

## Layout

| path | what |
|---|---|
| `src/scenarios.ts` | the scenarios, hand-written: surface, data (literal or a `generate` spec), viewport, theme, steps, metrics with their kind and sentence |
| `scenarios/manifest.json`, `scenarios/<id>.json` | GENERATED from it (`bun run generate`; `src/generate.test.ts` gates drift). Every runner reads these |
| `runners/*.ts` | one bun script per renderer: `react` (Playwright Chromium through the React harness's `bench-suite` view), `core` and `gpui` (`cargo run --release --example bench_suite`), `swift` (`swift test -c release`), `swift-ios` (`xcodebuild test` on a Simulator or an attached iPhone), `compose-jvm` (Robolectric), `compose-android` (an emulator or device, the fragment read back from logcat) |
| `src/results.ts` | the result format: a runner writes one FRAGMENT `{renderer, platform, device?, date, scenarios: {<id>: {<metric>: n}}}` to `$EXPONENTIAL_UI_BENCH_OUT` (default `.bench/<renderer>.json` at the repo root) |
| `results/<machine>.json` | the committed record of one machine (every renderer that ran there), written by `bun run bench`; `results/index.ts` lists them for the site (generated) |
| `budgets.json` + `src/ratchet.ts` | the CI ratchet per machine × renderer × scenario × metric |
| `findings.json` | the weak points the numbers exposed, each with its follow-up issue (the Numbers page lists them) |

## Scenarios

| id | what | metrics |
|---|---|---|
| `cold-start` | the kitchen sink from a fresh process: catalog + theme parse, reduce, expand, bind, measure, lay out, paint | `coldStartMs`, `firstPaintMs`, `nodes`, `memoryMb`*, `allocations`* |
| `card-grid` | the 205-node card grid, warm passes alternating 390 ↔ 900 px | `warmPassMs`, `upcallsPerPass`*, `measureCallsPerPass`* |
| `list-100k` | `fixtures/bench-list.json`: 100,000 rows, 100 scroll steps, the jump to row 50,000 | `firstPaintMs`, `scrollStepMs`, `scrollToIndexMs`, `renderedItems`, `measureCallsPerStep`*, `ffiCallsPerStep`*, `ffiBytesPerStep`*, `memoryMb`* |
| `streaming` | 200 messages (20 / s for 10 s) alternating `updateComponents` (a transcript card) and `updateDataModel` (status) | `applyMs`, `p95ApplyMs`, `totalMs`, `nodes`, `ffiBytesTotal`* |
| `theme-switch` | the sink, theme cycled neutral → playful → exponential, mode flipped | `themeSwitchMs`, `modeSwitchMs`, `restyled`* |
| `resize` | the sink 390 ↔ 900 px × 100 | `resizeMs`, `upcallsPerResize`* |
| `deep-nesting` | a 48-level chain (the catalog's maxDepth) | `firstPaintMs`, `warmPassMs`, `depth` |
| `template-10k` | 10,000 template items in a plain column (maxTemplateItems), then the array replaced | `firstPaintMs`, `rebindMs`, `nodes` |
| `overlay` | a bound Dialog opened and closed 50 times | `openMs`, `closeMs` |
| `table-1k` | a Table over 1,000 bound rows, 20 scroll steps, the sort changed | `firstPaintMs`, `scrollStepMs`, `sortMs`, `renderedRows` |

\* optional: a renderer without the counter leaves it out. Counters live
behind a debug flag in each renderer (`SurfaceModel.benchCounters` in Swift
and Compose, the harness's probe on web, a counting allocator in the core's
example) and cost a release build nothing.

What one **pass** includes differs by renderer and the manifest says it
(`renderers[].pass`): the core lays out with a fixed fake measure; gpui draws
a full headless frame; React flushes the DOM update and reads the layout back
(no frame wait); SwiftUI and Compose run the model's pass (the core through
the facade, the real text measure, nodes and visuals read back) without a
body or composition. Compare a renderer with itself across scenarios first.

## Run

```bash
bun run --filter @exponential-at/ui-bench bench                       # every runner this machine can run
bun run --filter @exponential-at/ui-bench bench -- --renderers react,core --scenario list-100k --repeat 5
bun run --filter @exponential-at/ui-bench bench:react                 # one runner (also runners/<name>.ts directly)
bun run --filter @exponential-at/ui-bench machine                     # the id results are filed under
bun run --filter @exponential-at/ui-bench ratchet .bench/*.json       # the verdicts; --write records budgets
```

`bench` picks the runners: React and the core everywhere, gpui on Linux
(its headless text system), SwiftUI and the iPhone Simulator on macOS (an
attached iPhone makes it a device run), Compose on the JVM when an Android SDK
is around, Compose on a device when `adb devices` lists one. Every runner
takes `--scenario <id>` and `--repeat <n>` (default 3; the median per metric).
The fragments land in `.bench/`, the merged record in `results/<machine>.json`
(`EXPONENTIAL_UI_BENCH_MACHINE` names the machine; default = the CPU model +
the OS). Commit a rerun whole, never edit a number.

## The ratchet

`budgets.json` holds, per machine × renderer × scenario × metric, the number
CI accepts (`exponential-ui.yml`: each renderer's job runs its runner and
uploads the fragment; `bench` ratchets them all). The rules (`src/ratchet.ts`),
in the style of the Compose conformance budgets:

- a metric **worse** than its budget fails;
- a **count** (nodes, calls, bytes) **better** than its budget fails too, until
  the budget is lowered in the same change (counts are deterministic);
- a **timing** below `floor` × budget prints a warning asking to lower it (CI
  runners are noisy; a better timing alone never fails);
- a metric with **no budget** (a new machine, a new scenario) is a notice.

Record budgets after a green run from the `bench-results` artifact (or a
local `.bench/`): `ratchet .bench/*.json --write` puts timings at measured ×
`headroom` and counts exact. The CI machines are `github-ubuntu-latest`
(React, the core, gpui, Compose JVM and the emulator) and `github-macos-26`
(SwiftUI on macOS and the iPhone Simulator).
