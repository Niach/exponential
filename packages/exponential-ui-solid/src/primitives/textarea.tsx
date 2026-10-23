import { createEffect, splitProps, type ComponentProps } from "solid-js"
import { TEXTAREA_CLASSES } from "@exponential_app/ui/web/primitives"

import { cn } from "./cn"

/** React writes a controlled `value` as the textarea's TEXT (its default
 *  value) as well as the property: both, here too. */
function Textarea(props: ComponentProps<`textarea`>) {
  const [local, rest] = splitProps(props, [`class`, `value`, `ref`])
  let el: HTMLTextAreaElement | undefined
  const text = () => (local.value === undefined || local.value === null ? undefined : String(local.value))
  createEffect(() => {
    const v = text()
    if (el && v !== undefined && el.value !== v) el.value = v
  })
  return (
    <textarea
      ref={(node) => {
        el = node
        const ref = local.ref as ((el: HTMLTextAreaElement) => void) | undefined
        if (typeof ref === `function`) ref(node)
      }}
      data-slot="textarea"
      class={cn(TEXTAREA_CLASSES.root, local.class)}
      {...rest}
    >
      {text()}
    </textarea>
  )
}

export { Textarea }
