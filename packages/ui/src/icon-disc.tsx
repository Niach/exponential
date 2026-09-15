import type { LucideIcon } from "lucide-react"

import { cn } from "./cn"

// EXP-903 — the ONE icon disc: a 48px circle holding a 24px glyph.
//
// Eight web surfaces drew it by hand — the empty state, the onboarding step
// card and the invite page in the primary tint, the GitHub claim/installed
// pages in red, emerald and muted. The tone owns BOTH the wash and the glyph
// colour, so a call site never restates `text-*` on the icon. Placement
// (`mx-auto`, margins) stays at the call site.
export const ICON_DISC_TONES = [`primary`, `success`, `danger`, `muted`] as const

export type IconDiscTone = (typeof ICON_DISC_TONES)[number]

const TONE: Record<IconDiscTone, string> = {
  primary: `bg-primary/10 text-primary`,
  success: `bg-emerald-500/15 text-emerald-500`,
  danger: `bg-red-500/15 text-red-500`,
  muted: `bg-muted text-muted-foreground`,
}

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
      className={cn(
        `flex size-12 shrink-0 items-center justify-center rounded-full`,
        TONE[tone],
        className
      )}
    >
      <Icon className="size-6" strokeWidth={strokeWidth} />
    </div>
  )
}
