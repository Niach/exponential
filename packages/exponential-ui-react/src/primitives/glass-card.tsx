import * as React from "react"

import { cn } from "./cn"

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
// `Modifier.glassCard()`). N-12 (owner 2026-10-22): ONE gray ×4: the fill
// is the opaque card composite (`bg-glass-card-opaque`), never the
// translucent wash, so every card reads the same tone wherever it sits.
export const GLASS_CARD_CLASS = `rounded-xl border border-glass-stroke-card bg-glass-card-opaque`

export function GlassCard({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="glass-card"
      className={cn(GLASS_CARD_CLASS, className)}
      {...props}
    />
  )
}
