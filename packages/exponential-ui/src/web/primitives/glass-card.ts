// EXP-903 — the ONE glass-card surface recipe.
//
// Five web surfaces painted the same translucent card by copy-paste: the
// comment row, the agent AskCard, the agent usage bar, the mobile issue
// properties sheet and the repo picker (which had drifted to `rounded-md` and
// the bare `border` hairline). This is their common core — the XL radius, the
// card hairline and the glass fill. Everything a card owns on its own (its
// padding, its dividers, its overflow rule, a blur or a shadow) stays at the
// call site, because those genuinely differ per surface. `Card` in card.tsx
// derives its recipe from this constant; the natives already own the
// primitive (desktop `surface::glass_card`, iOS `GlassCard`, Android
// `Modifier.glassCard()`).
export const GLASS_CARD_CLASS = `rounded-xl border border-glass-stroke-card bg-glass-card`

export const GLASS_CARD_CLASSES = { root: GLASS_CARD_CLASS } as const
