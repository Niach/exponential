/* Each built-in theme's page ground per mode (its $color.background), so a
   stage behind a live surface wears the theme's own background. */
import type { ThemesDoc } from "../lib/catalog"

export type Backgrounds = Record<string, { light: string; dark: string }>

export function themeBackgrounds(doc: Pick<ThemesDoc, `tokens`>): Backgrounds {
  const row = doc.tokens.color.find((t) => t.name === `background`)
  return (row?.values ?? {}) as Backgrounds
}
