import { splitProps, type ComponentProps, type JSX } from "solid-js"
import { ChevronDown, ChevronRight } from "lucide-solid"
import { DISCLOSURE_HEADER_CLASSES } from "@exponential_app/ui/web/primitives"

import { PaintIcon } from "../icons"
import { cn } from "./cn"

export function DisclosureHeader(
  props: Omit<ComponentProps<`button`>, `onClick` | `onToggle` | `type` | `aria-expanded` | `children`> & {
    open: boolean
    onToggle: () => void
    chevron?: `leading` | `trailing`
    children: JSX.Element
  }
) {
  const [local, rest] = splitProps(props, [`open`, `onToggle`, `chevron`, `class`, `children`])
  const chevron = () => local.chevron ?? `leading`
  const glyph = () => (
    <PaintIcon
      icon={local.open ? ChevronDown : ChevronRight}
      data-slot="disclosure-chevron"
      class={cn(DISCLOSURE_HEADER_CLASSES.chevron, chevron() === `trailing` && DISCLOSURE_HEADER_CLASSES.chevronTrailing)}
    />
  )
  return (
    <button
      type="button"
      data-slot="disclosure-header"
      data-chevron={chevron()}
      aria-expanded={local.open}
      onClick={() => local.onToggle()}
      class={cn(DISCLOSURE_HEADER_CLASSES.root, local.class)}
      {...rest}
    >
      {chevron() === `leading` && glyph()}
      {local.children}
      {chevron() === `trailing` && glyph()}
    </button>
  )
}
