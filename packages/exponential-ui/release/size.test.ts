// VAPP-106: the root-entry split as a rule over a bundle's inputs: a
// renderer entry (the root, the host API, the web layer, both web
// renderers) carries neither the described catalog nor the conformance
// suite's fixtures; the prompt, schema and conformance entries may. The
// real bundles are checked by `size.ts npm` in CI (exponential-ui.yml).

import { describe, expect, test } from "bun:test"
import { RENDERER_ENTRIES, SPLIT_RULES, overBudget, splitViolations } from "./size"

const clean = [
  `packages/exponential-ui/src/index.ts`,
  `packages/exponential-ui/generated/core.runtime.json`,
  `packages/exponential-ui/catalog/macros.json`,
  `packages/exponential-ui/themes/exponential.theme.json`,
  `packages/exponential-ui/src/web/primitives/toast.ts`,
  `packages/exponential-ui-solid/src/natives/overlays.tsx`,
]

describe(`the root-entry split (VAPP-106)`, () => {
  test(`a renderer entry on runtime inputs is clean`, () => {
    for (const entry of RENDERER_ENTRIES) expect(splitViolations(entry, clean)).toEqual([])
  })

  test(`every rule names what it catches`, () => {
    const caught = [
      `packages/exponential-ui/generated/core.descriptions.json`,
      `packages/exponential-ui/src/catalog.described.ts`,
      `packages/exponential-ui/src/prompt.ts`,
      `packages/exponential-ui/src/schema.ts`,
      `packages/exponential-ui/vendor/a2ui/v0_9/basic_catalog.json`,
      `packages/exponential-ui/src/conformance.ts`,
      `packages/exponential-ui/conformance/manifest.json`,
      `packages/exponential-ui/fixtures/toast-stack.json`,
    ]
    expect(caught).toHaveLength(SPLIT_RULES.length)
    const problems = splitViolations(`@exponential-at/ui/web/primitives`, [...clean, ...caught])
    expect(problems).toHaveLength(caught.length)
    for (const rule of SPLIT_RULES) expect(problems.some((p) => p.includes(rule.name)), rule.name).toBe(true)
  })

  test(`the prompt, schema and conformance entries may carry what they are for`, () => {
    expect(splitViolations(`@exponential-at/ui/prompt`, [`packages/exponential-ui/generated/core.descriptions.json`])).toEqual([])
    expect(splitViolations(`@exponential-at/ui/schema`, [`packages/exponential-ui/vendor/a2ui/v0_9/basic_catalog.json`])).toEqual([])
    expect(splitViolations(`@exponential-at/ui/conformance`, [`packages/exponential-ui/fixtures/theme-recipes.json`])).toEqual([])
  })

  test(`the web layer and the Solid renderer are renderer entries`, () => {
    for (const e of [`@exponential-at/ui/web`, `@exponential-at/ui/web/primitives`, `@exponential-at/ui-solid/index`, `@exponential-at/ui-solid/primitives`]) expect(RENDERER_ENTRIES).toContain(e)
  })
})

describe(`the budget (VAPP-106)`, () => {
  test(`over on bytes or gzip fails, an entry without a budget is reported only`, () => {
    const budget = { "a/index": { bytes: 100, gzip: 50 }, "a/styles.css": { bytes: 10 } }
    expect(overBudget({ "a/index": { bytes: 100, gzip: 50 }, "a/styles.css": { bytes: 10 }, "b/index": { bytes: 10 ** 9 } }, budget)).toEqual([])
    expect(overBudget({ "a/index": { bytes: 101, gzip: 50 } }, budget)).toEqual([`a/index: 101 bytes, budget 100 (+1)`])
    expect(overBudget({ "a/index": { bytes: 90, gzip: 51 } }, budget)).toEqual([`a/index: 51 bytes gzipped, budget 50 (+1)`])
  })
})
