import type { ReactNode } from "react"
import { cn, conceptIcon } from "@exp/ui"

/**
 * What the dialog entries share (`blocked-start-dialog`,
 * `stack-merge-choice-dialog`).
 *
 * Not an entry itself (nothing in `sections.json` names it): just the frame.
 * `DialogContent` and `AlertDialogContent` are Radix portals, and a portal
 * renders nothing to static markup, so an entry draws the dialog's REAL parts
 * (`DialogHeader`, `DialogTitle`, `DialogFooter`, `Button`, ...) inside this
 * panel instead. The classes are the centred `sm` panel of
 * `packages/ui/src/dialog.tsx` (its `max-w-lg` width, its padding, its ✕),
 * minus its positioning and its animation. The page's demo canvas is
 * phone-width by default, so `styles.ts` widens it for each dialog entry to
 * the panel's real width.
 */

const CloseIcon = conceptIcon(`ui-close`)

export function DialogSpecimen({
  caption,
  showClose = false,
  className,
  children,
}: {
  /** One line above the panel naming the state it shows. */
  caption?: string
  /** `DialogContent` draws a ✕ top right; an alert dialog has none. */
  showClose?: boolean
  className?: string
  children: ReactNode
}) {
  return (
    <div className="grid gap-2">
      {caption !== undefined && (
        <p className="text-xs text-muted-foreground">{caption}</p>
      )}
      <div
        className={cn(
          `relative flex w-full max-w-lg flex-col gap-4 overflow-hidden rounded-2xl border border-glass-stroke-card bg-card/85 p-6 shadow-2xl shadow-black/40 backdrop-blur-2xl`,
          className
        )}
      >
        {children}
        {showClose && (
          <span className="absolute top-4 right-4 opacity-70">
            <CloseIcon className="size-4" />
          </span>
        )}
      </div>
    </div>
  )
}
