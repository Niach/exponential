// Round 1: shadcn's Kbd (the key-cap hint the catalog's Kbd macro paints the
// same way) — a single key, or `KbdGroup` for a chord.
export const KBD_CLASSES = {
  root: `pointer-events-none inline-flex h-5 w-fit min-w-5 items-center justify-center gap-1 rounded-sm bg-muted px-1 font-sans text-xs font-medium text-muted-foreground select-none [&_svg:not([class*='size-'])]:size-3`,
  group: `inline-flex items-center gap-1`,
} as const
