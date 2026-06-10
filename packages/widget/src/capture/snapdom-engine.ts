import { snapdom } from "@zumer/snapdom"
import type { CaptureEngine } from "./engine"

// PNG captures are transparent where nothing paints; fill with the page's
// own background so dark sites don't end up on white (and vice versa).
function pageBackgroundColor(): string {
  for (const el of [document.body, document.documentElement]) {
    const color = getComputedStyle(el).backgroundColor
    if (color && color !== `transparent` && color !== `rgba(0, 0, 0, 0)`) {
      return color
    }
  }
  return `#ffffff`
}

export const snapdomEngine: CaptureEngine = {
  name: `snapdom`,
  async capture({ excludeSelectors, keepNode, dpr }) {
    return await snapdom.toCanvas(document.body, {
      fast: true,
      dpr,
      embedFonts: true,
      backgroundColor: pageBackgroundColor(),
      exclude: excludeSelectors,
      excludeMode: `hide`,
      filter: keepNode,
      filterMode: `hide`,
    })
  },
}
