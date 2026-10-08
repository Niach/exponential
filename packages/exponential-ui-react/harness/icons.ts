// VAPP-87: the harness's icon map — every Lucide glyph by registry name plus
// the concept ids of `packages/icons/icons.json` (read at BUNDLE time; the
// SDK itself has no icon registry, the host passes one in). The custom and
// imported glyphs of the app's registry are app-side and fall back to the
// placeholder here.

import * as lucide from "lucide-react"
import registry from "../../icons/icons.json"
import type { IconComponent, IconMap } from "../src/host"

const pascal = (name: string) => name.replace(/(^|-)([a-z0-9])/g, (_, __, c: string) => c.toUpperCase())
const semantic = (registry as { semantic: Record<string, string> }).semantic

export const harnessIcons: IconMap = (name: string): IconComponent | undefined => {
  const glyph = semantic[name] ?? name
  const hit = (lucide as unknown as Record<string, IconComponent | undefined>)[pascal(glyph)]
  return typeof hit === `function` || (typeof hit === `object` && hit !== null) ? hit : undefined
}
