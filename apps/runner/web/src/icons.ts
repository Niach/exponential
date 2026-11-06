// The icon map the painter takes: every Lucide glyph by registry name plus the
// concept ids of `packages/icons/icons.json` (the genUI React harness's map;
// the SDK has no icon registry of its own, the host passes one in).

import * as lucide from "lucide-react"
import registry from "@exp/icons/icons.json" with { type: "json" }
import type { IconComponent, IconMap } from "@exponential_app/eui-react"

const pascal = (name: string) => name.replace(/(^|-)([a-z0-9])/g, (_, __, c: string) => c.toUpperCase())
const semantic = (registry as { semantic: Record<string, string> }).semantic

export const runnerIcons: IconMap = (name: string): IconComponent | undefined => {
  const glyph = semantic[name] ?? name
  const hit = (lucide as unknown as Record<string, IconComponent | undefined>)[pascal(glyph)]
  return typeof hit === `function` || (typeof hit === `object` && hit !== null) ? hit : undefined
}
