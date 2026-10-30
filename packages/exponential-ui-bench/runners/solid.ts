#!/usr/bin/env bun
// VAPP-105: the Solid runner. Every scenario of scenarios/manifest.json in
// headless Chromium through the Solid harness's `bench-suite` view
// (packages/exponential-ui-solid/harness/main.tsx, the SAME
// `window.__xuiBenchSuite` as React's); runners/web-shared.ts drives it. A
// pass = the signal writes applied synchronously (Solid has no scheduler)
// and the layout read back on the main thread. The harness bundles with
// Vite (Solid's JSX needs its compiler), ~10 s once per run.
//
//   bun run --filter @exponential_app/eui-bench bench:solid [--scenario <id>]… [--repeat <n>]
//
// Writes `{ renderer: "solid-chromium", platform: "web", … }` to
// $EXPONENTIAL_UI_BENCH_OUT (default .bench/solid-chromium.json).

import { bundleHarness } from "../../exponential-ui-solid/harness/serve"
import { runWebBench } from "./web-shared"

await runWebBench({ renderer: `solid-chromium`, bundle: bundleHarness })
