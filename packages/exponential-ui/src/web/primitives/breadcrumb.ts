// Round 1: shadcn's Breadcrumb (the catalog's Breadcrumb macro paints the
// same trail: links, a separator glyph, the current page last).
export const BREADCRUMB_CLASSES = {
  // The SDK sheet ships no preflight: the list resets its own markers.
  list: `flex list-none flex-wrap items-center gap-1.5 text-sm break-words text-muted-foreground sm:gap-2.5`,
  item: `inline-flex items-center gap-1.5`,
  link: `transition-colors hover:text-foreground`,
  page: `font-normal text-foreground`,
  separator: `[&>svg]:size-3.5`,
  ellipsis: `flex size-9 items-center justify-center`,
  ellipsisIcon: `size-4`,
  srOnly: `sr-only`,
} as const
