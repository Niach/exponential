// Round 1: the catalog's Kbd macro as classes (the theme's `recipes.Kbd`):
// `$control.badge` tall, `$spacing.xs` side padding, `$radius.sm`, the
// `accent` fill with no border, caption (12/16) medium `mutedForeground`.
// A single key, or `KbdGroup` for a chord (`$spacing.xs` apart).
export const KBD_CLASSES = {
  root: `pointer-events-none inline-flex h-5 w-fit min-w-5 items-center justify-center gap-1 rounded-sm bg-accent px-1 font-sans text-xs font-medium text-muted-foreground select-none [&_svg:not([class*='size-'])]:size-3`,
  group: `inline-flex items-center gap-1`,
} as const
