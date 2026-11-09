// VAPP-85: the drift gate. Every generated file is re-rendered in memory and
// byte-compared with what is committed, so an edit to catalog/*.json without
// `bun run --filter @exponential_app/eui generate` fails here (the
// domain-contract / icons pattern).

import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { render } from "../scripts/generate"
import { isIdentifier } from "./ids"

const pkgRoot = join(import.meta.dir, `..`)

describe(`generated outputs (VAPP-85)`, () => {
  const files = render()

  test(`renders the fifty-five outputs (round 2 adds seven fixtures, round 3 the tree guides, round 4 its contract, EXP-1324 the text shimmer, VAPP-106 the runtime/descriptions split, NEW-77 the voice wave, VAPP-161 the package format, VAPP-162 the QR matrix, EXP-1586 the zoom maths, VAPP-189 the records query, VAPP-160 the comment-free style, VAPP-222 the record transitions, VAPP-233 the record migrations, the host runtime slice)`, () => {
    expect(Object.keys(files).sort()).toEqual([
      `../../apps/desktop/crates/eui-core/src/generated/catalog.rs`,
      `../../apps/desktop/crates/eui-core/src/generated/descriptions.rs`,
      `../../apps/desktop/crates/eui-core/src/generated/themes.rs`,
      `../../apps/desktop/crates/eui-core/src/generated/xid.rs`,
      `catalog/core-lite.schema.json`,
      `catalog/core.schema.json`,
      `catalog/theme.schema.json`,
      `conformance/manifest.json`,
      `docs/components.generated.json`,
      `docs/themes.generated.json`,
      `fixtures/a2ui10-conformance.json`,
      `fixtures/a2ui10-functions.json`,
      `fixtures/a2ui10-messages.json`,
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
      `fixtures/host-package.json`,
      `fixtures/host-policy.json`,
      `fixtures/host-router.json`,
      `fixtures/host-transport.json`,
      `fixtures/kitchen-sink.expanded.json`,
      `fixtures/prompt-budget.json`,
      `fixtures/qr-code.json`,
      `fixtures/record-migrations.json`,
      `fixtures/record-transitions.json`,
      `fixtures/records-query.json`,
      `fixtures/resizable.json`,
      `fixtures/round4-contract.json`,
      `fixtures/specimens.json`,
      `fixtures/style-conditions.json`,
      `fixtures/template-items.json`,
      `fixtures/text-direction.json`,
      `fixtures/text-shimmer.json`,
      `fixtures/theme-extends.json`,
      `fixtures/theme-invalid.json`,
      `fixtures/theme-recipes.json`,
      `fixtures/theme-resolved.json`,
      `fixtures/tree-guides.json`,
      `fixtures/tree.json`,
      `fixtures/virtual-list.json`,
      `fixtures/voice-wave.json`,
      `fixtures/zoom.json`,
      `generated/ExponentialUICatalog.generated.kt`,
      `generated/ExponentialUICatalog.generated.swift`,
      `generated/ExponentialUIThemes.generated.kt`,
      `generated/ExponentialUIThemes.generated.swift`,
      `generated/Xid.generated.kt`,
      `generated/Xid.generated.swift`,
      `generated/catalog.descriptions.generated.rs`,
      `generated/catalog.generated.rs`,
      `generated/catalog.themes.generated.rs`,
      `generated/core.descriptions.json`,
      `generated/core.runtime.json`,
      `generated/host.runtime.json`,
      `generated/style.runtime.json`,
      `generated/xid.rs`,
      `generated/xid.ts`,
      `src/catalog.generated.ts`,
      `themes/exponential.theme.json`,
    ])
  })

  for (const rel of Object.keys(files)) {
    test(`${rel} is up to date`, () => {
      expect(readFileSync(join(pkgRoot, rel), `utf8`)).toBe(files[rel])
    })
  }

  // VAPP-289 (round 9 §4): every component id the generator mints is an
  // identifier. A derived id (`<id>.<part>`, `<id>.<key>`, §4.3) is checked
  // by its authored head. a2ui10-messages.json keeps its non-identifier ids:
  // they are the refusal cases' inputs.
  test(`every generated fixture's component ids are identifiers`, () => {
    const bad: string[] = []
    const walk = (rel: string, v: unknown): void => {
      if (Array.isArray(v)) return v.forEach((x) => walk(rel, x))
      if (!v || typeof v !== `object`) return
      const o = v as Record<string, unknown>
      // A specimen entry's `id` is its view id (exponential-ui-<name>); its tree is `node`.
      const isEntry = rel === `fixtures/specimens.json` && `node` in o && `title` in o
      if (!isEntry && typeof o.id === `string` && typeof o.component === `string` && !isIdentifier(o.id.split(`.`)[0]!)) bad.push(`${rel}: ${o.id}`)
      for (const x of Object.values(o)) walk(rel, x)
    }
    for (const [rel, content] of Object.entries(files)) {
      if (rel.startsWith(`fixtures/`) && rel.endsWith(`.json`) && rel !== `fixtures/a2ui10-messages.json`) walk(rel, JSON.parse(content))
    }
    expect(bad).toEqual([])
  })
})

describe(`core.schema.json actions (VAPP-99, round 9)`, () => {
  const schema = JSON.parse(readFileSync(join(pkgRoot, `catalog/core.schema.json`), `utf8`)) as { $defs: Record<string, Record<string, unknown>>; functions: Record<string, unknown> }

  test(`an action takes A2UI's functionCall, an event, or both`, () => {
    const action = schema.$defs.EuiAction as { properties: Record<string, unknown>; anyOf: unknown[] }
    expect(Object.keys(action.properties)).toEqual([`event`, `functionCall`])
    expect(action.anyOf).toContainEqual({ required: [`functionCall`] })
  })

  test(`an action may call a host function; the catalog's function map stays catalog-only`, () => {
    const call = ((schema.$defs.EuiAction as { properties: { functionCall: { properties: Record<string, Record<string, unknown>> } } }).properties.functionCall.properties)[`@call`]
    expect(call.enum).toBeUndefined()
    expect(call.type).toBe(`string`)
    expect(Object.keys(schema.functions)).toContain(`formatNumber`)
    expect(Object.keys(schema.functions).some((n) => n.startsWith(`app_`))).toBe(false)
  })
})
