import * as React from "react"
import { Slot } from "radix-ui"

import { cn } from "./cn"

// EXP-904 — the ONE floating-action chrome recipe.
//
// Four phone surfaces painted the same floating glass by copy-paste: the
// issue bar's coding circle, the Work bar's circles and capsule, the tab
// bar's FAB and its two-armed group, and the steer composer's tray. This is
// their common core — the card hairline, the 85% popover fill and the
// backdrop blur. Everything a floating piece owns on its own (its radius,
// its layout) stays at the call site, because a capsule and a rounded tray
// genuinely differ from the circle.
//
// EXP-916: no drop shadow. Android's floating bar (the reference the phones
// are locked to) sits flat on the content with only its hairline for an edge;
// the web's r16 shadow read as a different, heavier control.
export const FAB_CHROME_CLASS = `border border-glass-stroke-card bg-popover/85 backdrop-blur-xl`

// EXP-962 — the 52px CIRCLE itself, as a class and as a button. Three files
// restated the circle (one of them in rem) before it became `FabButton`: the
// tab bar's FAB, the issue bar's coding circle and every slot of the Work
// bar. Its glyph is white at the SECONDARY emphasis (Android's
// `TextEmphasis.Secondary`, iOS `FloatingBarCircle`); a slot that wants a
// full-white glyph — the one primary action in the bar — says
// `emphasis="primary"`. `MOBILE_WORK_CIRCLE_CLASS` in mobile-work-bar.tsx is
// this string, for the two slots that are not buttons (the usage ring, the
// capsule it stretches into).
export const FAB_CIRCLE_CLASS = `pointer-events-auto flex size-[52px] shrink-0 items-center justify-center rounded-full ${FAB_CHROME_CLASS} text-foreground/70`

export function FabButton({
  emphasis = `secondary`,
  asChild = false,
  className,
  type,
  ...props
}: React.ComponentProps<`button`> & {
  /** `primary` = the full-white glyph of the bar's one call to action. */
  emphasis?: `secondary` | `primary`
  /** Render as the single child (a `Link`/`<a>`), like `Button`. */
  asChild?: boolean
}) {
  const Comp = asChild ? Slot.Root : `button`
  return (
    <Comp
      type={asChild ? type : (type ?? `button`)}
      data-slot="fab-button"
      data-emphasis={emphasis}
      className={cn(
        FAB_CIRCLE_CLASS,
        `outline-none transition-colors duration-fast active:bg-glass-active focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50`,
        emphasis === `primary` && `text-foreground`,
        className
      )}
      {...props}
    />
  )
}
