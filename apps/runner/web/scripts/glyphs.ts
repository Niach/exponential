// VAPP-270: the kit's `glyphs.json` (src/glyphs.ts), every Lucide glyph of
// lucide-react plus its aliases and icons.json's `semantic` names, read from
// lucide-react's per-icon modules at build time.

import { readdirSync, readFileSync } from "node:fs"
import { createRequire } from "node:module"
import { dirname, join } from "node:path"
import { pathToFileURL } from "node:url"
import registry from "@exp/icons/icons.json" with { type: "json" }

import type { GlyphFile } from "../src/glyphs"

const ALIAS = /^export \{ default \} from '\.\/([a-z0-9-]+)\.js';$/m

export async function lucideGlyphs(): Promise<GlyphFile> {
  const dir = join(dirname(createRequire(import.meta.url).resolve(`lucide-react/package.json`)), `dist`, `esm`, `icons`)
  const icons: GlyphFile[`icons`] = {}
  const lucideAlias: Record<string, string> = {}
  for (const name of readdirSync(dir).filter((n) => n.endsWith(`.js`) && n !== `index.js`).sort()) {
    const id = name.slice(0, -3)
    const source = readFileSync(join(dir, name), `utf8`)
    if (source.includes(`__iconNode`)) {
      const { __iconNode } = (await import(pathToFileURL(join(dir, name)).href)) as { __iconNode: [string, Record<string, string>][] }
      icons[id] = __iconNode.map(([tag, { key: _, ...attrs }]) => [tag, attrs])
    } else {
      const to = ALIAS.exec(source)?.[1]
      if (to) lucideAlias[id] = to
    }
  }
  const alias: Record<string, string> = {}
  for (const [from, to] of Object.entries(lucideAlias)) if (icons[to]) alias[from] = to
  for (const [from, to] of Object.entries((registry as { semantic: Record<string, string> }).semantic)) {
    const glyph = icons[to] ? to : lucideAlias[to]
    if (glyph && icons[glyph]) alias[from] = glyph
  }
  return { icons, alias: Object.fromEntries(Object.entries(alias).sort(([a], [b]) => (a < b ? -1 : 1))) }
}
