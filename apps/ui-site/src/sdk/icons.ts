/* The icon map the site's surfaces get (the harness's and the builder's):
   every Lucide glyph by name plus the concept ids of the shared icon
   registry (packages/icons/icons.json `semantic`, read at bundle time). The
   SDK has no icon registry of its own; a host passes one in. Loaded only
   inside the islands (the whole Lucide set is the second-heaviest chunk). */
import * as lucide from "lucide-react"
import type { IconComponent, IconMap } from "@exponential-at/ui-react"
import registry from "../../../../packages/icons/icons.json"

const pascal = (name: string) => name.replace(/(^|-)([a-z0-9])/g, (_, __, c: string) => c.toUpperCase())
const semantic = (registry as { semantic: Record<string, string> }).semantic
const table = lucide as unknown as Record<string, IconComponent | undefined>

export const siteIcons: IconMap = (name: string): IconComponent | undefined => {
  const hit = table[pascal(semantic[name] ?? name)]
  return typeof hit === `function` || (typeof hit === `object` && hit !== null) ? hit : undefined
}
