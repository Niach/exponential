import { splitProps, type JSX } from "solid-js"
import { LoaderCircle } from "lucide-solid"
import { SPINNER_CLASSES } from "@exponential_app/ui/web/primitives"

import { PaintIcon } from "../icons"
import { css } from "../style"
import { cn } from "./cn"

function Spinner(
  props: Omit<JSX.SvgSVGAttributes<SVGSVGElement>, `children`> & {
    size?: number
    /** The accessible name; the glyph itself never carries words. */
    label?: string
    class?: string
    [data: `data-${string}`]: string | undefined
  }
) {
  const [local, rest] = splitProps(props, [`size`, `label`, `class`])
  const size = () => local.size ?? 10
  return (
    <PaintIcon
      icon={LoaderCircle}
      data-slot="spinner"
      data-testid="feedback-spinner"
      aria-label={local.label ?? `Loading`}
      width={size()}
      height={size()}
      style={css({ width: size(), height: size() })}
      class={cn(SPINNER_CLASSES.root, local.class)}
      {...rest}
    />
  )
}

export { Spinner }
