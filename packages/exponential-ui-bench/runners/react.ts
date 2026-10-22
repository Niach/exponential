#!/usr/bin/env bun
// VAPP-105: the React runner. Every scenario of scenarios/manifest.json in
// headless Chromium through the React harness's `bench-suite` view
// (packages/exponential-ui-react/harness/main.tsx, `window.__xuiBenchSuite`);
// runners/web-shared.ts drives it. A pass = the change applied inside
// flushSync and the layout read back on the main thread.
//
//   bun run --filter @exponential-at/ui-bench bench:react [--scenario <id>]… [--repeat <n>]
//
// Writes `{ renderer: "react-chromium", platform: "web", … }` to
// $EXPONENTIAL_UI_BENCH_OUT (default .bench/react-chromium.json).

import { bundleHarness } from "../../exponential-ui-react/harness/serve"
import { runWebBench } from "./web-shared"

await runWebBench({ renderer: `react-chromium`, bundle: bundleHarness })
