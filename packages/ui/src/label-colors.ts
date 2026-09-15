export const LABEL_COLORS = [
  `#ef4444`,
  `#dc2626`,
  `#f97316`,
  `#f59e0b`,
  `#eab308`,
  `#84cc16`,
  `#22c55e`,
  `#10b981`,
  `#14b8a6`,
  `#06b6d4`,
  `#0ea5e9`,
  `#3b82f6`,
  `#6366f1`,
  `#8b5cf6`,
  `#a855f7`,
  `#ec4899`,
  `#f43f5e`,
  `#78716c`,
  `#64748b`,
  `#a3a3a3`,
]

// The status picker's palette: the label palette plus white (EXP-685 — the
// retired builtin Todo was the one white status; a team that wants it back
// recreates it byte-identical). Status-only on purpose: labels and boards
// keep LABEL_COLORS, and the mobile palettes never manage statuses.
export const STATUS_COLORS = [...LABEL_COLORS, `#fafafa`]
