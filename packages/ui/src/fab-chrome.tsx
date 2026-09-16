// EXP-904 — the ONE floating-action chrome recipe.
//
// Four phone surfaces painted the same floating glass by copy-paste: the
// issue bar's coding circle, the Work bar's circles and capsule, the tab
// bar's FAB and its two-armed group, and the steer composer's tray. This is
// their common core — the card hairline, the 85% popover fill, the drop
// shadow and the backdrop blur. Everything a floating piece owns on its own
// (its size, its radius, its text colour, its layout) stays at the call site,
// because a 52px circle, a capsule and a rounded tray genuinely differ.
export const FAB_CHROME_CLASS = `border border-glass-stroke-card bg-popover/85 shadow-lg shadow-black/40 backdrop-blur-xl`
