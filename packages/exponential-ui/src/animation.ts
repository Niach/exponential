// Round 2 (docs/round-2-contract.md §2, animation): the token-only
// `animation` style key. catalog/style.json `animations` holds the keyframe
// sets; this module turns one into what a painter needs at a moment —
// duration from the theme's motion tokens, per-segment easing, the channel
// values (opacity, translate, rotate, scale, the shimmer band) — and into
// CSS @keyframes. fixtures/animations.json locks the sampled frames. Pure.

import styleJson from "../generated/style.runtime.json" with { type: "json" }
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
  /** Round 4 motion: an `exitAnimation` set (played when the node leaves). */
  exit?: boolean
  keyframes: Keyframe[]
  band?: { color: string; stops: { offset: number; alpha: number }[] }
}

/** The keyframe sets by name (style.json `animations`, `$comment` aside). */
export const ANIMATIONS: Readonly<Record<string, AnimationDef>> = Object.fromEntries(Object.entries(styleJson.animations).filter(([k]) => !k.startsWith(`$`))) as unknown as Record<string, AnimationDef>
/** The `animation` (enter + loop) sets. */
export const ANIMATION_NAMES: readonly string[] = Object.keys(ANIMATIONS).filter((n) => !ANIMATIONS[n].exit)
/** Round 4 motion: the `exitAnimation` sets. */
export const EXIT_ANIMATION_NAMES: readonly string[] = Object.keys(ANIMATIONS).filter((n) => ANIMATIONS[n].exit)

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
  /** Round 4 motion: ms before the run starts (the node's stagger offset +
   *  its `animationDelay`); absent = 0. The node holds the first keyframe
   *  meanwhile (CSS fill `both`). */
  delayMs?: number
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

/** The frame `elapsedMs` after the node entered the tree (an exit set:
 *  after it began leaving). Finished finite animations hold their last
 *  keyframe; reduced motion = the `rest` keyframe (no band); before
 *  `delayMs` the first keyframe. */
export function animationFrame(name: string, elapsedMs: number, theme: ResolvedTheme, options: { reducedMotion?: boolean; durationToken?: string; delayMs?: number } = {}): AnimationFrame | null {
  const def = ANIMATIONS[name]
  const timing = animationTiming(name, theme, options.durationToken)
  if (!def || !timing) return null
  if (options.reducedMotion) {
    const rest = frameAt(def, def.rest, LINEAR)
    return { ...rest, band: null }
  }
  const d = timing.durationMs
  const delay = options.delayMs ?? 0
  if (delay > 0 && elapsedMs < delay) return frameAt(def, 0, timing.easing)
  if (d <= 0) return frameAt(def, 1, LINEAR)
  const iterations = timing.iterations === `infinite` ? Infinity : timing.iterations
  const t = Math.max(0, elapsedMs - delay)
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
  const delay = timing.delayMs ? ` ${timing.delayMs}ms` : ``
  return `xui-${name} ${timing.durationMs}ms ${easing}${delay} ${iterations} both`
}

const ms = (v: unknown): number => (typeof v === `number` && Number.isFinite(v) && v > 0 ? v : 0)

/** Round 4 motion: the ms a node's ENTER animation waits: its stagger index
 *  (null = it did not enter with a staggered batch) × the parent's
 *  `animationStagger` + its own `animationDelay` (both base style, negative
 *  or non-numbers = 0). Reduced motion = 0. */
export function enterDelay(ownDelay: unknown, staggerIndex: number | null, parentStagger: unknown, reducedMotion = false): number {
  if (reducedMotion) return 0
  return r4(ms(ownDelay) + (staggerIndex === null ? 0 : staggerIndex * ms(parentStagger)))
}

/** Round 4 motion: the timing of a node's `exitAnimation` (`animationDuration`
 *  ms, once resolved, replaces the set's own, its factor kept); null = no
 *  exit (none set, an unknown set, or reduced motion: the node leaves at
 *  once). */
export function exitTiming(name: unknown, theme: ResolvedTheme, durationMs?: number, reducedMotion = false): AnimationTiming | null {
  if (reducedMotion || typeof name !== `string` || !ANIMATIONS[name]?.exit) return null
  const timing = animationTiming(name, theme)
  if (!timing) return null
  return typeof durationMs === `number` && Number.isFinite(durationMs) ? { ...timing, durationMs: durationMs * ANIMATIONS[name].factor } : timing
}

/** One child as a container paints it now (its key + what the renderer
 *  needs to paint it; `exitMs` = its exit's duration, 0 = none). */
export interface PresenceChild<T> {
  key: string
  value: T
  exitMs: number
}

/** One painted child: a present one or a GHOST (`exiting`: its last painted
 *  value, out of hit-testing, keeping its slot until `finishExit`). */
export interface PresenceEntry<T> extends PresenceChild<T> {
  exiting: boolean
}

/** Round 4 motion: the children a container paints after a pass (the
 *  reference the web renderers run and fixtures/animations.json `presence`
 *  locks with the Rust core's surface). A previous child missing from
 *  `next` with an exit (or already exiting) stays as a ghost right after the
 *  nearest previous sibling still present (else first), in the previous
 *  order; a key back in `next` is present again (its ghost dropped; it
 *  enters anew). Reduced motion keeps no ghost. `entering` = the keys of
 *  `next` not present before (ghosts count as absent), in paint order: the
 *  stagger batch (index i → i × animationStagger). */
export function presence<T>(prev: readonly PresenceEntry<T>[], next: readonly PresenceChild<T>[], reducedMotion = false): { entries: PresenceEntry<T>[]; entering: string[] } {
  const nextKeys = new Set(next.map((c) => c.key))
  const present = new Set(prev.filter((p) => !p.exiting).map((p) => p.key))
  const entering = next.filter((c) => !present.has(c.key)).map((c) => c.key)
  // Ghosts by anchor (the last previous key still present before them).
  const head: PresenceEntry<T>[] = []
  const after = new Map<string, PresenceEntry<T>[]>()
  let anchor: string | null = null
  for (const p of prev) {
    if (nextKeys.has(p.key)) {
      anchor = p.key
      continue
    }
    if (reducedMotion || (!p.exiting && !(p.exitMs > 0))) continue
    const ghost = p.exiting ? p : { ...p, exiting: true }
    if (anchor === null) head.push(ghost)
    else {
      const list = after.get(anchor)
      if (list) list.push(ghost)
      else after.set(anchor, [ghost])
    }
  }
  if (head.length === 0 && after.size === 0) return { entries: next.map((c) => ({ ...c, exiting: false })), entering }
  const entries: PresenceEntry<T>[] = [...head]
  for (const c of next) {
    entries.push({ ...c, exiting: false })
    const ghosts = after.get(c.key)
    if (ghosts) entries.push(...ghosts)
  }
  return { entries, entering }
}

/** The entries once `key`'s exit ended (layout closes its gap now). */
export function finishExit<T>(entries: readonly PresenceEntry<T>[], key: string): PresenceEntry<T>[] {
  return entries.filter((e) => !(e.exiting && e.key === key))
}
