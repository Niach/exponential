import * as React from "react"
import { XIcon } from "lucide-react"
import { Dialog as SheetPrimitive } from "radix-ui"

import { cn } from "@/lib/utils"
import {
  SheetDragProvider,
  SheetGrabber,
  useSheetDragHandleProps,
} from "@/components/ui/sheet-chrome"
import { useSheetDrag } from "@/hooks/use-sheet-drag"

function Sheet({ ...props }: React.ComponentProps<typeof SheetPrimitive.Root>) {
  return <SheetPrimitive.Root data-slot="sheet" {...props} />
}

function SheetTrigger({
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Trigger>) {
  return <SheetPrimitive.Trigger data-slot="sheet-trigger" {...props} />
}

function SheetClose({
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Close>) {
  return <SheetPrimitive.Close data-slot="sheet-close" {...props} />
}

function SheetPortal({
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Portal>) {
  return <SheetPrimitive.Portal data-slot="sheet-portal" {...props} />
}

function SheetOverlay({
  className,
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Overlay>) {
  return (
    <SheetPrimitive.Overlay
      data-slot="sheet-overlay"
      className={cn(
        `fixed inset-0 z-50 bg-black/60 backdrop-blur-sm data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:animate-in data-[state=open]:fade-in-0`,
        className
      )}
      {...props}
    />
  )
}

function SheetContent({
  className,
  children,
  side = `right`,
  // A bottom sheet carries NO ✕ (EXP-687): it is dismissed by tapping outside
  // or dragging the grabber down, exactly like its iOS/Android twins. The side
  // and top drawers keep theirs.
  showCloseButton = side !== `bottom`,
  showGrabber = side === `bottom`,
  ref,
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Content> & {
  side?: `top` | `right` | `bottom` | `left`
  showCloseButton?: boolean
  /** The drag pill at the top edge. On by default for `bottom`; the two
   * page-like bottom sheets (New issue, search) turn it off. */
  showGrabber?: boolean
}) {
  const panelRef = React.useRef<HTMLDivElement | null>(null)
  const dragCloseRef = React.useRef<HTMLButtonElement | null>(null)
  const onDragDismiss = React.useCallback(() => {
    dragCloseRef.current?.click()
  }, [])
  const { handleProps } = useSheetDrag({ panelRef, onDismiss: onDragDismiss })
  const setPanel = React.useCallback(
    (node: HTMLDivElement | null) => {
      panelRef.current = node
      if (typeof ref === `function`) {
        ref(node)
      } else if (ref) {
        ref.current = node
      }
    },
    [ref]
  )

  return (
    <SheetPortal>
      {/* The overlay doubles as a Close trigger: Radix's outside-dismiss
          waits for a `click` after pointerdown, which iOS Safari never
          synthesizes on non-interactive elements — so bottom sheets were
          undismissable by tapping outside on mobile web (EXP-247).
          CAVEAT: this calls onOpenChange(false) on the Root directly, i.e.
          OUTSIDE the DismissableLayer, so a SheetContent's
          `onInteractOutside` + preventDefault no longer blocks an overlay
          tap. Every current consumer wants outside-dismiss, and the one
          guard that matters (issue-editor/dialog-shell.tsx) re-checks at
          the Root — but if you need to veto a dismissal, do it there, not
          with onInteractOutside.
          EXP-687's drag-to-dismiss takes the SAME Root path (it clicks the
          hidden Close below), so it is subject to the same caveat. */}
      <SheetPrimitive.Close asChild>
        <SheetOverlay />
      </SheetPrimitive.Close>
      <SheetPrimitive.Content
        data-slot="sheet-content"
        data-side={side}
        ref={setPanel}
        className={cn(
          // The mobile sheet radius (24px = rounded-*-3xl) on the leading
          // edge. Full-screen consumers (e.g. the search sheet) pass
          // rounded-none.
          `group/sheet fixed z-50 flex flex-col gap-4 border-glass-stroke-card shadow-lg transition ease-in-out data-[state=closed]:animate-out data-[state=closed]:duration-300 data-[state=open]:animate-in data-[state=open]:duration-500`,
          // Side/top drawers stay frosted glass (EXP-269); the BOTTOM sheet
          // is the opaque #18181B surface every client shares (EXP-687) — it
          // covers the screen edge to edge, so there is nothing to blur.
          side === `right` &&
            `inset-y-0 right-0 h-full w-3/4 rounded-l-3xl border-l bg-card/85 backdrop-blur-2xl data-[state=closed]:slide-out-to-right data-[state=open]:slide-in-from-right sm:max-w-sm`,
          side === `left` &&
            `inset-y-0 left-0 h-full w-3/4 rounded-r-3xl border-r bg-card/85 backdrop-blur-2xl data-[state=closed]:slide-out-to-left data-[state=open]:slide-in-from-left sm:max-w-sm`,
          side === `top` &&
            `inset-x-0 top-0 h-auto rounded-b-3xl border-b bg-card/85 backdrop-blur-2xl data-[state=closed]:slide-out-to-top data-[state=open]:slide-in-from-top`,
          side === `bottom` &&
            `inset-x-0 bottom-0 h-auto max-h-[90dvh] rounded-t-3xl border-t bg-glass-bottom data-[state=closed]:slide-out-to-bottom data-[state=open]:slide-in-from-bottom`,
          className
        )}
        {...props}
      >
        <SheetDragProvider value={side === `bottom` ? handleProps : null}>
          {showGrabber && (
            <>
              <SheetGrabber />
              <SheetPrimitive.Close
                ref={dragCloseRef}
                aria-hidden
                tabIndex={-1}
                className="hidden"
              />
            </>
          )}
          {children}
          {showCloseButton && (
            <SheetPrimitive.Close className="absolute top-4 right-4 rounded-xs opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:ring-2 focus:ring-ring focus:ring-offset-2 focus:outline-hidden disabled:pointer-events-none data-[state=open]:bg-secondary">
              <XIcon className="size-4" />
              <span className="sr-only">Close</span>
            </SheetPrimitive.Close>
          )}
        </SheetDragProvider>
      </SheetPrimitive.Content>
    </SheetPortal>
  )
}

function SheetHeader({ className, ...props }: React.ComponentProps<`div`>) {
  // Inside a bottom sheet the header area drags too, not just the grabber.
  const drag = useSheetDragHandleProps()
  return (
    <div
      data-slot="sheet-header"
      {...drag}
      className={cn(`flex flex-col gap-1.5 p-4 text-left`, className)}
      {...props}
    />
  )
}

function SheetFooter({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="sheet-footer"
      className={cn(`mt-auto flex flex-col gap-2 p-4`, className)}
      {...props}
    />
  )
}

function SheetTitle({
  className,
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Title>) {
  return (
    <SheetPrimitive.Title
      data-slot="sheet-title"
      // Headline weight and size on a phone (EXP-687): the same left-aligned
      // sheet title on web, iOS (`.headline`) and Android (`titleMedium`).
      className={cn(
        `font-semibold text-foreground max-sm:group-data-[side=bottom]/sheet:text-lg max-sm:group-data-[side=bottom]/sheet:leading-none`,
        className
      )}
      {...props}
    />
  )
}

function SheetDescription({
  className,
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Description>) {
  return (
    <SheetPrimitive.Description
      data-slot="sheet-description"
      className={cn(`text-sm text-muted-foreground`, className)}
      {...props}
    />
  )
}

export {
  Sheet,
  SheetTrigger,
  SheetClose,
  SheetContent,
  SheetHeader,
  SheetFooter,
  SheetTitle,
  SheetDescription,
}
