// VAPP-85: the drift gate. Every generated file is re-rendered in memory and
// byte-compared with what is committed, so an edit to catalog/*.json without
// `bun run --filter @exponential-at/ui generate` fails here (the
// domain-contract / icons pattern).

import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { render } from "../scripts/generate"

const pkgRoot = join(import.meta.dir, `..`)

describe(`generated outputs (VAPP-85)`, () => {
  const files = render()

  test(`renders the twelve outputs`, () => {
    expect(Object.keys(files).sort()).toEqual([
      `catalog/core.schema.json`,
      `docs/components.generated.json`,
      `fixtures/catalog-basic-map.json`,
      `fixtures/catalog-components.json`,
      `fixtures/catalog-extension.json`,
      `fixtures/catalog-macros.json`,
      `fixtures/kitchen-sink.expanded.json`,
      `fixtures/prompt-budget.json`,
      `generated/ExponentialUICatalog.generated.kt`,
      `generated/ExponentialUICatalog.generated.swift`,
      `generated/catalog.generated.rs`,
      `src/catalog.generated.ts`,
    ])
  })

  for (const rel of Object.keys(files)) {
    test(`${rel} is up to date`, () => {
      expect(readFileSync(join(pkgRoot, rel), `utf8`)).toBe(files[rel])
    })
  }
})
