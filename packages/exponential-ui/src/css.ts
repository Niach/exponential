// VAPP-92: resolved style → CSS declarations. The ONE mapping web consumers
// share (the theme builder's preview now, the React renderer in VAPP-87): the
// Box style keys are CSS-named on purpose (the VAPP-4 "CSS equals taffy"
// rules), so most keys only change case; the few logical shorthands and the
// theme's value types (shadow layers, font families, easing curves,
// gradients) are handled here.

import { easingCss, shadowCss } from "./theme"
import { ANIMATIONS, animatesOpacity, animationCss, animationTiming } from "./animation"
import type { Easing, FontSpec, ResolvedStyle, ResolvedTheme, Shadow } from "./theme-types"

const UNITLESS = new Set([`flexGrow`, `flexShrink`, `opacity`, `fontWeight`, `aspectRatio`])
const SHORTHANDS: Record<string, string[]> = {
  paddingHorizontal: [`padding-left`, `padding-right`],
  paddingVertical: [`padding-top`, `padding-bottom`],
  paddingInlineStart: [`padding-inline-start`],
  paddingInlineEnd: [`padding-inline-end`],
  marginHorizontal: [`margin-left`, `margin-right`],
  marginVertical: [`margin-top`, `margin-bottom`],
  marginInlineStart: [`margin-inline-start`],
  marginInlineEnd: [`margin-inline-end`],
  insetInlineStart: [`inset-inline-start`],
  insetInlineEnd: [`inset-inline-end`],
  insetBlockStart: [`inset-block-start`],
  insetBlockEnd: [`inset-block-end`],
  backgroundGradient: [`background-image`],
  backdropBlur: [`backdrop-filter`],
}
/** Keys that are not declarations of their own (folded into another). */
const FOLDED = new Set([`transitionEasing`, `native`, `animationDuration`])

const kebab = (key: string) => key.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)

interface GradientValue {
  angle: number
  stops: { color: string; offset: number }[]
}

export function gradientCss(gradient: GradientValue): string {
  return `linear-gradient(${gradient.angle}deg, ${gradient.stops.map((s) => `${s.color} ${Math.round(s.offset * 10000) / 100}%`).join(`, `)})`
}

function cssValue(key: string, value: unknown, fonts: Record<string, FontSpec>, style: Record<string, unknown>): string | null {
  if (key === `boxShadow`) return Array.isArray(value) ? shadowCss(value as Shadow[]) : String(value)
  if (key === `fontFamily`) {
    const family = String(value)
    const fallback = fonts[family]?.fallback
    return fallback ? `"${family}", ${fallback}` : `"${family}"`
  }
  if (key === `gridTemplateAreas` && Array.isArray(value)) return value.map((row) => `"${row}"`).join(` `)
  if (key === `backgroundGradient`) return typeof value === `object` && value !== null ? gradientCss(value as GradientValue) : null
  if (key === `backdropBlur`) return typeof value === `number` ? `blur(${value}px)` : null
  if (key === `transition`) {
    if (typeof value !== `number`) return null
    const easing = style.transitionEasing
    return `all ${value}ms${Array.isArray(easing) ? ` ${easingCss(easing as Easing)}` : ``}`
  }
  if (typeof value === `number`) return UNITLESS.has(key) ? String(value) : `${value}px`
  if (typeof value === `boolean`) return null
  return String(value)
}

/** Flat declarations for one resolved style (condition blocks are the
 *  caller's: resolve them against the surface first). `native` is not CSS
 *  and is dropped; `transitionEasing` folds into `transition`; round 2:
 *  `backdropBlur` (px) → `backdrop-filter` (+ the -webkit- twin);
 *  `animation` (+ `animationDuration`, ms once resolved) → the `animation`
 *  shorthand over `@keyframes xui-<name>` (src/animation.ts keyframesCss)
 *  when `theme` is given (its motion tokens time it), else nothing; a set
 *  that animates opacity makes `opacity` = own × `var(--xui-a-opacity, 1)`
 *  (register it: ANIMATION_PROPERTIES_CSS). The caller drops `animation`
 *  under reduced motion. */
export function styleToCss(style: ResolvedStyle | Record<string, unknown>, fonts: Record<string, FontSpec> = {}, theme?: ResolvedTheme): Record<string, string> {
  const out: Record<string, string> = {}
  for (const [key, value] of Object.entries(style)) {
    if (value === undefined || FOLDED.has(key)) continue
    if (key === `animation`) {
      const timing = theme && typeof value === `string` ? animationTiming(value, theme) : null
      if (!timing) continue
      // `animationDuration` arrives resolved (a $motion token → ms): it
      // replaces the animation's own token, its factor kept.
      const duration = (style as Record<string, unknown>).animationDuration
      const durationMs = typeof duration === `number` ? duration * (ANIMATIONS[value as string]?.factor ?? 1) : timing.durationMs
      out.animation = animationCss(value as string, { ...timing, durationMs })
      continue
    }
    if (key.startsWith(`@`) || key.startsWith(`:`)) continue
    if (typeof value === `object` && value !== null && !Array.isArray(value) && key !== `backgroundGradient`) continue
    const css = cssValue(key, value, fonts, style)
    if (css === null) continue
    const targets = SHORTHANDS[key] ?? [kebab(key)]
    for (const target of targets) out[target] = css
  }
  // The animation's opacity MULTIPLIES the node's (style.json `animations`).
  if (out.animation && animatesOpacity(String(style.animation))) out.opacity = out.opacity ? `calc(${out.opacity} * var(--xui-a-opacity, 1))` : `var(--xui-a-opacity, 1)`
  if (out[`backdrop-filter`]) out[`-webkit-backdrop-filter`] = out[`backdrop-filter`]
  const anyBorder = [`border-width`, `border-top-width`, `border-right-width`, `border-bottom-width`, `border-left-width`].some((k) => out[k] && out[k] !== `0px`)
  if (anyBorder && !out[`border-style`]) out[`border-style`] = `solid`
  if (anyBorder && !out[`border-color`]) out[`border-color`] = `transparent`
  return out
}

/** The same as one `style=""` attribute string. */
export function styleAttribute(style: ResolvedStyle | Record<string, unknown>, fonts: Record<string, FontSpec> = {}): string {
  return Object.entries(styleToCss(style, fonts))
    .map(([k, v]) => `${k}:${v}`)
    .join(`;`)
}
