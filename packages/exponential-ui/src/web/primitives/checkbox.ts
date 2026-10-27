export const CHECKBOX_CLASSES = {
  // EXP-616: glass box — the strong stroke keeps a 16px unchecked square
  // legible against the row fill it sits on. 4px corner = the Exponential UI
  // Checkbox box (control-geometry.json); rounded-sm (8px) made it a circle.
  root: `peer size-4 shrink-0 rounded-[4px] border border-glass-stroke-strong bg-glass-row shadow-none transition-shadow outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-destructive/20 data-[state=checked]:border-primary data-[state=checked]:bg-primary data-[state=checked]:text-primary-foreground dark:aria-invalid:ring-destructive/40 dark:data-[state=checked]:bg-primary`,
  indicator: `grid place-content-center text-current transition-none`,
  icon: `size-3.5`,
} as const
