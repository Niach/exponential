import { splitProps, type ComponentProps } from "solid-js"
import { INPUT_CLASSES } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

export { BARE_FIELD_CLASS, FIELD_CHROME_CLASS } from "@exponential-at/ui/web/primitives"

/** React reflects a controlled `value` as the attribute too (shared.tsx
 *  rule 2): `attr:value` mirrors it. */
function Input(props: ComponentProps<`input`>) {
  const [local, rest] = splitProps(props, [`class`])
  const valueAttr = () => (props.value === undefined || props.value === null ? undefined : String(props.value))
  return <input data-slot="input" class={cn(INPUT_CLASSES.root, local.class)} attr:value={valueAttr()} {...rest} />
}

export { Input }
