// Round 1: shadcn's ScrollArea on Radix (the catalog's ScrollArea macro is a
// Box with overflowX/Y; this is the app-side primitive with styled bars).
export const SCROLL_AREA_CLASSES = {
  root: `relative`,
  viewport: `size-full rounded-[inherit] transition-[color,box-shadow] outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-1`,
  scrollbar: `flex touch-none p-px transition-colors select-none`,
  orientations: {
    vertical: `h-full w-2.5 border-l border-l-transparent`,
    horizontal: `h-2.5 flex-col border-t border-t-transparent`,
  },
  thumb: `relative flex-1 rounded-full bg-border`,
} as const
