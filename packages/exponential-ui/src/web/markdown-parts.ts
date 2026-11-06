// #VAPP-258 (catalog/recipes.json `native.Markdown`): what a web painter puts
// on each `Markdown/<part>` element so the theme's part recipes reach it: the
// theme scope class (a part rule selects `.scope.xui-Markdown-<part>`) and
// the painter-supplied recipe props as attributes: `density` (the host's,
// `document` when unset, so a theme always sees a concrete value), a
// heading's `level`, a chip's `chip` kind. React and Solid share it.

import { recipeStyle, type ResolvedTheme } from "@exponential_app/eui"
import { declarations, partClass, propAttribute } from "./theme-css"

export type MarkdownDensity = `document` | `chat`

/** A `Markdown/<part>` element's class + recipe attributes. */
export function markdownPartAttrs(scope: string, part: string, density: MarkdownDensity | undefined, extra?: Record<string, string | number>): { class: string; attrs: Record<string, string> } {
  const attrs: Record<string, string> = { "data-xui-part": `Markdown/${part}`, [propAttribute(false, `density`)]: density ?? `document` }
  if (extra) for (const [k, v] of Object.entries(extra)) attrs[propAttribute(false, k)] = String(v)
  return { class: scope ? `${scope} ${partClass(`Markdown`, part)}` : partClass(`Markdown`, part), attrs }
}

/** #VAPP-258: an inline chip's `Markdown/chip` / `Markdown/chipLabel` rules
 *  for its `chip` kind (and the density), as CSS declarations painted OVER
 *  the catalog Chip's look (Chip/root, Chip/label): a theme restyles the
 *  Markdown chip without touching every Chip. No theme, or no rule, = none. */
export function markdownChipOverlay(theme: ResolvedTheme | null | undefined, part: `chip` | `chipLabel`, kind: string, density: MarkdownDensity | undefined): [string, string][] {
  if (!theme?.recipes.Markdown?.[part]) return []
  const style = recipeStyle(theme, { component: `Markdown`, part, props: { chip: kind, density: density ?? `document` } })
  return Object.keys(style).length ? declarations(style as Record<string, unknown>, theme.fonts) : []
}
