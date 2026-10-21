// FEED-128 (React's natives.test.ts): the painter map gated against the
// catalog — every `kind: native` component has a painter (the SAME keys as
// the React renderer), every `builtinIcons` glyph resolves without a host
// registry, every built-in string id the renderer asks for exists.

import { describe, expect, it } from "vitest"
import { readFileSync, readdirSync } from "node:fs"
import { join } from "node:path"
import { coreCatalog, DEFAULT_STRINGS } from "@exponential-at/ui"
import { NATIVES } from "./natives"
import { FALLBACK_ICONS } from "./icons"
import { Pending, PENDING_NATIVES } from "./natives/pending"

describe(`natives × catalog`, () => {
  const natives = Object.entries(coreCatalog.components)
    .filter(([, d]) => d.kind === `native`)
    .map(([n]) => n)
  it(`every catalog native has a painter`, () => {
    for (const name of natives) expect(NATIVES[name], name).toBeDefined()
  })
  it(`every painter is a catalog native`, () => {
    for (const name of Object.keys(NATIVES)) expect(natives, name).toContain(name)
  })
  it(`the same 48 keys as the React renderer's map`, () => {
    const react = readFileSync(join(import.meta.dirname, `../../exponential-ui-react/src/natives/index.ts`), `utf8`)
    const block = react.slice(react.indexOf(`export const NATIVES`), react.indexOf(`export const NATIVE_NAMES`))
    const keys = [...block.matchAll(/^\s+([A-Za-z]+):/gm)].map((m) => m[1])
    expect(Object.keys(NATIVES)).toEqual(keys)
    expect(keys.length).toBe(48)
  })
  it(`PENDING_NATIVES lists exactly the entries still painted by Pending`, () => {
    expect(Object.entries(NATIVES).filter(([, p]) => p === Pending).map(([n]) => n).sort()).toEqual([...PENDING_NATIVES].sort())
  })
  it(`every builtinIcons glyph has a registry-free fallback`, () => {
    for (const [slot, icon] of Object.entries(coreCatalog.builtinIcons as Record<string, string>)) {
      if (slot === `$comment`) continue
      expect(FALLBACK_ICONS[icon], `${slot} → ${icon}`).toBeDefined()
    }
  })
  it(`every ctx.t(…) id in the renderer is a catalog string`, () => {
    const dir = join(import.meta.dirname, `natives`)
    const files = [...readdirSync(dir).map((f) => join(dir, f)), join(import.meta.dirname, `form.tsx`)]
    const ids = new Set<string>()
    for (const f of files) for (const m of readFileSync(f, `utf8`).matchAll(/\bt\(`([a-zA-Z]+)`/g)) ids.add(m[1])
    expect(ids.size).toBeGreaterThan(1)
    for (const id of ids) expect(DEFAULT_STRINGS[id], id).toBeDefined()
  })
})
