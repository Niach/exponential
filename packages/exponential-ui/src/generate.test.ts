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

  test(`renders the forty-two outputs (round 2 adds seven fixtures, round 3 the tree guides, round 4 its contract)`, () => {
    expect(Object.keys(files).sort()).toEqual([
      `../../apps/desktop/crates/exponential-ui/src/generated/catalog.rs`,
      `../../apps/desktop/crates/exponential-ui/src/generated/themes.rs`,
      `catalog/core.schema.json`,
      `catalog/theme.schema.json`,
      `conformance/manifest.json`,
      `docs/components.generated.json`,
      `docs/themes.generated.json`,
      `fixtures/animations.json`,
      `fixtures/bench-list.json`,
      `fixtures/bind-time.json`,
      `fixtures/catalog-basic-map.json`,
      `fixtures/catalog-components.json`,
      `fixtures/catalog-extension.json`,
      `fixtures/catalog-macros.json`,
      `fixtures/code-tokens.json`,
      `fixtures/control-geometry.json`,
      `fixtures/format.json`,
      `fixtures/host-policy.json`,
      `fixtures/host-router.json`,
      `fixtures/host-transport.json`,
      `fixtures/kitchen-sink.expanded.json`,
      `fixtures/prompt-budget.json`,
      `fixtures/resizable.json`,
      `fixtures/round4-contract.json`,
      `fixtures/specimens.json`,
      `fixtures/style-conditions.json`,
      `fixtures/template-items.json`,
      `fixtures/text-direction.json`,
      `fixtures/theme-extends.json`,
      `fixtures/theme-invalid.json`,
      `fixtures/theme-recipes.json`,
      `fixtures/theme-resolved.json`,
      `fixtures/tree-guides.json`,
      `fixtures/virtual-list.json`,
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

describe(`core.schema.json actions (VAPP-99)`, () => {
  const schema = JSON.parse(readFileSync(join(pkgRoot, `catalog/core.schema.json`), `utf8`)) as { $defs: Record<string, Record<string, unknown>> }

  test(`an Action takes A2UI's functionCall beside the legacy function key`, () => {
    const action = schema.$defs.Action as { properties: Record<string, unknown>; anyOf: unknown[] }
    expect(action.properties.functionCall).toEqual({ $ref: `#/$defs/ActionFunctionCall` })
    expect(action.properties.function).toEqual({ $ref: `#/$defs/ActionFunctionCall` })
    expect(action.anyOf).toContainEqual({ required: [`functionCall`] })
  })

  test(`an action may call a host function; a value position stays catalog-only`, () => {
    const call = (schema.$defs.ActionFunctionCall as { properties: { call: Record<string, unknown> } }).properties.call
    expect(call.enum).toBeUndefined()
    expect(call.type).toBe(`string`)
    expect((schema.$defs.FunctionCall as { properties: { call: { enum: string[] } } }).properties.call.enum).toContain(`formatNumber`)
  })
})
