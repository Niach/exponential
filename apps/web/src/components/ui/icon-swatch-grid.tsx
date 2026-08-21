import type { BoardIcon } from "@exp/db-schema/domain"
import { BOARD_ICON_OPTIONS } from "@/lib/board-icons"

interface IconSwatchGridProps {
  // Empty = nothing selected (IconPicker with `allowsNone`).
  value: BoardIcon | ``
  onChange: (icon: BoardIcon) => void
  // Tints the selected glyph with the board color for a live preview.
  color?: string
}

// Sibling of ColorSwatchGrid: the curated icon set as a swatch grid. Since
// EXP-575 it only renders inside `IconPicker`'s popover — forms show the slim
// trigger, never this grid inline. The search filter it once had was dropped
// in EXP-390: 60 glyphs scan faster than they search, on every platform.
export function IconSwatchGrid({ value, onChange, color }: IconSwatchGridProps) {
  return (
    <div className="flex flex-wrap gap-1.5">
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
