#!/usr/bin/env bun
// VAPP-105: the web-component runner. Every scenario of
// scenarios/manifest.json in headless Chromium through the element harness's
// `bench-suite` view (packages/exponential-ui-element/harness/main.ts: one
// `<exponential-surface>` driven through its properties and methods, the
// SAME `window.__xuiBenchSuite` as React's and Solid's); runners/web-shared.ts
// drives it. A pass = the property writes applied synchronously (the Solid
// painter underneath) and the layout read back on the main thread. The
// harness bundles with Vite, ~10 s once per run.
//
//   bun run --filter @exponential-at/ui-bench bench:element [--scenario <id>]… [--repeat <n>]
//
// Writes `{ renderer: "element-chromium", platform: "web", … }` to
// $EXPONENTIAL_UI_BENCH_OUT (default .bench/element-chromium.json).

import { bundleHarness } from "../../exponential-ui-element/harness/serve"
import { runWebBench } from "./web-shared"

await runWebBench({ renderer: `element-chromium`, bundle: bundleHarness })
