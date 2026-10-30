import * as React from "react"
import { XIcon } from "lucide-react"
import { Dialog as SheetPrimitive } from "radix-ui"
import { SHEET_CLASSES, isTypeaheadPortalInteraction, type SheetSide } from "@exponential_app/eui/web/primitives"

import { cn } from "./cn"
import {
  SheetDragProvider,
  SheetGrabber,
  useSheetDragHandleProps,
} from "./sheet-chrome"
import { useSheetDrag } from "./use-sheet-drag"

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
      className={cn(SHEET_CLASSES.overlay, className)}
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
  onInteractOutside,
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Content> & {
  side?: SheetSide
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
        className={cn(SHEET_CLASSES.content, SHEET_CLASSES.sides[side], className)}
        // A pointer-down on the typeahead's body portal is not "outside"
        // (see DialogContent); the caller's own guard runs first.
        onInteractOutside={(event) => {
          onInteractOutside?.(event)
          if (!event.defaultPrevented && isTypeaheadPortalInteraction(event)) {
            event.preventDefault()
          }
        }}
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
                className={SHEET_CLASSES.dragClose}
              />
            </>
          )}
          {children}
          {showCloseButton && (
            <SheetPrimitive.Close className={SHEET_CLASSES.close}>
              <XIcon className={SHEET_CLASSES.closeIcon} />
              <span className={SHEET_CLASSES.srOnly}>Close</span>
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
      className={cn(SHEET_CLASSES.header, className)}
      {...props}
    />
  )
}

function SheetFooter({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="sheet-footer"
      className={cn(SHEET_CLASSES.footer, className)}
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
      className={cn(SHEET_CLASSES.title, className)}
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
      className={cn(SHEET_CLASSES.description, className)}
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
