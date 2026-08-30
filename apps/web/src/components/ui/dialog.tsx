import * as React from "react"
import { XIcon } from "lucide-react"
import { Dialog as DialogPrimitive } from "radix-ui"

import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { MOBILE_ARMS, type DialogMobileArm } from "@/components/ui/dialog-arms"
import {
  SheetDragProvider,
  SheetGrabber,
  useSheetDragHandleProps,
} from "@/components/ui/sheet-chrome"
import { useSheetDrag } from "@/hooks/use-sheet-drag"

function Dialog({
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Root>) {
  return <DialogPrimitive.Root data-slot="dialog" {...props} />
}

function DialogTrigger({
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Trigger>) {
  return <DialogPrimitive.Trigger data-slot="dialog-trigger" {...props} />
}

function DialogPortal({
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Portal>) {
  return <DialogPrimitive.Portal data-slot="dialog-portal" {...props} />
}

function DialogClose({
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Close>) {
  return <DialogPrimitive.Close data-slot="dialog-close" {...props} />
}

function DialogOverlay({
  className,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Overlay>) {
  return (
    <DialogPrimitive.Overlay
      data-slot="dialog-overlay"
      className={cn(
        `fixed inset-0 z-50 bg-black/60 backdrop-blur-sm data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:animate-in data-[state=open]:fade-in-0`,
        className
      )}
      {...props}
    />
  )
}

/** The `< sm` presentation of the enclosing dialog, for chrome that differs
 * per arm (the ✕, DialogCancel). `null` outside a DialogContent. */
const DialogPresentationContext = React.createContext<DialogMobileArm | null>(
  null
)

function DialogContent({
  className,
  children,
  showCloseButton = true,
  mobile = `sheet`,
  ref,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Content> & {
  showCloseButton?: boolean
  /** How the dialog presents below `sm` (EXP-687). Defaults to the bottom
   * `sheet`; the `sm:` presentation is identical for every arm. See
   * ui/dialog-arms.ts for what each one means. */
  mobile?: DialogMobileArm
}) {
  const isSheet = mobile === `sheet` || mobile === `sheet-full`
  const panelRef = React.useRef<HTMLDivElement | null>(null)
  const dragCloseRef = React.useRef<HTMLButtonElement | null>(null)

  // Dragging the sheet down clicks a hidden Close — i.e. it takes the same
  // Root-level path as the EXP-247 overlay tap, NOT the DismissableLayer, so
  // `onInteractOutside` + preventDefault does not veto it. A dialog that must
  // guard its dismissal (issue-editor/dialog-shell.tsx) re-checks at the Root.
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
    <DialogPortal data-slot="dialog-portal">
      <DialogOverlay />
      <DialogPrimitive.Content
        data-slot="dialog-content"
        data-mobile={mobile}
        ref={setPanel}
        className={cn(
          // The panel is a flex COLUMN that never scrolls itself (EXP-369):
          // header and footer stay pinned and the scrolling happens inside
          // `DialogBody` (`flex-1 min-h-0 overflow-y-auto`). Any dialog whose
          // content can outgrow the viewport MUST wrap that content in a
          // DialogBody — without one, tall content is clipped rather than
          // scrolled (short confirm dialogs only hit that below ~200px of
          // viewport height).
          // Below `sm` the presentation is one of the four arms in
          // ui/dialog-arms.ts, `sheet` by default (EXP-687): a bottom sheet
          // with a grabber and no ✕, dismissed by dragging it down or tapping
          // outside. From `sm` up it is the centered frosted panel (EXP-269),
          // capped to the viewport — THE SAME for every arm, which is why the
          // arms own all the padding and zoom. Callers that reposition or
          // re-cap the panel must sm:-prefix those classes so they compose
          // with (and tailwind-merge away) the sm: base here.
          // A column flex container also fixes EXP-178 for free: items take
          // the container's definite width and long nowrap lines (e.g. an
          // issue title in a picker row) overflow their item instead of
          // inflating the panel the way a grid track's min-content did.
          // sm:w-[calc(100%-2rem)] keeps the 1rem side gutter the old
          // unprefixed max-w-[calc(100%-2rem)] used to give, without sitting in
          // the max-w-* tailwind-merge group — so a caller's sm:max-w-* still
          // caps the panel instead of dropping the gutter.
          `fixed z-50 flex flex-col gap-4 overflow-hidden shadow-lg duration-200 outline-none data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:animate-in data-[state=open]:fade-in-0 sm:inset-auto sm:top-[50%] sm:left-[50%] sm:max-h-[calc(100dvh-2rem)] sm:w-[calc(100%-2rem)] sm:max-w-lg sm:translate-x-[-50%] sm:translate-y-[-50%] sm:rounded-2xl sm:border sm:border-glass-stroke-card sm:bg-card/85 sm:shadow-2xl sm:shadow-black/40 sm:backdrop-blur-2xl`,
          MOBILE_ARMS[mobile],
          className
        )}
        {...props}
      >
        <DialogPresentationContext.Provider value={mobile}>
          <SheetDragProvider value={isSheet ? handleProps : null}>
            {isSheet && (
              <>
                <SheetGrabber className="hidden max-sm:block" />
                <DialogPrimitive.Close
                  ref={dragCloseRef}
                  aria-hidden
                  tabIndex={-1}
                  className="hidden"
                />
              </>
            )}
            {children}
            {showCloseButton && (
              <DialogPrimitive.Close
                data-slot="dialog-close"
                className={cn(
                  `absolute top-4 right-4 rounded-xs opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:ring-2 focus:ring-ring focus:ring-offset-2 focus:outline-hidden disabled:pointer-events-none data-[state=open]:bg-accent data-[state=open]:text-muted-foreground [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4`,
                  // Sheets and alerts carry no ✕ on a phone (EXP-687); the
                  // full-screen page arm is the one that still needs it.
                  mobile !== `page` && `max-sm:hidden`
                )}
              >
                <XIcon />
                <span className="sr-only">Close</span>
              </DialogPrimitive.Close>
            )}
          </SheetDragProvider>
        </DialogPresentationContext.Provider>
      </DialogPrimitive.Content>
    </DialogPortal>
  )
}

function DialogHeader({ className, ...props }: React.ComponentProps<`div`>) {
  // Inside a sheet the whole header area drags, not just the 4px grabber.
  const drag = useSheetDragHandleProps()
  return (
    <div
      data-slot="dialog-header"
      {...drag}
      // Left-aligned on every viewport (EXP-687): a centered mobile title
      // reads as an alert, and sheets are not alerts.
      className={cn(`flex flex-col gap-2 text-left`, className)}
      {...props}
    />
  )
}

// The one scrolling region of a dialog. Everything that can grow — a form's
// fields, a long list, a wall of explanatory copy — goes in here so the header
// above it and the footer below it stay pinned to the panel edges. Short
// content just doesn't scroll: flex-1 only claims free space when the panel is
// actually height-capped.
function DialogBody({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="dialog-body"
      className={cn(`min-h-0 flex-1 overflow-y-auto`, className)}
      {...props}
    />
  )
}

function DialogFooter({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="dialog-footer"
      className={cn(
        // On a phone every action is the full-width `lg` capsule pinned to
        // the bottom of the sheet (EXP-687) — one button shape across web,
        // iOS `GlassSubmitButton` and Android `GlassSubmitButton`.
        `flex flex-col-reverse gap-2 max-sm:[&>[data-slot=button]]:h-10 max-sm:[&>[data-slot=button]]:w-full max-sm:[&>[data-slot=button]]:px-6 sm:flex-row sm:justify-end`,
        className
      )}
      {...props}
    />
  )
}

/**
 * The Cancel of a dialog footer (EXP-687). Hidden on a phone for the sheet
 * arms — a sheet is dismissed by dragging it down or tapping outside, so a
 * Cancel button is redundant chrome — and visible for `alert`/`page`, where
 * native platforms keep it. Desktop is unchanged: a ghost button.
 */
function DialogCancel({
  className,
  children = `Cancel`,
  ...props
}: React.ComponentProps<typeof Button>) {
  const mobile = React.useContext(DialogPresentationContext)
  const keepOnMobile = mobile === `alert` || mobile === `page`
  return (
    <DialogPrimitive.Close asChild>
      <Button
        type="button"
        variant="ghost"
        // Keeps Button's `data-slot="button"`, which DialogFooter's phone
        // sizing keys on; this attribute only names the role.
        data-dialog-cancel=""
        className={cn(!keepOnMobile && `max-sm:hidden`, className)}
        {...props}
      >
        {children}
      </Button>
    </DialogPrimitive.Close>
  )
}

function DialogTitle({
  className,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Title>) {
  return (
    <DialogPrimitive.Title
      data-slot="dialog-title"
      className={cn(`text-lg leading-none font-semibold`, className)}
      {...props}
    />
  )
}

function DialogDescription({
  className,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Description>) {
  return (
    <DialogPrimitive.Description
      data-slot="dialog-description"
      className={cn(`text-sm text-muted-foreground`, className)}
      {...props}
    />
  )
}

export {
  Dialog,
  DialogBody,
  DialogCancel,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
  DialogPortal,
  DialogPresentationContext,
  DialogTitle,
  DialogTrigger,
}
