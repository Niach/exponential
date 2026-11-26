// Round 4 motion (catalog/style.json `animations` $comment, fixtures/
// animations.json `presence`): a container's children as it PAINTS them.
// A child that leaves the tree (`visible` turning false, its template item
// removed, an update dropping it) with an `exitAnimation` stays as a GHOST:
// its last painted subtree (the data frozen as it was), `inert` (no events,
// no focus, out of the accessibility tree), in its own slot, playing its
// exit; when the exit ran it leaves and layout closes the gap. Children
// entering in one render are numbered 0, 1, 2… and child i waits i ×
// `animationStagger` ms (+ its own `animationDelay`) before its enter
// animation. The order and the ghosts are `presence` of @exponential_app/eui,
// the reference the Rust core's surface replays.

import { createContext, useContext, useEffect, useReducer, useRef, type CSSProperties, type ReactNode } from "react"
import { animationCss, enterDelay, exitTiming, finishExit, parseTokenRef, presence, type PresenceEntry, type UiNode } from "@exponential_app/eui"
import { SurfaceContext, deriveContext, useSurfaceContext, type SurfaceContextValue } from "./context"

/** What a child's ROOT element adds (consumed there, never inherited). */
export interface MotionRoot {
  /** The enter's offset (ms): the stagger + the node's own delay. */
  delayMs?: number
  /** A ghost: the exit's CSS `animation` shorthand. */
  exit?: string
}

export const MotionContext = createContext<MotionRoot | null>(null)

/** One child a container paints now: its key, its exit (ms, 0 = none) and
 *  how it renders. */
export interface PresenceItem {
  key: string
  node: UiNode | undefined
  render: () => ReactNode
}

interface Painted {
  render: () => ReactNode
  data: unknown
  exit: string | null
  own: unknown
}

/** The ms of a `$motion` token (or a number) under the theme. */
function motionMs(ctx: SurfaceContextValue, value: unknown): number | undefined {
  if (typeof value === `number`) return value
  const token = typeof value === `string` ? parseTokenRef(value) : null
  return token?.group === `motion` ? ctx.theme.tokens.motion[token.name] : undefined
}

/** The node's exit as CSS (null = none, or reduced motion). */
export function exitCss(ctx: SurfaceContextValue, node: UiNode | undefined): { css: string; ms: number } | null {
  const style = node?.style as Record<string, unknown> | undefined
  const timing = exitTiming(style?.exitAnimation, ctx.theme, motionMs(ctx, style?.animationDuration), ctx.reducedMotion)
  if (!timing) return null
  return { css: animationCss(style!.exitAnimation as string, timing), ms: timing.durationMs }
}

/** Does a container need the presence pass (a stagger, or a child with an
 *  exit)? Otherwise it paints its children as they are. */
export function wantsPresence(stagger: unknown, items: readonly PresenceItem[]): boolean {
  return enterDelay(0, 1, stagger) > 0 || items.some((i) => (i.node?.style as Record<string, unknown> | undefined)?.exitAnimation !== undefined)
}

/** The children as painted (ghosts included, each wrapped in its root's
 *  MotionContext). `items` null = the plain pass (nothing tracked).
 *  `firstOnly` = a windowed List: offsets on its first render only, never a
 *  ghost. */
export function usePresence(items: readonly PresenceItem[], stagger: unknown, options: { firstOnly?: boolean } = {}): { key: string; ghost: boolean; node: ReactNode }[] {
  const ctx = useSurfaceContext()
  const prev = useRef<PresenceEntry<Painted>[]>([])
  const offsets = useRef(new Map<string, number>())
  const first = useRef(true)
  const [, rerender] = useReducer((n: number) => n + 1, 0)
  const track = wantsPresence(stagger, items) || prev.current.some((e) => e.exiting || e.exitMs > 0)
  const next = track
    ? items.map((i) => {
        const exit = options.firstOnly ? null : exitCss(ctx, i.node)
        return { key: i.key, exitMs: exit?.ms ?? 0, value: { render: i.render, data: ctx.data, exit: exit?.css ?? null, own: (i.node?.style as Record<string, unknown> | undefined)?.animationDelay } }
      })
    : []
  const out = track ? presence(prev.current, next, ctx.reducedMotion) : { entries: [] as PresenceEntry<Painted>[], entering: [] as string[] }
  if (track) {
    out.entering.forEach((key, at) => {
      const own = next.find((n) => n.key === key)?.value.own
      const staggered = options.firstOnly && !first.current ? null : at
      offsets.current.set(key, enterDelay(own, staggered, stagger, ctx.reducedMotion))
    })
  }
  const entries = out.entries
  useEffect(() => {
    prev.current = entries
    first.current = false
    for (const key of [...offsets.current.keys()]) if (!entries.some((e) => e.key === key)) offsets.current.delete(key)
  })
  // Each new ghost leaves once its exit ran.
  const timers = useRef(new Map<string, ReturnType<typeof setTimeout>>())
  useEffect(() => {
    for (const e of entries) {
      if (!e.exiting || timers.current.has(e.key)) continue
      timers.current.set(
        e.key,
        setTimeout(() => {
          timers.current.delete(e.key)
          prev.current = finishExit(prev.current, e.key)
          rerender()
        }, e.exitMs)
      )
    }
    for (const [key, t] of timers.current)
      if (!entries.some((e) => e.exiting && e.key === key)) {
        clearTimeout(t)
        timers.current.delete(key)
      }
  })
  useEffect(() => () => timers.current.forEach(clearTimeout), [])
  if (!track) return items.map((i) => ({ key: i.key, ghost: false, node: i.render() }))
  return entries.map((e) => {
    if (e.exiting) {
      const frozen = deriveContext(ctx, { data: e.value.data } as Partial<SurfaceContextValue>)
      return {
        key: e.key,
        ghost: true,
        node: (
          <SurfaceContext.Provider key={e.key} value={frozen}>
            <MotionContext.Provider value={{ exit: e.value.exit ?? undefined }}>{e.value.render()}</MotionContext.Provider>
          </SurfaceContext.Provider>
        ),
      }
    }
    const delayMs = offsets.current.get(e.key) ?? 0
    return { key: e.key, ghost: false, node: delayMs > 0 ? <MotionContext.Provider value={{ delayMs }}>{e.value.render()}</MotionContext.Provider> : e.value.render() }
  })
}

/** The root attributes a MotionRoot adds: the enter offset as
 *  `animation-delay`; a ghost's exit `animation` (opacity multiplied like
 *  an enter's), `inert` and `data-xui-exit`. */
export function motionAttributes(motion: MotionRoot | null, node: UiNode): { style?: CSSProperties; attrs?: Record<string, unknown> } {
  if (!motion) return {}
  if (motion.exit) {
    const own = (node.style as Record<string, unknown> | undefined)?.opacity
    const opacity = typeof own === `number` ? `calc(${own} * var(--xui-a-opacity, 1))` : `var(--xui-a-opacity, 1)`
    return { style: { animation: motion.exit, opacity, pointerEvents: `none` } as CSSProperties, attrs: { inert: true, "aria-hidden": `true`, "data-xui-exit": `` } }
  }
  if (motion.delayMs) return { style: { animationDelay: `${motion.delayMs}ms` } }
  return {}
}

export const useMotionRoot = () => useContext(MotionContext)
