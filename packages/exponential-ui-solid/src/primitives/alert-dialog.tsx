import { createContext, splitProps, useContext, type ComponentProps, type JSX } from "solid-js"
import { ALERT_DIALOG_CLASSES } from "@exponential-at/ui/web/primitives"

import {
  DialogClosePart,
  DialogContentPart,
  DialogDescriptionPart,
  DialogOverlayPart,
  DialogPortalPart,
  DialogRoot,
  DialogTitlePart,
  DialogTriggerPart,
  mergeRefs,
  type AsProps,
  type DialogContentPartProps,
  type DialogRootProps,
} from "../behaviors/dialog"
import { cn } from "./cn"

// Radix AlertDialog = a modal Dialog with `role="alertdialog"`, no outside
// dismissal, the focus on Cancel when it opens.

const CancelContext = createContext<{ cancel?: HTMLElement }>()

function AlertDialog(props: Omit<DialogRootProps, `modal`>) {
  return <DialogRoot {...props} modal />
}

function AlertDialogTrigger(props: AsProps) {
  return <DialogTriggerPart data-slot="alert-dialog-trigger" {...props} />
}

function AlertDialogPortal(props: { container?: HTMLElement | null; children?: JSX.Element }) {
  return <DialogPortalPart {...props} />
}

function AlertDialogOverlay(props: ComponentProps<`div`> & Record<string, unknown>) {
  const [local, rest] = splitProps(props, [`class`])
  return <DialogOverlayPart data-slot="alert-dialog-overlay" class={cn(ALERT_DIALOG_CLASSES.overlay, local.class)} {...rest} />
}

function AlertDialogContent(props: DialogContentPartProps & { class?: string; container?: HTMLElement | null }) {
  const [local, rest] = splitProps(props, [`class`, `container`, `onOpenAutoFocus`])
  const cancel: { cancel?: HTMLElement } = {}
  return (
    <>
      <AlertDialogPortal container={local.container}>
        <AlertDialogOverlay />
      </AlertDialogPortal>
      <AlertDialogPortal container={local.container}>
        <CancelContext.Provider value={cancel}>
          <DialogContentPart
            role="alertdialog"
            data-slot="alert-dialog-content"
            class={cn(ALERT_DIALOG_CLASSES.content, local.class)}
            {...rest}
            onOpenAutoFocus={(e: Event) => {
              local.onOpenAutoFocus?.(e)
              if (e.defaultPrevented) return
              e.preventDefault()
              cancel.cancel?.focus({ preventScroll: true })
            }}
            onPointerDownOutside={(e) => e.preventDefault()}
            onInteractOutside={(e) => e.preventDefault()}
          />
        </CancelContext.Provider>
      </AlertDialogPortal>
    </>
  )
}

function AlertDialogHeader(props: ComponentProps<`div`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <div data-slot="alert-dialog-header" class={cn(ALERT_DIALOG_CLASSES.header, local.class)} {...rest} />
}

function AlertDialogFooter(props: ComponentProps<`div`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <div data-slot="alert-dialog-footer" class={cn(ALERT_DIALOG_CLASSES.footer, local.class)} {...rest} />
}

function AlertDialogTitle(props: ComponentProps<`h2`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <DialogTitlePart data-slot="alert-dialog-title" class={cn(ALERT_DIALOG_CLASSES.title, local.class)} {...rest} />
}

function AlertDialogDescription(props: ComponentProps<`p`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <DialogDescriptionPart data-slot="alert-dialog-description" class={cn(ALERT_DIALOG_CLASSES.description, local.class)} {...rest} />
}

function AlertDialogAction(props: AsProps & { class?: string }) {
  const [local, rest] = splitProps(props, [`class`])
  return <DialogClosePart class={cn(ALERT_DIALOG_CLASSES.action, local.class)} {...rest} />
}

function AlertDialogCancel(props: AsProps & { class?: string }) {
  const [local, rest] = splitProps(props, [`class`, `ref`])
  const ctx = useContext(CancelContext)
  return <DialogClosePart class={cn(ALERT_DIALOG_CLASSES.cancel, local.class)} {...rest} ref={mergeRefs(local.ref, (el: HTMLElement) => ctx && (ctx.cancel = el))} />
}

export { AlertDialog, AlertDialogPortal, AlertDialogOverlay, AlertDialogTrigger, AlertDialogContent, AlertDialogHeader, AlertDialogFooter, AlertDialogTitle, AlertDialogDescription, AlertDialogAction, AlertDialogCancel }
