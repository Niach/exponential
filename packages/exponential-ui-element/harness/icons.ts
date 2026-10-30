// FEED-130: the harness's icon map (Solid's harness/icons.ts): every Lucide
// glyph by registry name plus the concept ids of `packages/icons/icons.json`
// (read at BUNDLE time; the SDK itself has no icon registry, the host passes
// one in through the element's `plugin`).

import * as lucide from "lucide-solid"
import registry from "../../icons/icons.json" with { type: "json" }
import type { IconComponent, IconMap } from "@exponential_app/eui-element"

const pascal = (name: string) => name.replace(/(^|-)([a-z0-9])/g, (_, __, c: string) => c.toUpperCase())
const semantic = (registry as { semantic: Record<string, string> }).semantic

export const harnessIcons: IconMap = (name: string): IconComponent | undefined => {
  const glyph = semantic[name] ?? name
  const hit = (lucide as unknown as Record<string, IconComponent | undefined>)[pascal(glyph)]
  return typeof hit === `function` ? hit : undefined
}
