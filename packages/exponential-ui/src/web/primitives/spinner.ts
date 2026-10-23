// EXP-1275 (EXP-1259 D8.1): THE inline spinner, feedback-states.json
// `inline.spinner` (10px) by default. The EXP-1274 list-head glyph (the
// GlassSectionHeader `loading` arm) IS this primitive; a pending Button swaps
// its icon slot for it at `pending.spinner` (12px). ×4: desktop
// `feedback::spinner`, iOS `ExpSpinner`, Android `Spinner`.
export const SPINNER_CLASSES = {
  root: `shrink-0 animate-spin text-muted-foreground`,
} as const
