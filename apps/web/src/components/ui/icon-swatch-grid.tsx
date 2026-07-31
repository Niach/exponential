import type { BoardIcon } from "@exp/db-schema/domain"
import { BOARD_ICON_OPTIONS } from "@/lib/board-icons"

interface IconSwatchGridProps {
  value: BoardIcon
  onChange: (icon: BoardIcon) => void
  // Tints the selected glyph with the board color for a live preview.
  color?: string
}

// Sibling of ColorSwatchGrid: the curated icon set as a swatch grid.
// EXP-273 grew the set from 16 to 60, so it gained a bounded scroll area — a
// flat wrap of 60 swatches pushed the dialog's action row off screen. The
// search filter it also gained was dropped again in EXP-390: 60 glyphs scan
// faster than they search, on every platform.
export function IconSwatchGrid({ value, onChange, color }: IconSwatchGridProps) {
  return (
    <div className="flex max-h-44 flex-wrap gap-1.5 overflow-y-auto">
      {BOARD_ICON_OPTIONS.map(({ name, icon: Icon }) => (
        <button
          key={name}
          type="button"
          aria-label={name}
          aria-pressed={value === name}
          title={name}
          className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-md border transition-all hover:scale-110 ${
            value === name
              ? `border-foreground bg-accent`
              : `border-border text-muted-foreground`
          }`}
          onClick={() => onChange(name)}
        >
          <Icon
            className="h-4 w-4"
            style={value === name && color ? { color } : undefined}
          />
        </button>
      ))}
    </div>
  )
}
