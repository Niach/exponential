// VAPP-270: the runner's icons come from the export's glyphs.json, never a bundled set.

import { describe, expect, test } from "bun:test"

import { glyphMap, type GlyphFile } from "./glyphs"
import { lucideGlyphs } from "../scripts/glyphs"

const map = (file: GlyphFile) => glyphMap(file) as (name: string) => unknown

describe(`glyphs`, () => {
  const file: GlyphFile = { icons: { camera: [[`circle`, { cx: `12`, cy: `13`, r: `3` }]] }, alias: { "ui-photo": `camera` } }

  test(`a name draws its glyph or its alias's, one component per glyph`, () => {
    const icons = map(file)
    expect(icons(`camera`)).toBeDefined()
    expect(icons(`ui-photo`)).toBe(icons(`camera`))
  })

  test(`a name missing from the file is the painter's fallback`, () => {
    expect(map(file)(`star`)).toBeUndefined()
    expect(map(file)(`constructor`)).toBeUndefined()
  })

  test(`the kit's file: every Lucide glyph, its aliases and the registry's semantic names`, async () => {
    const all = await lucideGlyphs()
    expect(Object.keys(all.icons).length).toBeGreaterThan(1500)
    expect(all.icons[`square-pen`]?.[0]?.[0]).toBe(`path`)
    expect(all.alias[`edit`]).toBe(`square-pen`)
    expect(all.alias[`nav-search`]).toBe(`search`)
    for (const node of Object.values(all.icons)) for (const [, attrs] of node) expect(attrs).not.toHaveProperty(`key`)
  })
})
