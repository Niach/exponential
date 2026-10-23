import { splitProps, type ComponentProps, type JSX } from "solid-js"
import { LABEL_CLASSES } from "@exponential_app/ui/web/primitives"

import { cn } from "./cn"

// Radix's Label as plain DOM: a `<label>` that keeps a double-click from
// selecting its text, unless the press lands on a control inside it.
function Label(props: ComponentProps<`label`>) {
  const [local, rest] = splitProps(props, [`class`, `onMouseDown`])
  const onMouseDown: JSX.EventHandler<HTMLLabelElement, MouseEvent> = (event) => {
    if ((event.target as Element).closest(`button, input, select, textarea`)) return
    const handler = local.onMouseDown
    if (typeof handler === `function`) handler(event)
    else if (Array.isArray(handler)) handler[0](handler[1], event)
    if (!event.defaultPrevented && event.detail > 1) event.preventDefault()
  }
  return <label data-slot="label" class={cn(LABEL_CLASSES.root, local.class)} {...rest} onMouseDown={onMouseDown} />
}

export { Label }
