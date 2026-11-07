// VAPP-289: the CSS font stack alone, so a primitive (canvas.ts) names a
// family without pulling the theme compiler into `web/primitives`.

import type { FontSpec } from "@exponential_app/eui"

/** A CSS string literal: backslashes and quotes escaped, so a family name
 *  can never end the string early (`validateTheme` refuses them too; this
 *  mirrors the core's `css.ts` `cssString`). */
function cssString(text: string): string {
  return `"${text.replace(/\\/g, `\\\\`).replace(/"/g, `\\"`)}"`
}

/** The CSS generic font families (CSS Fonts 4): emitted bare, never quoted. */
const CSS_GENERIC_FAMILIES: ReadonlySet<string> = new Set([`serif`, `sans-serif`, `monospace`, `cursive`, `fantasy`, `system-ui`, `ui-serif`, `ui-sans-serif`, `ui-monospace`, `ui-rounded`, `math`, `emoji`, `fangsong`])

export function fontStack(family: string, fonts: Record<string, FontSpec>): string {
  const fallback = fonts[family]?.fallback
  // VAPP-162: a CSS generic family (`ui-serif`, `ui-monospace`…) stays a
  // keyword: quoted it would name a font no system has.
  const name = CSS_GENERIC_FAMILIES.has(family) ? family : cssString(family)
  return fallback ? `${name}, ${fallback}` : name
}
