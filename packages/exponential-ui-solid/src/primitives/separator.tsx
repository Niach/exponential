import { splitProps, type ComponentProps } from "solid-js"
import { SEPARATOR_CLASSES } from "@exponential_app/eui/web/primitives"

import { cn } from "./cn"

// Radix's Separator as plain DOM: `data-orientation` always, `role="none"`
// when decorative, else `role="separator"` (+ `aria-orientation` only when
// vertical).
function Separator(props: ComponentProps<`div`> & { orientation?: `horizontal` | `vertical`; decorative?: boolean }) {
  const [local, rest] = splitProps(props, [`class`, `orientation`, `decorative`])
  const orientation = () => (local.orientation === `vertical` ? `vertical` : `horizontal`)
  const decorative = () => local.decorative ?? true
  return (
    <div
      data-orientation={orientation()}
      role={decorative() ? `none` : `separator`}
      aria-orientation={!decorative() && orientation() === `vertical` ? `vertical` : undefined}
      data-slot="separator"
      class={cn(SEPARATOR_CLASSES.root, local.class)}
      {...rest}
    />
  )
}

export { Separator }
