import { For, Show, splitProps, type ComponentProps } from "solid-js"
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

/** EXP-1495: React's `Outline` (an anchor list, ONE active row, a `· n` count). */
export function Outline(props: OutlineProps) {
  const [local, rest] = splitProps(props, [`items`, `value`, `caption`, `onSelect`, `class`])
  return (
    <nav data-slot="outline" aria-label={local.caption || `Outline`} class={cn(OUTLINE_CLASSES.root, local.class)} {...rest}>
      <Show when={local.caption}>
        <div data-slot="outline-caption" aria-hidden="true" class={OUTLINE_CLASSES.caption}>
          {local.caption}
        </div>
      </Show>
      <For each={local.items}>
        {(item) => {
          const active = () => local.value != null && item.id === local.value
          const count = () => outlineCountText(item.count)
          return (
            <button
              type="button"
              data-slot="outline-row"
              data-active={active() ? `true` : undefined}
              aria-current={active() ? `location` : undefined}
              aria-label={count() ? `${item.title}, ${item.count}` : undefined}
              class={OUTLINE_CLASSES.row}
              onClick={() => local.onSelect?.(item.id)}
            >
              <Show when={active()}>
                <span data-slot="outline-bar" aria-hidden="true" class={OUTLINE_CLASSES.bar} />
              </Show>
              <span class={OUTLINE_CLASSES.title}>{item.title}</span>
              <Show when={count()}>
                <span aria-hidden="true" class={OUTLINE_CLASSES.count}>
                  {count()}
                </span>
              </Show>
            </button>
          )
        }}
      </For>
    </nav>
  )
}
