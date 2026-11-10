/* #EXP-1779: /sdk/genui/ is THE component catalogue. Every authorable
   catalog component is in it with a specimen, and its reference page's
   variants gallery holds every variant (no cap drops one). */
import { describe, expect, test } from "bun:test"
import catalog from "@exponential_app/eui/catalog/core.catalog.json"
import { CATALOGUE, COMPONENT_DOCS, specimenFor, variantsOf } from "../src/components/eui/components/specimen.ts"

const AUTHORABLE = Object.entries(catalog.components as Record<string, { hidden?: boolean }>)
  .filter(([, c]) => !c.hidden)
  .map(([name]) => name)
  .sort()

describe(`the genUI catalogue`, () => {
  test(`renders every authorable component, once`, () => {
    const shown = CATALOGUE.flatMap((g) => g.docs.map((d) => d.name))
    expect([...shown].sort()).toEqual(AUTHORABLE)
    expect(new Set(shown).size).toBe(shown.length)
  })

  test(`every component has a specimen to render`, () => {
    expect(COMPONENT_DOCS.filter((d) => !specimenFor(d)).map((d) => d.name)).toEqual([])
  })

  test(`every short enum option is a specimen case or a variant (no cap drops one)`, () => {
    const ENUM = /^[A-Za-z0-9_-]+(\s*\|\s*[A-Za-z0-9_-]+)+$/
    const missing: string[] = []
    for (const doc of COMPONENT_DOCS) {
      if (doc.group === `overlay`) continue
      const variants = variantsOf(doc)
      if (!variants.length) continue
      const shown = JSON.stringify(specimenFor(doc)) + JSON.stringify(variants.map((v) => v.node)) + JSON.stringify(doc.example)
      for (const p of doc.props as { name: string; type: string; default?: unknown }[]) {
        const t = String(p.type).trim()
        if (!ENUM.test(t) || !variants.some((v) => v.label.startsWith(`${p.name} · `))) continue
        for (const o of t.split(`|`).map((x) => x.trim())) if (o !== p.default && !shown.includes(`"${p.name}":"${o}"`) && !shown.includes(`${p.name} · ${o}`)) missing.push(`${doc.name}.${p.name}=${o}`)
      }
    }
    expect(missing).toEqual([])
  })
})
