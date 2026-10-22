import { splitProps, type ComponentProps } from "solid-js"
import { KBD_CLASSES } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

function Kbd(props: ComponentProps<`kbd`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <kbd data-slot="kbd" class={cn(KBD_CLASSES.root, local.class)} {...rest} />
}

function KbdGroup(props: ComponentProps<`span`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <span data-slot="kbd-group" class={cn(KBD_CLASSES.group, local.class)} {...rest} />
}

export { Kbd, KbdGroup }
