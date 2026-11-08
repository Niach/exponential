// #VAPP-258 (catalog/recipes.json `native.Markdown`): what a web painter puts
// on each `Markdown/<part>` element so the theme's part recipes reach it: the
// theme scope class (a part rule selects `.scope.xui-Markdown-<part>`) and
// the painter-supplied recipe props as attributes: `density` (the host's,
// `document` when unset, so a theme always sees a concrete value), a
// heading's `level`, a chip's `chip` kind. React and Solid share it.

import { markdownInlineRecipes, recipeStyle, type ResolvedTheme } from "@exponential_app/eui"
import { declarations, partClass, propAttribute } from "./theme-css"

export type MarkdownDensity = `document` | `chat`

/** Round 9 §9.1: an inline-profile Markdown's paragraph type. */
export type MarkdownVariant = `body` | `caption`

/** A `Markdown/<part>` element's class + recipe attributes. `inline` = the
 *  inline profile's `variant`: a paragraph or heading ALSO answers to the
 *  `markdownInlineRecipes` it paints with (Text/root, Heading/root), under
 *  the document density so a host's `chat` rule cannot reach it. */
export function markdownPartAttrs(scope: string, part: string, density: MarkdownDensity | undefined, extra?: Record<string, string | number>, inline?: MarkdownVariant): { class: string; attrs: Record<string, string> } {
  const type = inline && (part === `paragraph` || part === `heading`) ? markdownInlineRecipes(part, { variant: inline, level: Number(extra?.level) }) : []
  const attrs: Record<string, string> = { "data-xui-part": `Markdown/${part}`, [propAttribute(false, `density`)]: (type.length ? undefined : density) ?? `document` }
  if (extra) for (const [k, v] of Object.entries(extra)) attrs[propAttribute(false, k)] = String(v)
  const classes = [scope, ...type.map((q) => partClass(q.component, q.part)), partClass(`Markdown`, part)].filter(Boolean)
  for (const q of type) for (const [k, v] of Object.entries(q.props ?? {})) attrs[propAttribute(q.component === `Heading`, k)] = String(v)
  return { class: classes.join(` `), attrs }
}

/** The host a Markdown block tree renders with: `markdownInline` carries the
 *  inline profile down the (host-passed) block renderers. */
export function markdownProfileHost<H extends object>(host: H, inline: MarkdownVariant | undefined): H & { markdownInline?: MarkdownVariant } {
  return inline ? { ...host, markdownInline: inline } : host
}

/** #VAPP-258: an inline chip's `Markdown/chip` / `chipLabel` / `chipIdent` rules
 *  for its `chip` kind (and the density), as CSS declarations painted OVER
 *  the catalog Chip's look (Chip/root, Chip/label): a theme restyles the
 *  Markdown chip without touching every Chip. No theme, or no rule, = none. */
export function markdownChipOverlay(theme: ResolvedTheme | null | undefined, part: `chip` | `chipLabel` | `chipIdent`, kind: string, density: MarkdownDensity | undefined): [string, string][] {
  if (!theme?.recipes.Markdown?.[part]) return []
  const style = recipeStyle(theme, { component: `Markdown`, part, props: { chip: kind, density: density ?? `document` } })
  return Object.keys(style).length ? declarations(style as Record<string, unknown>, theme.fonts) : []
}
