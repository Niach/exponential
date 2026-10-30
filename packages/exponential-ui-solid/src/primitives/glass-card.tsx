import { splitProps, type ComponentProps } from "solid-js"
import { GLASS_CARD_CLASSES } from "@exponential_app/eui/web/primitives"

import { cn } from "./cn"

export { GLASS_CARD_CLASS, GLASS_CARD_CLASSES } from "@exponential_app/eui/web/primitives"

export function GlassCard(props: ComponentProps<`div`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <div data-slot="glass-card" class={cn(GLASS_CARD_CLASSES.root, local.class)} {...rest} />
}
