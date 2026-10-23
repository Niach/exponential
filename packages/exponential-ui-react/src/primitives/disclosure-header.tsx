import * as React from "react"
import { ChevronDown, ChevronRight } from "lucide-react"
import { DISCLOSURE_HEADER_CLASSES } from "@exponential_app/ui/web/primitives"

import { cn } from "./cn"

const ChevronRightGlyph = ChevronRight
const ChevronDownGlyph = ChevronDown

export function DisclosureHeader({
  open,
  onToggle,
  chevron = `leading`,
  className,
  children,
  ...props
}: Omit<
  React.ComponentProps<`button`>,
  `onClick` | `type` | `aria-expanded` | `children`
> & {
  open: boolean
  onToggle: () => void
  chevron?: `leading` | `trailing`
  children: React.ReactNode
}) {
  const Glyph = open ? ChevronDownGlyph : ChevronRightGlyph
  const glyph = (
    <Glyph
      data-slot="disclosure-chevron"
      className={cn(
        DISCLOSURE_HEADER_CLASSES.chevron,
        chevron === `trailing` && DISCLOSURE_HEADER_CLASSES.chevronTrailing
      )}
    />
  )
  return (
    <button
      type="button"
      data-slot="disclosure-header"
      data-chevron={chevron}
      aria-expanded={open}
      onClick={onToggle}
      className={cn(DISCLOSURE_HEADER_CLASSES.root, className)}
      {...props}
    >
      {chevron === `leading` && glyph}
      {children}
      {chevron === `trailing` && glyph}
    </button>
  )
}
