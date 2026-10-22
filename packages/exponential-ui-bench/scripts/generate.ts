#!/usr/bin/env bun
// VAPP-105: writes scenarios/manifest.json and scenarios/<id>.json from
// src/scenarios.ts. `bun run --filter @exponential-at/ui-bench generate`;
// src/generate.test.ts fails on committed drift.
import { mkdirSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { renderScenarios } from "../src/scenarios"

const root = join(import.meta.dir, `..`)
for (const [file, content] of Object.entries(renderScenarios())) {
  const path = join(root, file)
  mkdirSync(dirname(path), { recursive: true })
  writeFileSync(path, content)
  console.log(`wrote ${file} (${content.length.toLocaleString(`en-US`)} chars)`)
}
