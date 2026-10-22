import { Show, splitProps, type ComponentProps } from "solid-js"
import { BADGE_CLASSES, badgeLabel, type BadgeTone } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

export function Badge(
  props: Omit<ComponentProps<`span`>, `children`> & {
    count: number
    /** Past this the badge reads `${max}+`. */
    max?: number
    tone?: BadgeTone
  }
) {
  const [local, rest] = splitProps(props, [`count`, `max`, `tone`, `class`])
  const tone = () => local.tone ?? `muted`
  return (
    <Show when={local.count > 0}>
      <span data-slot="badge" data-tone={tone()} class={cn(BADGE_CLASSES.root, BADGE_CLASSES.tones[tone()], local.class)} {...rest}>
        {badgeLabel(local.count, local.max ?? 99)}
      </span>
    </Show>
  )
}
