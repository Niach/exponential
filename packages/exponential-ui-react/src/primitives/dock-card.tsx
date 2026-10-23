import * as React from "react"
import { ChevronRight } from "lucide-react"
import { DOCK_CARD_CLASSES, DOCK_PILE_CLASSES, dockPileGeometry, type DockCardTint } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

// N-12: THE DockCard and THE Dock pile (the recipe and the geometry in
// @exponential-at/ui/web/primitives/dock): a one-row floating card (the kind
// glyph, the tinted label, the muted caption, trailing meta, ONE title line,
// the open chevron or a trailing action) and the pile several of them make,
// placed like the toasts (anchored at the bottom: the front card last, the
// older ones peeking out above it; `expanded` fans them). The Solid twin
// paints the same markup.

type Glyph = React.ComponentType<{ className?: string; "data-slot"?: string; "aria-hidden"?: boolean }>

export type DockCardProps = Omit<React.ComponentProps<`div`>, `title` | `children`> & {
  tint?: DockCardTint
  /** The kind glyph (a component; the card sizes and tints it). */
  icon?: Glyph
  label: React.ReactNode
  caption?: React.ReactNode
  meta?: React.ReactNode
  title: React.ReactNode
  /** The card opens: the trailing chevron, a button role, Enter / Space. */
  onOpen?: () => void
  /** Without `onOpen`: a trailing node (a banner's action). */
  trailing?: React.ReactNode
  /** A card behind the front one while piled: its chrome peeks, its words do not. */
  hidden?: boolean
  /** The accessible name of a card that opens (default: no override). */
  openLabel?: string
}

export function DockCard({
  tint = `muted`,
  icon: Icon,
  label,
  caption,
  meta,
  title,
  onOpen,
  trailing,
  hidden = false,
  openLabel,
  className,
  onKeyDown,
  ...props
}: DockCardProps) {
  const pressable = onOpen !== undefined
  return (
    <div
      data-slot="dock-card"
      role={pressable ? `button` : undefined}
      tabIndex={pressable && !hidden ? 0 : undefined}
      aria-label={pressable ? openLabel : undefined}
      aria-hidden={hidden ? true : undefined}
      className={cn(DOCK_CARD_CLASSES.root, pressable && DOCK_CARD_CLASSES.pressable, className)}
      onClick={onOpen}
      onKeyDown={(event) => {
        onKeyDown?.(event)
        if (!pressable || event.defaultPrevented) return
        if (event.key === `Enter` || event.key === ` `) {
          event.preventDefault()
          onOpen()
        }
      }}
      {...props}
    >
      <div data-slot="dock-card-body" data-hidden={hidden ? `true` : `false`} className={DOCK_CARD_CLASSES.body}>
        {Icon && <Icon aria-hidden data-slot="dock-card-icon" className={cn(DOCK_CARD_CLASSES.icon, DOCK_CARD_CLASSES.tint[tint])} />}
        <div className={DOCK_CARD_CLASSES.text}>
          <div className={DOCK_CARD_CLASSES.header}>
            <span data-slot="dock-card-label" className={cn(DOCK_CARD_CLASSES.label, DOCK_CARD_CLASSES.tint[tint])}>
              {label}
            </span>
            {caption !== undefined && caption !== null && (
              <span data-slot="dock-card-caption" className={DOCK_CARD_CLASSES.caption}>
                {caption}
              </span>
            )}
            {meta !== undefined && meta !== null && (
              <span data-slot="dock-card-meta" className={DOCK_CARD_CLASSES.meta}>
                {meta}
              </span>
            )}
          </div>
          <div className={DOCK_CARD_CLASSES.row}>
            <span data-slot="dock-card-title" className={DOCK_CARD_CLASSES.title}>
              {title}
            </span>
            {pressable ? (
              <ChevronRight aria-hidden data-slot="dock-card-chevron" className={DOCK_CARD_CLASSES.chevron} />
            ) : trailing !== undefined && trailing !== null ? (
              <span data-slot="dock-card-trailing" className={DOCK_CARD_CLASSES.trailing}>
                {trailing}
              </span>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  )
}

export interface DockPileItem {
  id: string
  /** The card (a `DockCard`; pass `hidden` for the ones behind the front). */
  node: React.ReactNode
  /** A known height (px) instead of the measured one (specimens, tests). */
  height?: number
}

export type DockPileProps = Omit<React.ComponentProps<`div`>, `children`> & {
  /** Oldest first: the LAST item is the front card. */
  items: readonly DockPileItem[]
  /** Fanned: every card at full size, `gap` apart. */
  expanded: boolean
  /** A pointer entering/leaving the pile or the focus moving in/out of it. */
  onExpandedChange?: (expanded: boolean) => void
}

export function DockPile({ items, expanded, onExpandedChange, className, style, ...props }: DockPileProps) {
  const [measured, setMeasured] = React.useState<Record<string, number>>({})
  const nodes = React.useRef(new Map<string, HTMLDivElement>())
  React.useLayoutEffect(() => {
    const measure = () => {
      const next: Record<string, number> = {}
      for (const item of items) {
        const el = nodes.current.get(item.id)
        if (item.height === undefined && el) next[item.id] = el.offsetHeight
      }
      setMeasured((prev) =>
        Object.keys(next).length === Object.keys(prev).length && Object.entries(next).every(([k, v]) => prev[k] === v) ? prev : next
      )
    }
    measure()
    if (typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(measure)
    for (const el of nodes.current.values()) observer.observe(el)
    return () => observer.disconnect()
  }, [items])
  const geometry = dockPileGeometry(
    items.map((item) => item.height ?? measured[item.id] ?? 0),
    expanded
  )
  return (
    <div
      data-slot="dock-pile"
      data-expanded={expanded ? `true` : `false`}
      className={cn(DOCK_PILE_CLASSES.root, className)}
      style={{ ...style, height: geometry.height }}
      onPointerEnter={(event) => {
        if (event.pointerType !== `touch`) onExpandedChange?.(true)
      }}
      onPointerLeave={(event) => {
        if (event.pointerType !== `touch`) onExpandedChange?.(false)
      }}
      onFocus={() => onExpandedChange?.(true)}
      onBlur={(event) => {
        const next = event.relatedTarget as Node | null
        if (!next || !event.currentTarget.contains(next)) onExpandedChange?.(false)
      }}
      {...props}
    >
      {items.map((item, i) => {
        const placed = geometry.items[i] ?? { offset: 0, scale: 1, visible: true }
        return (
          <div
            key={item.id}
            ref={(el) => {
              if (el) nodes.current.set(item.id, el)
              else nodes.current.delete(item.id)
            }}
            data-slot="dock-pile-item"
            data-front={i === items.length - 1 ? `true` : `false`}
            data-visible={placed.visible ? `true` : `false`}
            className={DOCK_PILE_CLASSES.item}
            style={{ "--offset": `${placed.offset}px`, "--scale": `${placed.scale}`, zIndex: i + 1 } as React.CSSProperties}
          >
            {item.node}
          </div>
        )
      })}
    </div>
  )
}

export { DOCK_CARD_CLASSES, DOCK_PILE_CLASSES, DOCK_PILE_CONSTANTS, dockPileGeometry } from "@exponential-at/ui/web/primitives"
export type { DockCardTint } from "@exponential-at/ui/web/primitives"
