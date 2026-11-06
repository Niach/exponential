// VAPP-270: the icons an export carries. The runner draws Lucide glyphs from
// `glyphs.json` beside index.html, never from a bundled icon set: the kit's
// `vite build` writes every Lucide glyph there, and `exponential bundle`
// prunes it to the names the package and the runner's shell use. A name
// missing from it paints genUI's missing-icon fallback.

import { createLucideIcon, type IconNode } from "lucide-react"
import type { IconComponent, IconMap } from "@exponential_app/eui-react"

/** `glyphs.json`: Lucide icon nodes by canonical name (no React `key`s), and
 *  the other names that draw one (Lucide's aliases, icons.json `semantic`). */
export interface GlyphFile {
  icons: Record<string, [string, Record<string, string>][]>
  alias: Record<string, string>
}

export const GLYPHS_FILE = `glyphs.json`

/** The painter's icon map over a glyph file. */
export function glyphMap(file: GlyphFile): IconMap {
  const made = new Map<string, IconComponent>()
  return (name: string) => {
    const glyph = file.alias[name] ?? name
    const node = Object.hasOwn(file.icons, glyph) ? file.icons[glyph] : undefined
    if (!node) return undefined
    let icon = made.get(glyph)
    if (!icon) {
      icon = createLucideIcon(glyph, node.map(([tag, attrs], i) => [tag, { ...attrs, key: String(i) }]) as IconNode) as IconComponent
      made.set(glyph, icon)
    }
    return icon
  }
}

/** The export's glyph file (beside index.html); none (every icon the fallback) when it is missing. */
export async function loadGlyphs(url: URL = new URL(GLYPHS_FILE, document.baseURI)): Promise<IconMap> {
  const file = await fetch(url).then((r) => (r.ok ? (r.json() as Promise<GlyphFile>) : null), () => null)
  return glyphMap(file ?? { icons: {}, alias: {} })
}
