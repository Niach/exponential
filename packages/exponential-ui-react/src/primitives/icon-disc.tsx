import type { LucideIcon } from "lucide-react"
import { ICON_DISC_CLASSES, type IconDiscTone } from "@exponential_app/ui/web/primitives"

import { cn } from "./cn"

export { ICON_DISC_TONES } from "@exponential_app/ui/web/primitives"
export type { IconDiscTone } from "@exponential_app/ui/web/primitives"

export function IconDisc({
  icon: Icon,
  tone = `primary`,
  strokeWidth,
  className,
}: {
  icon: LucideIcon
  tone?: IconDiscTone
  strokeWidth?: number
  className?: string
}) {
  return (
    <div
      data-slot="icon-disc"
      className={cn(ICON_DISC_CLASSES.root, ICON_DISC_CLASSES.tones[tone], className)}
    >
      {/* Decorative: the disc always sits beside a title that names it. */}
      <Icon
        className={ICON_DISC_CLASSES.icon}
        strokeWidth={strokeWidth}
        aria-hidden="true"
        focusable="false"
      />
    </div>
  )
}
