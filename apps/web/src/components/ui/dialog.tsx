import * as React from "react"
import { XIcon } from "lucide-react"
import { Dialog as DialogPrimitive } from "radix-ui"

import { cn } from "@/lib/utils"

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

// Below `sm` a dialog is one of two things. The DEFAULT is the full-screen
// page (EXP-255) every dialog has had since then. `mobileSheet` opts into the
// iOS presentation instead (EXP-616): a large-detent bottom sheet — rounded
// top edge, the page peeking behind it, sliding up from the bottom. Both arms
// carry their OWN mobile-only classes so nothing leaks between them; from `sm`
// up the two are identical (the shared string below), which is why the sheet
// arm re-states the `sm:` padding and zoom the page arm owns unprefixed.
const MOBILE_PAGE = `inset-0 bg-background p-6 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95`
const MOBILE_SHEET = `max-sm:inset-x-0 max-sm:bottom-0 max-sm:max-h-[92dvh] max-sm:rounded-t-3xl max-sm:border-t max-sm:border-glass-stroke-card max-sm:bg-card/85 max-sm:p-4 max-sm:pb-[max(1rem,env(safe-area-inset-bottom))] max-sm:backdrop-blur-2xl max-sm:data-[state=closed]:slide-out-to-bottom-1/2 max-sm:data-[state=open]:slide-in-from-bottom-1/2 sm:p-6 sm:data-[state=closed]:zoom-out-95 sm:data-[state=open]:zoom-in-95`

function DialogContent({
  className,
  children,
  showCloseButton = true,
  mobileSheet = false,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Content> & {
  showCloseButton?: boolean
  /** Present the dialog as a bottom sheet below `sm` instead of a full-screen
   * page (EXP-616). Opt-in — the `sm:` presentation is unchanged either way. */
  mobileSheet?: boolean
}) {
  return (
    <DialogPortal data-slot="dialog-portal">
      <DialogOverlay />
      <DialogPrimitive.Content
        data-slot="dialog-content"
        className={cn(
          // The panel is a flex COLUMN that never scrolls itself (EXP-369):
          // header and footer stay pinned and the scrolling happens inside
          // `DialogBody` (`flex-1 min-h-0 overflow-y-auto`). Any dialog whose
          // content can outgrow the viewport MUST wrap that content in a
          // DialogBody — without one, tall content is clipped rather than
          // scrolled (short confirm dialogs only hit that below ~200px of
          // viewport height).
          // Below `sm` every dialog is still a full-screen page (EXP-255):
          // inset-0, no rounding/border — the same flex column, just against
          // a definite 100dvh height, so the body scrolls and the action
          // buttons stay reachable at the bottom of the phone screen; opting
          // into `mobileSheet` swaps that arm for a bottom sheet whose height
          // follows its content up to 92dvh (see MOBILE_PAGE/MOBILE_SHEET
          // above). From `sm` up it is the centered panel, capped to the
          // viewport — the same for both arms. Callers
          // that reposition or re-cap the panel must sm:-prefix those classes
          // so they compose with (and tailwind-merge away) the sm: base here.
          // A column flex container also fixes EXP-178 for free: items take
          // the container's definite width and long nowrap lines (e.g. an
          // issue title in a picker row) overflow their item instead of
          // inflating the panel the way a grid track's min-content did.
          // sm:w-[calc(100%-2rem)] keeps the 1rem side gutter the old
          // unprefixed max-w-[calc(100%-2rem)] used to give, without sitting in
          // the max-w-* tailwind-merge group — so a caller's sm:max-w-* still
          // caps the panel instead of dropping the gutter.
          // From `sm` up the panel is frosted glass (EXP-269): translucent
          // card surface + backdrop blur + hairline. Below `sm` the page arm
          // stays opaque — no phone-sized blur cost, and flat #0a0a0a is
          // indistinguishable from the gradient top; only the sheet arm, which
          // deliberately shows the page behind it, pays for the blur.
          `fixed z-50 flex w-full flex-col gap-4 overflow-hidden shadow-lg duration-200 outline-none data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:animate-in data-[state=open]:fade-in-0 sm:inset-auto sm:top-[50%] sm:left-[50%] sm:max-h-[calc(100dvh-2rem)] sm:w-[calc(100%-2rem)] sm:max-w-lg sm:translate-x-[-50%] sm:translate-y-[-50%] sm:rounded-2xl sm:border sm:border-glass-stroke-card sm:bg-card/85 sm:shadow-2xl sm:shadow-black/40 sm:backdrop-blur-2xl`,
          mobileSheet ? MOBILE_SHEET : MOBILE_PAGE,
          className
        )}
        {...props}
      >
        {children}
        {showCloseButton && (
          <DialogPrimitive.Close
            data-slot="dialog-close"
            className="absolute top-4 right-4 rounded-xs opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:ring-2 focus:ring-ring focus:ring-offset-2 focus:outline-hidden disabled:pointer-events-none data-[state=open]:bg-accent data-[state=open]:text-muted-foreground [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4"
          >
            <XIcon />
            <span className="sr-only">Close</span>
          </DialogPrimitive.Close>
        )}
      </DialogPrimitive.Content>
    </DialogPortal>
  )
}

function DialogHeader({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="dialog-header"
      className={cn(`flex flex-col gap-2 text-center sm:text-left`, className)}
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
        `flex flex-col-reverse gap-2 sm:flex-row sm:justify-end`,
        className
      )}
      {...props}
    />
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
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
  DialogPortal,
  DialogTitle,
  DialogTrigger,
}
