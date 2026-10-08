// Round 2 (docs/round-2-contract.md §2, animation): the token-only
// `animation` style key. catalog/style.json `animations` holds the keyframe
// sets; this module turns one into what a painter needs at a moment —
// duration from the theme's motion tokens, per-segment easing, the channel
// values (opacity, translate, rotate, scale, the shimmer band) — and into
// CSS @keyframes. fixtures/animations.json locks the sampled frames. Pure.

import styleJson from "../catalog/style.json" with { type: "json" }
import { parseTokenRef } from "./catalog"
import type { Easing, ResolvedTheme } from "./theme-types"

export interface Keyframe {
  offset: number
  opacity?: number
  translateX?: number
  translateY?: number
  rotate?: number
  scale?: number
  band?: number
}

export interface AnimationDef {
  duration: string
  factor: number
  easing: string
  iterations: number | `infinite`
  rest: number
  keyframes: Keyframe[]
  band?: { color: string; stops: { offset: number; alpha: number }[] }
}

/** The keyframe sets by name (style.json `animations`, `$comment` aside). */
export const ANIMATIONS: Readonly<Record<string, AnimationDef>> = Object.fromEntries(Object.entries(styleJson.animations).filter(([k]) => !k.startsWith(`$`))) as unknown as Record<string, AnimationDef>
export const ANIMATION_NAMES: readonly string[] = Object.keys(ANIMATIONS)

/** One frame: every channel at its value (`band` null = no shimmer band). */
export interface AnimationFrame {
  opacity: number
  translateX: number
  translateY: number
  rotate: number
  scale: number
  band: number | null
}

export interface AnimationTiming {
  durationMs: number
  /** A cubic bezier or `linear`. */
  easing: Easing | `linear`
  iterations: number | `infinite`
}

const LINEAR: `linear` = `linear`

/** Duration and easing of an animation under a theme: the `duration` token
 *  (or `durationToken`, the node's `animationDuration`) × `factor`. */
export function animationTiming(name: string, theme: ResolvedTheme, durationToken?: string): AnimationTiming | null {
  const def = ANIMATIONS[name]
  if (!def) return null
  const token = parseTokenRef(durationToken ?? def.duration)
  const base = token && token.group === `motion` ? theme.tokens.motion[token.name] : undefined
  const fallback = parseTokenRef(def.duration)
  const ms = (base ?? (fallback ? theme.tokens.motion[fallback.name] : 0) ?? 0) * def.factor
  const ease = parseTokenRef(def.easing)
  const easing = def.easing === LINEAR ? LINEAR : ease && ease.group === `ease` ? (theme.tokens.ease[ease.name] ?? LINEAR) : LINEAR
  return { durationMs: ms, easing, iterations: def.iterations }
}

/** y of a CSS cubic-bezier at x (bisection on x, 60 steps: deterministic). */
export function cubicBezier(curve: Easing, x: number): number {
  const [x1, y1, x2, y2] = curve
  if (x <= 0) return 0
  if (x >= 1) return 1
  const bx = (t: number) => 3 * x1 * t * (1 - t) ** 2 + 3 * x2 * t * t * (1 - t) + t ** 3
  const by = (t: number) => 3 * y1 * t * (1 - t) ** 2 + 3 * y2 * t * t * (1 - t) + t ** 3
  let lo = 0
  let hi = 1
  for (let i = 0; i < 60; i++) {
    const mid = (lo + hi) / 2
    if (bx(mid) < x) lo = mid
    else hi = mid
  }
  return by((lo + hi) / 2)
}

const CHANNELS = [`opacity`, `translateX`, `translateY`, `rotate`, `scale`] as const
const REST: AnimationFrame = { opacity: 1, translateX: 0, translateY: 0, rotate: 0, scale: 1, band: null }
const r4 = (v: number) => {
  const r = Math.round(v * 1e4) / 1e4
  return Object.is(r, -0) ? 0 : r
}

function frameAt(def: AnimationDef, progress: number, easing: Easing | `linear`): AnimationFrame {
  const ks = def.keyframes
  let k = 0
  while (k < ks.length - 2 && progress > ks[k + 1].offset) k++
  const a = ks[k]
  const b = ks[Math.min(k + 1, ks.length - 1)]
  const span = b.offset - a.offset
  const local = span > 0 ? Math.max(0, Math.min(1, (progress - a.offset) / span)) : 1
  const t = easing === LINEAR ? local : cubicBezier(easing, local)
  const out: AnimationFrame = { ...REST }
  for (const ch of CHANNELS) {
    const va = a[ch] ?? REST[ch]
    const vb = b[ch] ?? REST[ch]
    out[ch] = r4(va + (vb - va) * t)
  }
  if (a.band !== undefined && b.band !== undefined) out.band = r4(a.band + (b.band - a.band) * t)
  return out
}

/** The frame `elapsedMs` after the node entered the tree. Finished finite
 *  animations hold their last keyframe; reduced motion = the `rest`
 *  keyframe (no band). */
export function animationFrame(name: string, elapsedMs: number, theme: ResolvedTheme, options: { reducedMotion?: boolean; durationToken?: string } = {}): AnimationFrame | null {
  const def = ANIMATIONS[name]
  const timing = animationTiming(name, theme, options.durationToken)
  if (!def || !timing) return null
  if (options.reducedMotion) {
    const rest = frameAt(def, def.rest, LINEAR)
    return { ...rest, band: null }
  }
  const d = timing.durationMs
  if (d <= 0) return frameAt(def, 1, LINEAR)
  const iterations = timing.iterations === `infinite` ? Infinity : timing.iterations
  const t = Math.max(0, elapsedMs)
  if (t >= d * iterations) return frameAt(def, 1, timing.easing)
  return frameAt(def, (t % d) / d, timing.easing)
}

/** What a painter shows: the node's own opacity × the frame's (an
 *  animation MULTIPLIES opacity; it never replaces it). */
export function paintedOpacity(own: number | undefined, frame: AnimationFrame | null): number {
  return r4((own ?? 1) * (frame?.opacity ?? 1))
}

const css = (n: number) => String(r4(n))

/** The custom properties the keyframes animate, registered so they
 *  interpolate: `--xui-a-opacity` (the animation's opacity factor; the node's
 *  `opacity` = own × it, see css.ts styleToCss) and `--xui-band` (the shimmer
 *  band). A CSS renderer adds this ONCE per document beside the keyframes. */
export const ANIMATION_PROPERTIES_CSS = `@property --xui-a-opacity{syntax:"<number>";inherits:false;initial-value:1}@property --xui-band{syntax:"<number>";inherits:true;initial-value:1}`

/** Whether a keyframe set animates opacity (its node's CSS opacity then
 *  multiplies `--xui-a-opacity`). */
export function animatesOpacity(name: string): boolean {
  return ANIMATIONS[name]?.keyframes.some((k) => k.opacity !== undefined) ?? false
}

/** The CSS `@keyframes xui-<name>` rule (individual `translate`/`rotate`/
 *  `scale` properties compose OUTSIDE the node's own `transform`; opacity
 *  moves `--xui-a-opacity`, which the node's `opacity` multiplies, so the
 *  author's opacity survives the `both` fill; the shimmer band moves
 *  `--xui-band`, which the renderer's overlay reads). Needs
 *  `ANIMATION_PROPERTIES_CSS`. */
export function keyframesCss(name: string): string {
  const def = ANIMATIONS[name]
  if (!def) return ``
  const steps = def.keyframes.map((k) => {
    const decl: string[] = []
    if (k.opacity !== undefined) decl.push(`--xui-a-opacity:${css(k.opacity)}`)
    if (k.translateX !== undefined || k.translateY !== undefined) decl.push(`translate:${css(k.translateX ?? 0)}px ${css(k.translateY ?? 0)}px`)
    if (k.rotate !== undefined) decl.push(`rotate:${css(k.rotate)}deg`)
    if (k.scale !== undefined) decl.push(`scale:${css(k.scale)}`)
    if (k.band !== undefined) decl.push(`--xui-band:${css(k.band)}`)
    return `${css(k.offset * 100)}%{${decl.join(`;`)}}`
  })
  return `@keyframes xui-${name}{${steps.join(``)}}`
}

/** The CSS `animation` shorthand for a resolved timing (per-segment easing
 *  is CSS's own semantics). */
export function animationCss(name: string, timing: AnimationTiming): string {
  const easing = timing.easing === LINEAR ? `linear` : `cubic-bezier(${timing.easing.join(`, `)})`
  const iterations = timing.iterations === `infinite` ? `infinite` : String(timing.iterations)
  return `xui-${name} ${timing.durationMs}ms ${easing} ${iterations} both`
}
