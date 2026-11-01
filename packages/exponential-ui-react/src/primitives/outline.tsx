import type { ComponentProps } from "react"
import { OUTLINE_CLASSES, outlineCountText, type OutlineItem } from "@exponential_app/eui/web/primitives"

import { cn } from "./cn"

export { OUTLINE_GEOMETRY, outlineCountText } from "@exponential_app/eui/web/primitives"
export type { OutlineItem } from "@exponential_app/eui/web/primitives"

export type OutlineProps = Omit<ComponentProps<`nav`>, `onSelect` | `children`> & {
  items: readonly OutlineItem[]
  /** The active row's id (the host's scroll-spy binds it); none = no active row. */
  value?: string | null
  /** The uppercase caption over the rows (and the nav's name). */
  caption?: string
  onSelect?: (id: string) => void
}

/** EXP-1495: the catalog's `Outline` macro as a primitive: an anchor list,
 *  ONE active row (accent + a leading foreground bar, `aria-current`), a
 *  muted `· n` count. A press selects the row; scroll-spy stays in the host. */
export function Outline({ items, value, caption, onSelect, className, ...props }: OutlineProps) {
  return (
    <nav data-slot="outline" aria-label={caption || `Outline`} className={cn(OUTLINE_CLASSES.root, className)} {...props}>
      {caption ? (
        <div data-slot="outline-caption" aria-hidden="true" className={OUTLINE_CLASSES.caption}>
          {caption}
        </div>
      ) : null}
      {items.map((item) => {
        const active = value != null && item.id === value
        const count = outlineCountText(item.count)
        return (
          <button
            key={item.id}
            type="button"
            data-slot="outline-row"
            data-active={active ? `true` : undefined}
            aria-current={active ? `location` : undefined}
            aria-label={count ? `${item.title}, ${item.count}` : undefined}
            className={OUTLINE_CLASSES.row}
            onClick={() => onSelect?.(item.id)}
          >
            {active ? <span data-slot="outline-bar" aria-hidden="true" className={OUTLINE_CLASSES.bar} /> : null}
            <span className={OUTLINE_CLASSES.title}>{item.title}</span>
            {count ? (
              <span aria-hidden="true" className={OUTLINE_CLASSES.count}>
                {count}
              </span>
            ) : null}
          </button>
        )
      })}
    </nav>
  )
}
