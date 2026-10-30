import * as React from "react"
import { SHEET_CHROME_CLASSES } from "@exponential_app/eui/web/primitives"

import { cn } from "./cn"
import type { SheetDragHandleProps } from "./use-sheet-drag"

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
      className={cn(SHEET_CHROME_CLASSES.grabber, className)}
    />
  )
}
