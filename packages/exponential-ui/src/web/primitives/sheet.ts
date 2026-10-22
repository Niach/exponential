export type SheetSide = `top` | `right` | `bottom` | `left`

export const SHEET_CLASSES = {
  overlay: `fixed inset-0 z-50 bg-black/60 backdrop-blur-sm data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:duration-standard data-[state=closed]:ease-accelerate data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:duration-slow data-[state=open]:ease-decelerate`,
  // The mobile sheet radius (24px = rounded-*-3xl) on the leading
  // edge. Full-screen consumers (e.g. the search sheet) pass
  // rounded-none.
  content: `group/sheet fixed z-50 flex flex-col gap-4 border-glass-stroke-card shadow-lg motion-sheet`,
  // Side/top drawers stay frosted glass (EXP-269); the BOTTOM sheet
  // is the opaque #18181B surface every client shares (EXP-687) — it
  // covers the screen edge to edge, so there is nothing to blur.
  sides: {
    right: `inset-y-0 right-0 h-full w-3/4 rounded-l-3xl border-l bg-card/85 backdrop-blur-2xl data-[state=closed]:slide-out-to-right data-[state=open]:slide-in-from-right sm:max-w-sm`,
    left: `inset-y-0 left-0 h-full w-3/4 rounded-r-3xl border-r bg-card/85 backdrop-blur-2xl data-[state=closed]:slide-out-to-left data-[state=open]:slide-in-from-left sm:max-w-sm`,
    top: `inset-x-0 top-0 h-auto rounded-b-3xl border-b bg-card/85 backdrop-blur-2xl data-[state=closed]:slide-out-to-top data-[state=open]:slide-in-from-top`,
    bottom: `inset-x-0 bottom-0 h-auto max-h-[90dvh] rounded-t-3xl border-t bg-glass-bottom data-[state=closed]:slide-out-to-bottom data-[state=open]:slide-in-from-bottom`,
  } satisfies Record<SheetSide, string>,
  /** The hidden Close a drag-to-dismiss clicks. */
  dragClose: `hidden`,
  close: `absolute top-4 right-4 rounded-xs opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:ring-2 focus:ring-ring focus:ring-offset-2 focus:outline-hidden disabled:pointer-events-none data-[state=open]:bg-secondary`,
  closeIcon: `size-4`,
  srOnly: `sr-only`,
  header: `flex flex-col gap-1.5 p-4 text-left`,
  footer: `mt-auto flex flex-col gap-2 p-4`,
  // Headline weight and size on a phone (EXP-687): the same left-aligned
  // sheet title on web, iOS (`.headline`) and Android (`titleMedium`).
  title: `font-semibold text-foreground max-sm:group-data-[side=bottom]/sheet:text-lg max-sm:group-data-[side=bottom]/sheet:leading-none`,
  description: `text-sm text-muted-foreground`,
} as const
