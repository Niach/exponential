import { createContext, splitProps, useContext, type ComponentProps, type JSX } from "solid-js"
import { SHEET_CHROME_CLASSES } from "@exponential_app/eui/web/primitives"

import { cn } from "./cn"
import type { SheetDragHandleProps } from "./use-sheet-drag"

// Lives in its own module so both primitives can import it without a cycle.

const SheetDragContext = createContext<SheetDragHandleProps | null>(null)

export function SheetDragProvider(props: { value: SheetDragHandleProps | null; children?: JSX.Element }) {
  return <SheetDragContext.Provider value={props.value}>{props.children}</SheetDragContext.Provider>
}

/** The drag props of the enclosing sheet, or `null` outside one (a desktop
 *  panel, a side drawer). Header components spread these so the whole title
 *  area is draggable, not just the 4px bar. */
export function createSheetDragHandleProps(): SheetDragHandleProps | null {
  return useContext(SheetDragContext)
}

export function SheetGrabber(props: ComponentProps<`div`>) {
  const [local, rest] = splitProps(props, [`class`])
  const drag = createSheetDragHandleProps()
  return <div data-slot="sheet-grabber" aria-hidden="true" {...(drag ?? {})} {...rest} class={cn(SHEET_CHROME_CLASSES.grabber, local.class)} />
}
