import type { ComponentProps, ReactNode } from "react"
import { useKeyboardInset } from "@/hooks/use-keyboard-inset"
import { FAB_CHROME_CLASS } from "@exp/ui"
import { cn } from "@/lib/utils"

// EXP-893: the phone's ONE floating bottom bar — `[left circle] [centre
// capsule] [right circle]`, the glass recipe of EXP-568's issue bar and the
// tab bar, now shared by every face of the Work screen: the issue (Properties
// · Comment · Start/switcher), the run (usage ring · composer · switcher) and
// the changes (GitHub · Merge · switcher). `expanded` replaces the left circle
// and the capsule with a full-width composer while the trailing circle stays
// MOUNTED (a `contents`/`hidden` swap), because the switcher owns lookups
// that must not re-run on every expand.

/** The 52px glass circle every slot of the bar is made of. */
export const MOBILE_WORK_CIRCLE_CLASS = `pointer-events-auto flex size-[52px] shrink-0 items-center justify-center rounded-full ${FAB_CHROME_CLASS} text-muted-foreground`

/** The capsule between the circles: the same chrome stretched. */
export const MOBILE_WORK_CAPSULE_CLASS = cn(
  MOBILE_WORK_CIRCLE_CLASS,
  `h-[52px] w-auto min-w-0 flex-1 justify-start gap-2 px-4 text-sm`
)

/** The scroll clearance a face's column reserves under the bar (52px + the
 *  bar's own safe-area padding + a gap), so the last row scrolls clear of
 *  the glass instead of ending under it. */
export const MOBILE_WORK_BAR_CLEARANCE = `pb-[calc(5.5rem+env(safe-area-inset-bottom))]`

export function MobileWorkCapsule({
  className,
  children,
  ...props
}: ComponentProps<`button`>) {
  return (
    <button
      type="button"
      className={cn(MOBILE_WORK_CAPSULE_CLASS, className)}
      {...props}
    >
      {children}
    </button>
  )
}

export function MobileWorkBar({
  leading,
  capsule,
  trailing,
  expanded,
  hidden = false,
}: {
  /** The left circle (Properties, the usage ring, GitHub) — or nothing. */
  leading?: ReactNode
  /** The centre capsule (`+ Comment`, the composer prompt, Merge PR). */
  capsule?: ReactNode
  /** The right circle: Start coding or the face switcher. Stays mounted
   *  while `expanded` shows. */
  trailing?: ReactNode
  /** A full-width node that REPLACES leading + capsule (the expanded
   *  composer). Null/undefined = the three-slot layout. */
  expanded?: ReactNode
  /** Hidden while another bottom-edge owner is up (the description editor's
   *  keyboard rail). */
  hidden?: boolean
}) {
  const isExpanded = expanded !== null && expanded !== undefined
  // EXP-568: while the composer is open the bar rides above the keyboard
  // (the layout viewport does not shrink under it on mobile).
  const { inset } = useKeyboardInset()
  if (hidden) return null
  return (
    <div
      data-testid="mobile-work-bar"
      className="pointer-events-none fixed inset-x-0 bottom-0 z-[35] flex items-end gap-2 px-4 pb-[max(1rem,env(safe-area-inset-bottom))] md:hidden"
      style={isExpanded && inset > 0 ? { bottom: inset } : undefined}
    >
      {isExpanded ? (
        <div className="pointer-events-auto min-w-0 flex-1 animate-in fade-in zoom-in-95 duration-fast ease-standard motion-reduce:animate-none">
          {expanded}
        </div>
      ) : (
        <>
          {leading}
          {capsule ?? <span className="min-w-0 flex-1" />}
        </>
      )}
      {/* `hidden` beats `contents` through tailwind-merge, so this is
          display:none while expanded and a transparent wrapper otherwise. */}
      <div className={cn(`contents`, isExpanded && `hidden`)}>{trailing}</div>
    </div>
  )
}
