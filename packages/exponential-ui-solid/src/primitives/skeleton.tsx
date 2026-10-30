import { splitProps, type ComponentProps } from "solid-js"
import { SKELETON_CLASSES } from "@exponential_app/eui/web/primitives"

import { cn } from "./cn"

function Skeleton(props: ComponentProps<`div`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <div data-slot="skeleton" class={cn(SKELETON_CLASSES.root, local.class)} {...rest} />
}

export { Skeleton }
