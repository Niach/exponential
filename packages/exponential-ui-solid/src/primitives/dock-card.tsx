import { For, Show, createEffect, createMemo, createSignal, onCleanup, splitProps, type Component, type ComponentProps, type JSX } from "solid-js"
import { ChevronRight } from "lucide-solid"
import { DOCK_CARD_CLASSES, DOCK_PILE_CLASSES, dockPileGeometry, type DockCardTint } from "@exponential-at/ui/web/primitives"

import { bindStyle } from "../behaviors/style-writer"
import { PaintIcon } from "../icons"
import { css } from "../style"
import { cn } from "./cn"

// N-12: React's dock-card.tsx on Solid: the same DockCard markup and the
// same pile, placed by the shared geometry (dockPileGeometry).

type Glyph = Component<{ class?: string }>

export type DockCardProps = Omit<ComponentProps<`div`>, `title` | `children`> & {
  tint?: DockCardTint
  /** The kind glyph (a component; the card sizes and tints it). */
  icon?: Glyph
  label: JSX.Element
  caption?: JSX.Element
  meta?: JSX.Element
  title: JSX.Element
  /** The card opens: the trailing chevron, a button role, Enter / Space. */
  onOpen?: () => void
  /** Without `onOpen`: a trailing node (a banner's action). */
  trailing?: JSX.Element
  /** A card behind the front one while piled: its chrome peeks, its words do not. */
  hidden?: boolean
  /** The accessible name of a card that opens. */
  openLabel?: string
}

export function DockCard(props: DockCardProps) {
  const [local, rest] = splitProps(props, [`tint`, `icon`, `label`, `caption`, `meta`, `title`, `onOpen`, `trailing`, `hidden`, `openLabel`, `class`])
  const tint = () => local.tint ?? `muted`
  const pressable = () => local.onOpen !== undefined
  const has = (value: JSX.Element | undefined) => value !== undefined && value !== null
  return (
    <div
      data-slot="dock-card"
      role={pressable() ? `button` : undefined}
      tabIndex={pressable() && !local.hidden ? 0 : undefined}
      aria-label={pressable() ? local.openLabel : undefined}
      aria-hidden={local.hidden ? true : undefined}
      class={cn(DOCK_CARD_CLASSES.root, pressable() && DOCK_CARD_CLASSES.pressable, local.class)}
      onClick={() => local.onOpen?.()}
      onKeyDown={(event) => {
        if (!pressable() || event.defaultPrevented) return
        if (event.key === `Enter` || event.key === ` `) {
          event.preventDefault()
          local.onOpen?.()
        }
      }}
      {...rest}
    >
      <div data-slot="dock-card-body" data-hidden={local.hidden ? `true` : `false`} class={DOCK_CARD_CLASSES.body}>
        <Show when={local.icon}>
          {(icon) => <PaintIcon icon={icon()} aria-hidden data-slot="dock-card-icon" class={cn(DOCK_CARD_CLASSES.icon, DOCK_CARD_CLASSES.tint[tint()])} />}
        </Show>
        <div class={DOCK_CARD_CLASSES.text}>
          <div class={DOCK_CARD_CLASSES.header}>
            <span data-slot="dock-card-label" class={cn(DOCK_CARD_CLASSES.label, DOCK_CARD_CLASSES.tint[tint()])}>
              {local.label}
            </span>
            <Show when={has(local.caption)}>
              <span data-slot="dock-card-caption" class={DOCK_CARD_CLASSES.caption}>
                {local.caption}
              </span>
            </Show>
            <Show when={has(local.meta)}>
              <span data-slot="dock-card-meta" class={DOCK_CARD_CLASSES.meta}>
                {local.meta}
              </span>
            </Show>
          </div>
          <div class={DOCK_CARD_CLASSES.row}>
            <span data-slot="dock-card-title" class={DOCK_CARD_CLASSES.title}>
              {local.title}
            </span>
            <Show
              when={pressable()}
              fallback={
                <Show when={has(local.trailing)}>
                  <span data-slot="dock-card-trailing" class={DOCK_CARD_CLASSES.trailing}>
                    {local.trailing}
                  </span>
                </Show>
              }
            >
              <PaintIcon icon={ChevronRight} aria-hidden data-slot="dock-card-chevron" class={DOCK_CARD_CLASSES.chevron} />
            </Show>
          </div>
        </div>
      </div>
    </div>
  )
}

export interface DockPileItem {
  id: string
  /** The card (a `DockCard`; pass `hidden` for the ones behind the front). */
  node: JSX.Element
  /** A known height (px) instead of the measured one (specimens, tests). */
  height?: number
}

export type DockPileProps = Omit<ComponentProps<`div`>, `children`> & {
  /** Oldest first: the LAST item is the front card. */
  items: readonly DockPileItem[]
  /** Fanned: every card at full size, `gap` apart. */
  expanded: boolean
  /** A pointer entering/leaving the pile or the focus moving in/out of it. */
  onExpandedChange?: (expanded: boolean) => void
}

export function DockPile(props: DockPileProps) {
  const [local, rest] = splitProps(props, [`items`, `expanded`, `onExpandedChange`, `class`, `style`])
  const [measured, setMeasured] = createSignal<Record<string, number>>({})
  const nodes = new Map<string, HTMLDivElement>()
  const measure = () => {
    const next: Record<string, number> = {}
    for (const item of local.items) {
      const el = nodes.get(item.id)
      if (item.height === undefined && el) next[item.id] = el.offsetHeight
    }
    const prev = measured()
    if (!(Object.keys(next).length === Object.keys(prev).length && Object.entries(next).every(([k, v]) => prev[k] === v))) setMeasured(next)
  }
  createEffect(() => {
    void local.items
    measure()
    if (typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(measure)
    for (const el of nodes.values()) observer.observe(el)
    onCleanup(() => observer.disconnect())
  })
  const geometry = createMemo(() =>
    dockPileGeometry(
      local.items.map((item) => item.height ?? measured()[item.id] ?? 0),
      local.expanded
    )
  )
  return (
    <div
      ref={(el) => bindStyle(el, () => ({ ...((local.style as Record<string, string> | undefined) ?? {}), height: `${geometry().height}px` }))}
      data-slot="dock-pile"
      data-expanded={local.expanded ? `true` : `false`}
      class={cn(DOCK_PILE_CLASSES.root, local.class)}
      onPointerEnter={(event) => {
        if (event.pointerType !== `touch`) local.onExpandedChange?.(true)
      }}
      onPointerLeave={(event) => {
        if (event.pointerType !== `touch`) local.onExpandedChange?.(false)
      }}
      onFocusIn={() => local.onExpandedChange?.(true)}
      onFocusOut={(event) => {
        const next = event.relatedTarget as Node | null
        if (!next || !event.currentTarget.contains(next)) local.onExpandedChange?.(false)
      }}
      {...rest}
    >
      <For each={local.items}>
        {(item, i) => {
          const placed = () => geometry().items[i()] ?? { offset: 0, scale: 1, visible: true }
          let node: HTMLDivElement | undefined
          onCleanup(() => {
            if (nodes.get(item.id) === node) nodes.delete(item.id)
          })
          return (
            <div
              ref={(el) => {
                node = el
                nodes.set(item.id, el)
                bindStyle(el, () => css({ "--offset": `${placed().offset}px`, "--scale": `${placed().scale}`, zIndex: i() + 1 })!)
              }}
              data-slot="dock-pile-item"
              data-front={i() === local.items.length - 1 ? `true` : `false`}
              data-visible={placed().visible ? `true` : `false`}
              class={DOCK_PILE_CLASSES.item}
            >
              {item.node}
            </div>
          )
        }}
      </For>
    </div>
  )
}

export { DOCK_CARD_CLASSES, DOCK_PILE_CLASSES, DOCK_PILE_CONSTANTS, dockPileGeometry } from "@exponential-at/ui/web/primitives"
export type { DockCardTint } from "@exponential-at/ui/web/primitives"
