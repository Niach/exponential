// VAPP-87: the app's icon registry as the Exponential UI host icon map. The
// SDK never imports `@exp/icons`; the host hands it the generated Lucide
// components by concept id OR raw registry name (both are what the catalog's
// `Icon` prop accepts, `ICON_NAMES`), so `Icon { name: "nav-search" }` paints
// the same glyph the app paints.

import { ICON_COMPONENTS } from "@exp/ui"
import { SEMANTIC_ICONS, type IconConcept, type IconName } from "@exp/icons"
import type { IconMap } from "@exponential-at/ui-react"

export const exponentialUiIcons: IconMap = (name: string) => {
  const concept = SEMANTIC_ICONS[name as IconConcept] as IconName | undefined
  return ICON_COMPONENTS[(concept ?? name) as IconName]
}
