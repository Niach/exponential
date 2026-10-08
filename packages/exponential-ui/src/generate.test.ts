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

  test(`renders the twenty-eight outputs`, () => {
    expect(Object.keys(files).sort()).toEqual([
      `../../apps/desktop/crates/exponential-ui/src/generated/catalog.rs`,
      `../../apps/desktop/crates/exponential-ui/src/generated/themes.rs`,
      `catalog/core.schema.json`,
      `catalog/theme.schema.json`,
      `docs/components.generated.json`,
      `docs/themes.generated.json`,
      `fixtures/bind-time.json`,
      `fixtures/catalog-basic-map.json`,
      `fixtures/catalog-components.json`,
      `fixtures/catalog-extension.json`,
      `fixtures/catalog-macros.json`,
      `fixtures/code-tokens.json`,
      `fixtures/control-geometry.json`,
      `fixtures/kitchen-sink.expanded.json`,
      `fixtures/prompt-budget.json`,
      `fixtures/style-conditions.json`,
      `fixtures/theme-extends.json`,
      `fixtures/theme-invalid.json`,
      `fixtures/theme-recipes.json`,
      `fixtures/theme-resolved.json`,
      `generated/ExponentialUICatalog.generated.kt`,
      `generated/ExponentialUICatalog.generated.swift`,
      `generated/ExponentialUIThemes.generated.kt`,
      `generated/ExponentialUIThemes.generated.swift`,
      `generated/catalog.generated.rs`,
      `generated/catalog.themes.generated.rs`,
      `src/catalog.generated.ts`,
      `themes/exponential.theme.json`,
    ])
  })

  for (const rel of Object.keys(files)) {
    test(`${rel} is up to date`, () => {
      expect(readFileSync(join(pkgRoot, rel), `utf8`)).toBe(files[rel])
    })
  }
})
