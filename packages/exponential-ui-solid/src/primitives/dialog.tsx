import { Show, createContext, splitProps, useContext, type ComponentProps, type JSX } from "solid-js"
import { X } from "lucide-solid"
import { DIALOG_CLASSES, DIALOG_PHONE_QUERY, isTypeaheadPortalInteraction } from "@exponential_app/ui/web/primitives"

import {
  DialogClosePart,
  DialogContentPart,
  DialogDescriptionPart,
  DialogOverlayPart,
  DialogPortalPart,
  DialogRoot,
  DialogTitlePart,
  DialogTriggerPart,
  callRef,
  type AsProps,
  type DialogContentPartProps,
  type DialogRootProps,
  type OutsideEvent,
} from "../behaviors/dialog"
import { PaintIcon } from "../icons"
import { createMediaQuery } from "../platform"
import { Button, type ButtonProps } from "./button"
import { cn } from "./cn"
import { MOBILE_ARMS, type DialogMobileArm } from "./dialog-arms"
import { SheetDragProvider, SheetGrabber, createSheetDragHandleProps } from "./sheet-chrome"
import { SheetContent } from "./sheet"
import { createSheetDrag } from "./use-sheet-drag"

function Dialog(props: DialogRootProps) {
  return <DialogRoot {...props} />
}

function DialogTrigger(props: AsProps) {
  return <DialogTriggerPart data-slot="dialog-trigger" {...props} />
}

function DialogPortal(props: { container?: HTMLElement | null; children?: JSX.Element }) {
  return <DialogPortalPart {...props} />
}

function DialogClose(props: AsProps) {
  return <DialogClosePart data-slot="dialog-close" {...props} />
}

function DialogOverlay(props: ComponentProps<`div`> & Record<string, unknown>) {
  const [local, rest] = splitProps(props, [`class`])
  return <DialogOverlayPart data-slot="dialog-overlay" class={cn(DIALOG_CLASSES.overlay, local.class)} {...rest} />
}

/** The `< sm` presentation of the enclosing dialog, for chrome that differs
 *  per arm (the ✕, DialogCancel). `null` outside a DialogContent. */
const DialogPresentationContext = createContext<DialogMobileArm | null>(null)

export type DialogContentProps = DialogContentPartProps & {
  class?: string
  showCloseButton?: boolean
  /** How the dialog presents below `sm` (EXP-687). Defaults to the bottom
   *  `sheet`; the `sm:` presentation is identical for every arm. */
  mobile?: DialogMobileArm
  container?: HTMLElement | null
}

function DialogContent(props: DialogContentProps) {
  const [local, rest] = splitProps(props, [`class`, `children`, `showCloseButton`, `mobile`, `ref`, `onInteractOutside`, `container`])
  const mobile = () => local.mobile ?? `sheet`
  const isSheet = () => mobile() === `sheet` || mobile() === `sheet-full`
  const isPhone = createMediaQuery(DIALOG_PHONE_QUERY)
  let panel: HTMLElement | undefined
  let dragClose: HTMLButtonElement | undefined
  // Dragging the sheet down clicks a hidden Close: the same Root-level path
  // as the EXP-247 overlay tap, NOT the dismissable layer, so
  // `onInteractOutside` + preventDefault does not veto it.
  const { handleProps } = createSheetDrag({ panel: () => panel, onDismiss: () => dragClose?.click() })
  const setRef = (el: HTMLElement) => callRef(local.ref, el)

  return (
    <Show
      when={isSheet() && isPhone()}
      fallback={
        <>
          <DialogPortal container={local.container}>
            <DialogOverlay />
          </DialogPortal>
          <DialogPortal container={local.container}>
            <DialogContentPart
              data-slot="dialog-content"
              data-mobile={mobile()}
              ref={(el: HTMLElement) => {
                panel = el
                setRef(el)
              }}
              class={cn(DIALOG_CLASSES.content, MOBILE_ARMS[mobile()], local.class)}
              // The typeahead's anchored arm portals to document.body (EXP-54):
              // a pointer-down on it is not "outside"; the caller's guard runs
              // first and its veto stands.
              onInteractOutside={(event: OutsideEvent) => {
                local.onInteractOutside?.(event)
                if (!event.defaultPrevented && isTypeaheadPortalInteraction(event)) {
                  event.preventDefault()
                }
              }}
              {...rest}
            >
              <DialogPresentationContext.Provider value={mobile()}>
                <SheetDragProvider value={isSheet() ? handleProps : null}>
                  <Show when={isSheet()}>
                    <SheetGrabber class={DIALOG_CLASSES.grabber} />
                    <DialogClosePart ref={(el: HTMLButtonElement) => (dragClose = el)} aria-hidden="true" tabindex="-1" class={DIALOG_CLASSES.dragClose} />
                  </Show>
                  {local.children}
                  <Show when={local.showCloseButton ?? true}>
                    <DialogClosePart data-slot="dialog-close" class={cn(DIALOG_CLASSES.close, mobile() !== `page` && DIALOG_CLASSES.closePhoneHidden)}>
                      <PaintIcon icon={X} />
                      <span class={DIALOG_CLASSES.srOnly}>Close</span>
                    </DialogClosePart>
                  </Show>
                </SheetDragProvider>
              </DialogPresentationContext.Provider>
            </DialogContentPart>
          </DialogPortal>
        </>
      }
    >
      {/* ONE phone sheet: the dialog renders THROUGH `SheetContent` (the same
          Dialog underneath, so this Root still owns it). */}
      <SheetContent
        side="bottom"
        data-slot="dialog-content"
        data-mobile={mobile()}
        ref={setRef}
        class={cn(DIALOG_CLASSES.phoneSheet, mobile() === `sheet-full` && DIALOG_CLASSES.phoneSheetFull, local.class)}
        onInteractOutside={local.onInteractOutside}
        container={local.container}
        {...rest}
      >
        <DialogPresentationContext.Provider value={mobile()}>{local.children}</DialogPresentationContext.Provider>
      </SheetContent>
    </Show>
  )
}

function DialogHeader(props: ComponentProps<`div`>) {
  const [local, rest] = splitProps(props, [`class`])
  // Inside a sheet the whole header area drags, not just the 4px grabber.
  const drag = createSheetDragHandleProps()
  return <div data-slot="dialog-header" {...(drag ?? {})} class={cn(DIALOG_CLASSES.header, local.class)} {...rest} />
}

function DialogBody(props: ComponentProps<`div`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <div data-slot="dialog-body" class={cn(DIALOG_CLASSES.body, local.class)} {...rest} />
}

function DialogFooter(props: ComponentProps<`div`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <div data-slot="dialog-footer" class={cn(DIALOG_CLASSES.footer, local.class)} {...rest} />
}

/** The Cancel of a dialog footer (EXP-687): hidden on a phone for the sheet
 *  arms (a sheet is dismissed by dragging it down or tapping outside),
 *  visible for `alert`/`page`. Desktop: a ghost button. */
function DialogCancel(props: ButtonProps) {
  const [local, rest] = splitProps(props, [`class`, `children`])
  const mobile = useContext(DialogPresentationContext)
  const keepOnMobile = mobile === `alert` || mobile === `page`
  return (
    <DialogClosePart
      as={Button}
      type="button"
      variant="ghost"
      // Keeps Button's `data-slot="button"`, which DialogFooter's phone
      // sizing keys on; this attribute only names the role.
      data-dialog-cancel=""
      class={cn(!keepOnMobile && DIALOG_CLASSES.cancelPhoneHidden, local.class)}
      {...rest}
    >
      {local.children ?? `Cancel`}
    </DialogClosePart>
  )
}

function DialogTitle(props: ComponentProps<`h2`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <DialogTitlePart data-slot="dialog-title" class={cn(DIALOG_CLASSES.title, local.class)} {...rest} />
}

function DialogDescription(props: ComponentProps<`p`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <DialogDescriptionPart data-slot="dialog-description" class={cn(DIALOG_CLASSES.description, local.class)} {...rest} />
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
