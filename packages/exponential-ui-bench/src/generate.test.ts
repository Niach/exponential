// VAPP-105: the drift gate. The scenario files are re-rendered in memory and
// compared with the committed ones; a change to src/scenarios.ts without
// `bun run --filter @exponential_app/eui-bench generate` fails here. Also the
// invariants every runner relies on.
import { describe, expect, it } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { reduceNested, reduceSurface, MAX_DEPTH, MAX_TEMPLATE_ITEMS } from "@exponential_app/eui"
import type { FlatComponent, NestedNode } from "@exponential_app/eui"
import { generateItems, renderScenarios, SCENARIOS, type GeneratedData } from "./scenarios"
import { resultsIndexSource } from "./results-index"

const root = join(import.meta.dir, `..`)

describe(`scenario fixtures (VAPP-105)`, () => {
  it(`are committed as generated`, () => {
    for (const [file, content] of Object.entries(renderScenarios())) {
      expect(readFileSync(join(root, file), `utf8`), file).toBe(content)
    }
  })

  it(`results/index.ts lists every committed results file`, () => {
    expect(readFileSync(join(root, `results/index.ts`), `utf8`)).toBe(resultsIndexSource())
  })

  it(`every surface reduces without issues`, () => {
    for (const s of SCENARIOS) {
      const r = `nested` in s.surface ? reduceNested(s.surface.nested as NestedNode, { catalogId: `https://ui.exponential.at/catalogs/core/v1` }) : reduceSurface(s.surface.components as FlatComponent[], { catalogId: `https://ui.exponential.at/catalogs/core/v1` })
      expect(r.issues.map((i) => `${i.id}: ${i.message}`), s.id).toEqual([])
      expect(r.root, s.id).not.toBeNull()
    }
  })

  it(`ids and metric ids are unique; each scenario names at least one required metric`, () => {
    expect(new Set(SCENARIOS.map((s) => s.id)).size).toBe(SCENARIOS.length)
    for (const s of SCENARIOS) {
      expect(new Set(s.metrics.map((m) => m.id)).size, s.id).toBe(s.metrics.length)
      expect(s.metrics.some((m) => !m.optional), s.id).toBe(true)
    }
  })

  it(`the generated data stays inside the catalog limits where it is instantiated at once`, () => {
    const tpl = SCENARIOS.find((s) => s.id === `template-10k`)!
    const spec = (tpl.data as GeneratedData).generate
    expect(spec.count).toBe(MAX_TEMPLATE_ITEMS)
    const items = generateItems(spec)
    expect(items[0]).toEqual({ id: `t0`, title: `Item 0 of ten thousand` })
    expect(items[9999].id).toBe(`t9999`)
    const list = SCENARIOS.find((s) => s.id === `list-100k`)!
    expect(generateItems((list.data as GeneratedData).generate)[98]).toEqual({ id: `r98`, title: `Row 98`, meta: `1` })
  })

  it(`the deep chain reaches maxDepth exactly`, () => {
    const deep = SCENARIOS.find((s) => s.id === `deep-nesting`)!
    const depthOf = (n: NestedNode): number => 1 + Math.max(0, ...(n.children ?? []).map(depthOf))
    expect(depthOf((deep.surface as { nested: NestedNode }).nested)).toBe(MAX_DEPTH)
  })

  it(`the stream holds 200 messages, alternating kinds, and ends with 100 cards`, () => {
    const s = SCENARIOS.find((s) => s.id === `streaming`)!
    const messages = s.steps.messages as Record<string, unknown>[]
    expect(messages.length).toBe(200)
    expect(messages.filter((m) => `updateComponents` in m).length).toBe(100)
    const last = messages[198] as { updateComponents: { components: FlatComponent[] } }
    expect((last.updateComponents.components[0].children as string[]).length).toBe(100)
  })
})
