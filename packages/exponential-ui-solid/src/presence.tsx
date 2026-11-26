// Round 4 motion (React's presence.tsx; catalog/style.json `animations`
// $comment, fixtures/animations.json `presence`): a container's children as
// it PAINTS them. A child that leaves with an `exitAnimation` stays as a
// GHOST (its subtree, the data frozen as it last painted, `inert`, in its
// own slot) for its exit, then leaves; children entering in one pass wait
// i × `animationStagger` ms (+ their own `animationDelay`). `presence` of
// @exponential_app/eui orders them, as the Rust core's surface does.

import { createContext, createEffect, createMemo, createSignal, For, onCleanup, untrack, useContext, type Accessor, type JSX } from "solid-js"
import { animationCss, enterDelay, exitTiming, finishExit, parseTokenRef, presence, type PresenceEntry, type UiNode } from "@exponential_app/eui"
import { SurfaceContext, deriveContext, useSurfaceContext, type SurfaceContextValue } from "./context"
import type { SolidStyle } from "./style"

/** What a child's ROOT element adds (consumed there, never inherited). */
export interface MotionRoot {
  delayMs: Accessor<number>
  exit: Accessor<string | undefined>
}

export const MotionContext = createContext<MotionRoot | null>(null)
export const useMotionRoot = () => useContext(MotionContext)

/** One child a container paints now. */
export interface PresenceItem {
  key: string
  node: UiNode | undefined
  render: () => JSX.Element
}

interface Painted {
  render: () => JSX.Element
  data: unknown
  exit: string | null
  own: unknown
}

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

/** A stagger, or a child with an exit: the presence pass is needed. */
export function wantsPresence(stagger: unknown, nodes: readonly (UiNode | undefined)[]): boolean {
  return enterDelay(0, 1, stagger) > 0 || nodes.some((n) => (n?.style as Record<string, unknown> | undefined)?.exitAnimation !== undefined)
}

/** The root attributes a MotionRoot adds (React's `motionAttributes`). */
export function motionAttributes(motion: MotionRoot | null, node: UiNode): { style?: SolidStyle; attrs?: Record<string, unknown> } {
  if (!motion) return {}
  const exit = motion.exit()
  if (exit) {
    const own = (node.style as Record<string, unknown> | undefined)?.opacity
    const opacity = typeof own === `number` ? `calc(${own} * var(--xui-a-opacity, 1))` : `var(--xui-a-opacity, 1)`
    return { style: { animation: exit, opacity, "pointer-events": `none` }, attrs: { inert: true, "aria-hidden": `true`, "data-xui-exit": `` } }
  }
  const d = motion.delayMs()
  if (d > 0) return { style: { "animation-delay": `${d}ms` } }
  return {}
}

/** The children as painted (ghosts included), keyed. `firstOnly` = a
 *  windowed List (offsets on its first pass only, no ghost). */
export function PresenceList(props: { items: readonly PresenceItem[]; stagger: unknown }): JSX.Element {
  const ctx = useSurfaceContext()
  const offsets = new Map<string, number>()
  const [finished, setFinished] = createSignal<string[]>([], { equals: false })
  let prev: PresenceEntry<Painted>[] = []
  const entries = createMemo(() => {
    const items = props.items
    const done = finished()
    const data = untrack(() => ctx.data)
    for (const key of done) prev = finishExit(prev, key)
    if (done.length) done.length = 0
    const next = items.map((i) => {
      const exit = exitCss(ctx, i.node)
      return { key: i.key, exitMs: exit?.ms ?? 0, value: { render: i.render, data, exit: exit?.css ?? null, own: (i.node?.style as Record<string, unknown> | undefined)?.animationDelay } }
    })
    const out = presence(prev, next, ctx.reducedMotion)
    out.entering.forEach((key, at) => offsets.set(key, enterDelay(next.find((n) => n.key === key)?.value.own, at, props.stagger, ctx.reducedMotion)))
    prev = out.entries
    return out.entries
  })
  const byKey = createMemo(() => new Map(entries().map((e) => [e.key, e])))
  const keys = createMemo(() => entries().map((e) => e.key), undefined, { equals: (a, b) => a.length === b.length && a.every((x, i) => x === b[i]) })
  // Each new ghost leaves once its exit ran.
  const timers = new Map<string, ReturnType<typeof setTimeout>>()
  createEffect(() => {
    const list = entries()
    for (const e of list) {
      if (!e.exiting || timers.has(e.key)) continue
      timers.set(
        e.key,
        setTimeout(() => {
          timers.delete(e.key)
          setFinished((f) => [...f, e.key])
        }, e.exitMs)
      )
    }
    for (const [key, t] of timers)
      if (!list.some((e) => e.exiting && e.key === key)) {
        clearTimeout(t)
        timers.delete(key)
      }
  })
  onCleanup(() => timers.forEach(clearTimeout))
  return (
    <For each={keys()}>
      {(key) => {
        const entry = createMemo<PresenceEntry<Painted>>((p) => byKey().get(key) ?? p!)
        const ghost = createMemo(() => entry().exiting)
        const sub = deriveContext(ctx, { data: () => (ghost() ? entry().value.data : ctx.data) } as never)
        const motion: MotionRoot = { delayMs: () => (ghost() ? 0 : (offsets.get(key) ?? 0)), exit: () => (ghost() ? (entry().value.exit ?? undefined) : undefined) }
        const render = untrack(() => entry().value.render)
        return (
          <SurfaceContext.Provider value={sub}>
            <MotionContext.Provider value={motion}>{render()}</MotionContext.Provider>
          </SurfaceContext.Provider>
        )
      }}
    </For>
  )
}
