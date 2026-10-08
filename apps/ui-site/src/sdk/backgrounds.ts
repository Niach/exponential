/* Each built-in theme's page ground per mode (its $color.background), so a
   stage behind a live surface wears the theme's own background. */
import type { CSSProperties } from "react"
import type { ThemesDoc } from "../lib/catalog"

export type Backgrounds = Record<string, { light: string; dark: string }>

export function themeBackgrounds(doc: Pick<ThemesDoc, `tokens`>): Backgrounds {
  const row = doc.tokens.color.find((t) => t.name === `background`)
  return (row?.values ?? {}) as Backgrounds
}

/** Both grounds as CSS variables: the stylesheet picks one per mode class
 *  (`is-auto` = by <html data-scheme>), so the prerender already paints the
 *  visitor's scheme. */
export const groundVars = (ground: { light: string; dark: string } | undefined): CSSProperties | undefined =>
  ground ? ({ "--ground-light": ground.light, "--ground-dark": ground.dark } as CSSProperties) : undefined
