import { splitProps, type ComponentProps, type ValidComponent } from "solid-js"
import { Dynamic } from "solid-js/web"
import { buttonVariants, type ButtonVariantProps } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

export type ButtonProps = ComponentProps<`button`> &
  ButtonVariantProps & {
    /** React's `asChild`: the element (a tag or a component) the button
     *  paints as, wearing exactly what Radix's Slot merges onto the child. */
    as?: ValidComponent
    href?: string
    target?: string
    rel?: string
  }

function Button(props: ButtonProps) {
  const [local, rest] = splitProps(props, [`class`, `variant`, `size`, `as`])
  const variant = () => local.variant ?? `default`
  const size = () => local.size ?? `default`
  return (
    <Dynamic
      component={local.as ?? `button`}
      data-slot="button"
      data-variant={variant()}
      data-size={size()}
      class={cn(buttonVariants({ variant: variant(), size: size(), className: local.class }))}
      {...rest}
    />
  )
}

export { Button, buttonVariants }
