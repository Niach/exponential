import { buttonVariants } from "./button"

// Round 1: shadcn's AlertDialog on Radix — the confirm-or-cancel modal the
// catalog's AlertDialog macro also paints (no backdrop dismissal, Escape
// cancels, focus starts on Cancel).
export const ALERT_DIALOG_CLASSES = {
  overlay: `fixed inset-0 z-50 bg-black/50 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:duration-fast data-[state=closed]:ease-accelerate data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:duration-standard data-[state=open]:ease-decelerate`,
  content: `fixed top-[50%] left-[50%] z-50 grid w-full max-w-[calc(100%-2rem)] translate-x-[-50%] translate-y-[-50%] gap-4 rounded-lg border bg-background p-6 shadow-lg motion-dialog sm:max-w-lg`,
  header: `flex flex-col gap-2 text-center sm:text-left`,
  footer: `flex flex-col-reverse gap-2 sm:flex-row sm:justify-end`,
  title: `text-lg font-semibold`,
  description: `text-sm text-muted-foreground`,
  action: buttonVariants(),
  cancel: buttonVariants({ variant: `outline` }),
} as const
