import type { IconName } from "@exp/icons"
import { ICON_COMPONENTS } from "./icons.generated"
import { cn } from "./cn"

// EXP-887 — the PURE half of a status glyph: a registry icon painted either
// with a Tailwind token class (builtin statuses and priorities) or with an
// inline hex (EXP-314 custom rows). It knows nothing about teams, live queries
// or which row a given issue resolves to — the app's `StatusIcon` does that
// resolution and hands the answer down, so every presentational surface in
// this package (the issue chip today) can draw a status without reaching for
// the app's context.

export interface StatusGlyphProps {
  icon: IconName
  /** A `text-*` token class — builtin rows and the constructed fallbacks. */
  colorClass?: string
  /** A synced hex — custom rows. Wins over `colorClass`, like a label's. */
  colorHex?: string
  className?: string
}

export function StatusGlyph({
  icon,
  colorClass,
  colorHex,
  className,
}: StatusGlyphProps) {
  const Icon = ICON_COMPONENTS[icon]
  return (
    <Icon
      className={cn(colorClass, className)}
      style={colorHex ? { color: colorHex } : undefined}
    />
  )
}
