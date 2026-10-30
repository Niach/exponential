import { Show, splitProps, type ComponentProps, type JSX } from "solid-js"
import { X } from "lucide-solid"
import { SHEET_CLASSES, isTypeaheadPortalInteraction, type SheetSide } from "@exponential_app/eui/web/primitives"

import {
  DialogClosePart,
  DialogContentPart,
  DialogDescriptionPart,
  DialogOverlayPart,
  DialogPortalPart,
  DialogRoot,
  DialogTitlePart,
  DialogTriggerPart,
  useDialogContext,
  callRef,
  type AsProps,
  type DialogContentPartProps,
  type DialogRootProps,
  type OutsideEvent,
} from "../behaviors/dialog"
import { PaintIcon } from "../icons"
import { cn } from "./cn"
import { SheetDragProvider, SheetGrabber, createSheetDragHandleProps } from "./sheet-chrome"
import { createSheetDrag } from "./use-sheet-drag"

function Sheet(props: DialogRootProps) {
  return <DialogRoot {...props} />
}

function SheetTrigger(props: AsProps) {
  return <DialogTriggerPart data-slot="sheet-trigger" {...props} />
}

function SheetClose(props: AsProps) {
  return <DialogClosePart data-slot="sheet-close" {...props} />
}

function SheetPortal(props: { container?: HTMLElement | null; children?: JSX.Element }) {
  return <DialogPortalPart {...props} />
}

function SheetOverlay(props: ComponentProps<`div`> & Record<string, unknown>) {
  const [local, rest] = splitProps(props, [`class`])
  return <DialogOverlayPart data-slot="sheet-overlay" class={cn(SHEET_CLASSES.overlay, local.class)} {...rest} />
}

export type SheetContentProps = DialogContentPartProps & {
  class?: string
  side?: SheetSide
  showCloseButton?: boolean
  /** The drag pill at the top edge. On by default for `bottom`; the two
   *  page-like bottom sheets (New issue, search) turn it off. */
  showGrabber?: boolean
  container?: HTMLElement | null
}

function SheetContent(props: SheetContentProps) {
  const [local, rest] = splitProps(props, [`class`, `children`, `side`, `showCloseButton`, `showGrabber`, `ref`, `onInteractOutside`, `container`])
  const ctx = useDialogContext(`SheetContent`)
  const side = () => local.side ?? `right`
  // A bottom sheet carries NO ✕ (EXP-687): it is dismissed by tapping outside
  // or dragging the grabber down. The side and top drawers keep theirs.
  const showCloseButton = () => local.showCloseButton ?? side() !== `bottom`
  const showGrabber = () => local.showGrabber ?? side() === `bottom`
  let panel: HTMLElement | undefined
  let dragClose: HTMLButtonElement | undefined
  // Dragging the sheet down clicks the hidden Close: the same Root-level path
  // as the overlay tap (EXP-247), outside the dismissable layer.
  const { handleProps } = createSheetDrag({ panel: () => panel, onDismiss: () => dragClose?.click() })
  return (
    <>
      {/* The overlay doubles as a Close (EXP-247): iOS Safari never
          synthesizes the `click` Radix's outside-dismiss waits for. */}
      <SheetPortal container={local.container}>
        <SheetOverlay type="button" onClick={() => ctx.setOpen(false)} />
      </SheetPortal>
      <SheetPortal container={local.container}>
        <DialogContentPart
          data-slot="sheet-content"
          data-side={side()}
          ref={(el: HTMLElement) => {
            panel = el
            callRef(local.ref, el)
          }}
          class={cn(SHEET_CLASSES.content, SHEET_CLASSES.sides[side()], local.class)}
          onInteractOutside={(event: OutsideEvent) => {
            local.onInteractOutside?.(event)
            if (!event.defaultPrevented && isTypeaheadPortalInteraction(event)) {
              event.preventDefault()
            }
          }}
          {...rest}
        >
          <SheetDragProvider value={side() === `bottom` ? handleProps : null}>
            <Show when={showGrabber()}>
              <SheetGrabber />
              <DialogClosePart ref={(el: HTMLButtonElement) => (dragClose = el)} aria-hidden="true" tabindex="-1" class={SHEET_CLASSES.dragClose} />
            </Show>
            {local.children}
            <Show when={showCloseButton()}>
              <DialogClosePart class={SHEET_CLASSES.close}>
                <PaintIcon icon={X} class={SHEET_CLASSES.closeIcon} />
                <span class={SHEET_CLASSES.srOnly}>Close</span>
              </DialogClosePart>
            </Show>
          </SheetDragProvider>
        </DialogContentPart>
      </SheetPortal>
    </>
  )
}

function SheetHeader(props: ComponentProps<`div`>) {
  const [local, rest] = splitProps(props, [`class`])
  // Inside a bottom sheet the header area drags too, not just the grabber.
  const drag = createSheetDragHandleProps()
  return <div data-slot="sheet-header" {...(drag ?? {})} class={cn(SHEET_CLASSES.header, local.class)} {...rest} />
}

function SheetFooter(props: ComponentProps<`div`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <div data-slot="sheet-footer" class={cn(SHEET_CLASSES.footer, local.class)} {...rest} />
}

function SheetTitle(props: ComponentProps<`h2`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <DialogTitlePart data-slot="sheet-title" class={cn(SHEET_CLASSES.title, local.class)} {...rest} />
}

function SheetDescription(props: ComponentProps<`p`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <DialogDescriptionPart data-slot="sheet-description" class={cn(SHEET_CLASSES.description, local.class)} {...rest} />
}

export { Sheet, SheetTrigger, SheetClose, SheetContent, SheetHeader, SheetFooter, SheetTitle, SheetDescription }
