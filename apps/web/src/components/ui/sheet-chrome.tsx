import * as React from "react"

import { cn } from "@/lib/utils"
import type { SheetDragHandleProps } from "@/hooks/use-sheet-drag"

// EXP-687 — the shared chrome of every mobile bottom sheet, whether it was
// painted by ui/dialog.tsx's `sheet` arm or by ui/sheet.tsx's `bottom` side.
// One grabber, one drag gesture, no ✕: the sheet closes by tapping outside or
// by dragging it down (native parity across iOS/Android/web).
//
// Lives in its own module so both primitives can import it without a cycle.

const SheetDragContext = React.createContext<SheetDragHandleProps | null>(null)

export function SheetDragProvider({
  value,
  children,
}: {
  value: SheetDragHandleProps | null
  children: React.ReactNode
}) {
  return (
    <SheetDragContext.Provider value={value}>
      {children}
    </SheetDragContext.Provider>
  )
}

/**
 * The drag props of the enclosing sheet, or `null` outside one (a desktop
 * panel, a side drawer). Header components spread these so the whole title
 * area is draggable, not just the 4px bar.
 */
export function useSheetDragHandleProps(): SheetDragHandleProps | null {
  return React.useContext(SheetDragContext)
}

/** The pill at the top edge of a sheet. Purely decorative, but it drags. */
export function SheetGrabber({
  className,
  ...props
}: React.ComponentProps<`div`>) {
  const drag = useSheetDragHandleProps()
  return (
    <div
      data-slot="sheet-grabber"
      aria-hidden
      {...drag}
      {...props}
      className={cn(
        `mx-auto mt-2 -mb-1 h-1 w-9 shrink-0 rounded-full bg-foreground/25`,
        className
      )}
    />
  )
}
